use error::Result;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
};
use tracing::{debug, error};
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

async fn process_socket(
    mut socket: TcpStream,
    tunnel: Tunnel,
    state: State,
    finish_receiver: watch::Receiver<bool>,
) {
    let remote_client = socket
        .peer_addr()
        .map_err(|e| error!("Cannot get client address: {}", e))
        .ok();
    debug!(client = ?remote_client, "Client connected");
    state.client_connected(&tunnel.local, remote_client.as_ref());
    match TcpStream::connect(tunnel.remote.as_tuple()).await {
        Ok(mut stream) => {
            match copy_bidirectional(
                &mut socket,
                &mut stream,
                tunnel.local.clone(),
                state.clone(),
                finish_receiver,
            )
            .await
            {
                Ok((_sent, _received)) => {
                    // state.update_stats(&tunnel.local, received, sent, remote_client.as_ref());
                }
                Err(e) => error!("Error copying between streams: {}", e),
            };
        }
        Err(e) => error!("Error while connecting to remote {}: {}", tunnel.remote, e),
    }
    state.client_disconnected(&tunnel.local, remote_client.as_ref());
    debug!(client = ?remote_client, "Client disconnected");
}

pub(crate) struct TunnelHandler {
    state: State,
    tunnel: Tunnel,
    listener: TcpListener,
    close_channel: watch::Receiver<bool>,
}

pub fn stop_tunnel(local: &SocketSpec, state: State) -> Result<()> {
    let tunnel_info = state.remove_tunnel(local)?;
    if let Err(_) = tunnel_info.close_channel.send(true) {
        error!("Cannot close tunnel")
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
    state.add_tunnel(tunnel.clone(), sender)?;
    Ok(TunnelHandler {
        state,
        tunnel,
        listener,
        close_channel: receiver,
    })
}

async fn run_tunnel(mut handler: TunnelHandler) {
    debug!("Started tunnel {:?}", handler.tunnel);
    loop {
        let finish_receiver = handler.close_channel.clone();
        tokio::select! {
        socket = handler.listener.accept() => {
            match socket {
            Ok((socket, _remote)) => {
                tokio::spawn(process_socket(
                    socket,
                    handler.tunnel.clone(),
                    handler.state.clone(),
                    finish_receiver,
                ));
            }
            Err(e) => error!("Cannot accept connection: {}", e),
        }

        }

         _ = handler.close_channel.changed() => {
            debug!("Finished tunnel {:?}", handler.tunnel);
            break
         }
        }
    }
}
