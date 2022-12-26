use error::{Error, Result};
use std::{net::SocketAddr, str::FromStr};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
};
use tracing::{debug, error};

pub use state::State;

use crate::aio::copy_bidirectional;

mod aio;
pub mod config;
pub mod controller;
pub mod error;
mod state;

#[derive(Debug, Clone)]
pub struct Tunnel {
    pub local: SocketAddr,
    pub remote: SocketAddr,
}

impl std::fmt::Display for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}={}", self.local, self.remote)
    }
}

impl FromStr for Tunnel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let (local_part, remote_part) = s
            .split_once("=")
            .ok_or_else(|| Error::TunnelParseError(format!("Missing = in tunnel definition")))?;
        let remote: SocketAddr = remote_part.parse()?;
        let local: SocketAddr = if local_part.contains(":") {
            local_part.parse()?
        } else {
            let port: u16 = local_part
                .parse()
                .map_err(|e| Error::TunnelParseError(format!("Local port parse error: {}", e)))?;
            SocketAddr::V4(std::net::SocketAddrV4::new(
                std::net::Ipv4Addr::LOCALHOST,
                port,
            ))
        };
        Ok(Tunnel { local, remote })
    }
}

async fn process_socket(mut socket: TcpStream, tunnel: Tunnel, state: State) {
    let remote_client = socket
        .peer_addr()
        .map_err(|e| error!("Cannot get client address: {}", e))
        .ok();
    debug!(client = ?remote_client, "Client connected");
    state.client_connected(&tunnel.local, remote_client.as_ref());
    match TcpStream::connect(tunnel.remote).await {
        Ok(mut stream) => {
            match copy_bidirectional(&mut socket, &mut stream, tunnel.local, state.clone()).await {
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

pub fn stop_tunnel(local: &SocketAddr, state: State) -> Result<()> {
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
    let listener = TcpListener::bind(tunnel.local).await?;
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
        tokio::select! {
        socket = handler.listener.accept() => {
            match socket {
            Ok((socket, _remote)) => {
                tokio::spawn(process_socket(
                    socket,
                    handler.tunnel.clone(),
                    handler.state.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_full() {
        let t: Tunnel = "0.0.0.0:3333=127.0.0.1:3000".parse().expect("valid tunnel");
        assert_eq!(3333, t.local.port());
        assert_eq!(Ipv4Addr::new(0, 0, 0, 0), t.local.ip());
        assert_eq!(3000, t.remote.port());
        assert_eq!(Ipv4Addr::new(127, 0, 0, 1), t.remote.ip());
    }

    #[test]
    fn test_port_only() {
        let t: Tunnel = "3333=127.0.0.1:3000".parse().expect("valid tunnel");
        assert_eq!(3333, t.local.port());
        assert_eq!(Ipv4Addr::new(127, 0, 0, 1), t.local.ip());
        assert_eq!(3000, t.remote.port());
        assert_eq!(Ipv4Addr::new(127, 0, 0, 1), t.remote.ip());
    }
}
