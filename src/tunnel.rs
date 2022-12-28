use std::{fmt::Display, str::FromStr};

use crate::error::{Error, Result};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct SocketSpec {
    host: String,
    port: u16,
}

impl SocketSpec {
    pub fn as_tuple(&self) -> (&str, u16) {
        (self.host.as_str(), self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn host(&self) -> &str {
        self.host.as_str()
    }
}

impl FromStr for SocketSpec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let mut parts = s.splitn(2, ":");
        match parts.next() {
            Some(mut host) => {
                let port = match parts.next() {
                    Some(port) => port,
                    None => {
                        let h = host;
                        host = "127.0.0.1";
                        h
                    }
                };
                let port: u16 = port
                    .parse()
                    .map_err(|_e| Error::SocketSpecParseError("Invalid port number".into()))?;
                Ok(SocketSpec {
                    host: host.to_string(),
                    port,
                })
            }
            None => return Err(Error::SocketSpecParseError("Empty".into())),
        }
    }
}

impl Display for SocketSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct Tunnel {
    pub local: SocketSpec,
    pub remote: SocketSpec,
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
        let remote: SocketSpec = remote_part.parse()?;
        let local: SocketSpec = local_part.parse()?;
        Ok(Tunnel { local, remote })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full() {
        let t: Tunnel = "0.0.0.0:3333=127.0.0.1:3000".parse().expect("valid tunnel");
        assert_eq!(3333, t.local.port());
        assert_eq!("0.0.0.0", t.local.host());
        assert_eq!(3000, t.remote.port());
        assert_eq!("127.0.0.1", t.remote.host());
    }

    #[test]
    fn test_port_only() {
        let t: Tunnel = "3333=127.0.0.1:3000".parse().expect("valid tunnel");
        assert_eq!(3333, t.local.port());
        assert_eq!("127.0.0.1", t.local.host());
        assert_eq!(3000, t.remote.port());
        assert_eq!("127.0.0.1", t.remote.host());
    }
}
