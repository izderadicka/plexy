use error::Error;
use std::{io, net::SocketAddr, str::FromStr};
use tokio::net::{TcpListener, TcpStream};
use tracing::debug;

pub mod config;
pub mod controller;
pub mod error;

#[derive(Debug, Clone)]
pub struct Tunnel {
    pub local: SocketAddr,
    pub remote: SocketAddr,
}

impl FromStr for Tunnel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

pub async fn process_socket(mut socket: TcpStream, fwd: SocketAddr) -> io::Result<(u64, u64)> {
    let remote_client = socket
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    debug!(client = remote_client, "Client connected");
    let mut stream = TcpStream::connect(fwd).await?;
    let res = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
    debug!(client = remote_client, "Client disconnected");
    res
}

pub async fn run_tunnel(tunnel: Tunnel) -> io::Result<()> {
    let listener = TcpListener::bind(tunnel.local).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(process_socket(socket, tunnel.remote));
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
