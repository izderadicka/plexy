use error::{Error, Result};
use std::{net::SocketAddr, str::FromStr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tracing::{debug, error};

pub mod config;
pub mod controller;
pub mod error;

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

#[derive(Debug)]
struct TunnelInfo {
    streams_open: usize,
    bytes_sent: u64,
    bytes_received: u64,
    total_connections: u64,
    close_channel: oneshot::Sender<()>,
}

impl TunnelInfo {
    fn new(close_channel: oneshot::Sender<()>) -> Self {
        TunnelInfo {
            streams_open: 0,
            bytes_sent: 0,
            bytes_received: 0,
            total_connections: 0,
            close_channel,
        }
    }
}

#[derive(Clone)]
pub struct State {
    tunnels: Arc<dashmap::DashMap<SocketAddr, TunnelInfo, fxhash::FxBuildHasher>>,
}

impl State {
    pub fn new() -> Self {
        State {
            tunnels: Arc::new(dashmap::DashMap::with_hasher(
                fxhash::FxBuildHasher::default(),
            )),
        }
    }

    pub(crate) fn add_tunnel(
        &self,
        tunnel: Tunnel,
        close_channel: oneshot::Sender<()>,
    ) -> Result<()> {
        if self.tunnels.contains_key(&tunnel.local) {
            return Err(Error::TunnelExists);
        }
        let info = TunnelInfo::new(close_channel);
        self.tunnels.insert(tunnel.local, info);
        Ok(())
    }

    pub(crate) fn remove_tunnel(&self, local: &SocketAddr) {
        self.tunnels.remove(local);
    }
}

async fn process_socket(mut socket: TcpStream, tunnel: Tunnel, state: State) {
    let remote_client = socket
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    debug!(client = remote_client, "Client connected");
    match TcpStream::connect(tunnel.remote).await {
        Ok(mut stream) => {
            let res = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
        }
        Err(e) => error!("Error while connecting to remote {}: {}", tunnel.remote, e),
    }

    debug!(client = remote_client, "Client disconnected");
}

pub(crate) struct TunnelHandler {
    state: State,
    tunnel: Tunnel,
    listener: TcpListener,
    close_channel: oneshot::Receiver<()>,
}

pub async fn start_tunnel(tunnel: Tunnel, state: State) -> Result<()> {
    let handler = create_tunnel(tunnel, state).await?;
    tokio::spawn(run_tunnel(handler));
    Ok(())
}

async fn create_tunnel(tunnel: Tunnel, state: State) -> Result<TunnelHandler> {
    let listener = TcpListener::bind(tunnel.local).await?;
    let (sender, receiver) = oneshot::channel();
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

         _ = &mut handler.close_channel => {
            handler.state.remove_tunnel(&handler.tunnel.local);
            debug!("Finished tunnel {:?}", handler.tunnel);
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
        let t: Tunnel = "0.0.0.0:3333=>127.0.0.1:3000"
            .parse()
            .expect("valid tunnel");
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
