use std::{net::SocketAddr, time::Duration};

use error::Result;

use futures::TryFutureExt;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
    time::timeout,
};
use tracing::{debug, error, instrument};
use tunnel::SocketSpec;

pub use state::State;
pub use tunnel::Tunnel;

use crate::aio::copy_bidirectional;

mod aio;
pub mod config;
pub mod controller;
pub mod error;
mod state;
pub mod tunnel;

#[instrument(skip_all, fields(client=%local_client))]
async fn process_socket(
    mut socket: TcpStream,
    local_client: SocketAddr,
    tunnel_key: SocketSpec,
    state: State,
    finish_receiver: watch::Receiver<bool>,
) -> Result<()> {
    debug!("Client connected");
    state.client_connected(&tunnel_key, &local_client);
    let mut last_remote = None;
    let mut retries = state.remote_retries(&tunnel_key)?;
    while retries > 0 {
        match state.select_remote(&tunnel_key) {
            Ok((remote, options)) => {
                debug!(remote=%remote, "Selected remote");
                match timeout(
                    Duration::from_secs_f32(options.remote_connect_timeout),
                    TcpStream::connect(remote.as_tuple()),
                )
                .await
                {
                    Ok(Ok(mut stream)) => {
                        state.remote_connected(&tunnel_key, &remote, &local_client);
                        last_remote = Some(remote.clone());
                        match copy_bidirectional(
                            &mut socket,
                            &mut stream,
                            tunnel_key.clone(),
                            remote,
                            local_client,
                            state.clone(),
                            finish_receiver,
                        )
                        .await
                        {
                            Ok((_sent, _received)) => {
                                // state.update_stats(&tunnel.local, received, sent, remote_client.as_ref());
                            }
                            Err(e) => {
                                error!(error=%e, "Error copying between streams")
                            }
                        };
                        break;
                    }
                    Ok(Err(e)) => {
                        state.remote_error(&tunnel_key, &remote, &local_client, &options);
                        error!(error=%e, remote=%remote,
                            "Error while connecting to remote");
                    }
                    Err(_) => {
                        state.remote_error(&tunnel_key, &remote, &local_client, &options);
                        error!( remote=%remote,
                            "Timeout while connecting to remote");
                    }
                }
            }
            Err(e) => {
                error!(error=%e, "Cannot get available remote");
                last_remote = None;
                break;
            }
        };
        retries -= 1;
        debug!("Retrying to connect remote");
    }
    if retries == 0 {
        error!("Closing connection in tunnel as cannot establish connection to remote end");
    }
    state.client_disconnected(&tunnel_key, last_remote.as_ref(), &local_client);
    debug!("Client disconnected");
    Ok(())
}

pub(crate) struct TunnelHandler {
    state: State,
    tunnel_key: SocketSpec,
    listener: TcpListener,
    close_channel: watch::Receiver<bool>,
}

pub fn stop_tunnel(local: &SocketSpec, state: State) -> Result<()> {
    let tunnel_info = state.remove_tunnel(local)?;
    if let Err(e) = tunnel_info.close_channel.send(true) {
        error!(tunnel=%local, error=%e, "Cannot close tunnel")
    }
    Ok(())
}

pub async fn start_tunnel(tunnel: Tunnel, state: State) -> Result<JoinHandle<()>> {
    let handler = create_tunnel(tunnel, state).await?;
    Ok(tokio::spawn(run_tunnel(handler)))
}

async fn create_tunnel(tunnel: Tunnel, state: State) -> Result<TunnelHandler> {
    let listener = TcpListener::bind(tunnel.local.as_tuple()).await?;
    let (sender, receiver) = watch::channel(false);
    let tunnel_key = tunnel.local.clone();
    state.add_tunnel(tunnel, sender)?;
    Ok(TunnelHandler {
        state,
        tunnel_key,
        listener,
        close_channel: receiver,
    })
}

#[instrument(skip_all, fields(tunnel=%handler.tunnel_key))]
async fn run_tunnel(mut handler: TunnelHandler) {
    debug!("Started tunnel");
    let tunnel_key = handler.tunnel_key;
    loop {
        let finish_receiver = handler.close_channel.clone();
        tokio::select! {
        socket = handler.listener.accept() => {
            match socket {
            Ok((socket, client_addr)) => {
                tokio::spawn(process_socket(
                    socket,
                    client_addr,
                    tunnel_key.clone(),
                    handler.state.clone(),
                    finish_receiver,
                ).map_err(move |e| error!(error=%e, "Error in remote connection")));
            }
            Err(e) => error!(error=%e, "Cannot accept connection"),
        }

        }

         _ = handler.close_channel.changed() => {
            debug!("Finished tunnel");
            break
         }
        }
    }
}
