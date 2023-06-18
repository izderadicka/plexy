use rustls::ClientConfig;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    state::strategy::TunnelLBStrategy,
    State,
};
use std::{fmt::Display, str::FromStr, sync::Arc};

use self::parser::{socket_spec, tunnel};

mod parser;

/// This is our equivalence to SocketAddr, but with host name
/// As it is expected to move around thread frequently,
/// it's implemented as referenced string
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct SocketSpec {
    inner: Arc<str>,
}

impl SocketSpec {
    pub fn as_tuple(&self) -> (&str, u16) {
        // here we assume that inner str is in normalized form
        let (mut host, port) = self.inner.rsplit_once(':').unwrap();
        if host.starts_with('[') && host.ends_with(']') {
            host = &host[1..host.len() - 1];
        }
        let port = port.parse().unwrap();
        (host, port)
    }

    pub fn port(&self) -> u16 {
        self.as_tuple().1
    }

    pub fn host(&self) -> &str {
        self.as_tuple().0
    }
}

impl FromStr for SocketSpec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        socket_spec(s)
            .map_err(|e| match e {
                nom::Err::Incomplete(_) => {
                    Error::SocketSpecParseError("Incomplete Socket Spec".into())
                }
                nom::Err::Error(e) | nom::Err::Failure(e) => Error::SocketSpecParseError(format!(
                    "Failed parser: {:?}, unparsed: {}",
                    e.code, e.input
                )),
            })
            .and_then(|(rest, spec)| {
                if !rest.trim_end().is_empty() {
                    Err(Error::SocketSpecParseError(format!(
                        "Extra characters after spec: {}",
                        rest
                    )))
                } else {
                    Ok(spec)
                }
            })
    }
}

impl Display for SocketSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)
    }
}

#[cfg(feature = "metrics")]
impl From<&SocketSpec> for opentelemetry::Value {
    fn from(value: &SocketSpec) -> Self {
        opentelemetry::Value::String(value.inner.clone().into())
    }
}

#[cfg(feature = "metrics")]
impl From<SocketSpec> for opentelemetry::Value {
    fn from(value: SocketSpec) -> Self {
        opentelemetry::Value::String(value.inner.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRemoteOptions {
    pub errors_till_dead: u64,
    pub connect_timeout: f32,
    pub dead_retry: f32,
    pub tls: bool,
}

impl TunnelRemoteOptions {
    pub fn tls_config(&self, state: &State) -> Option<Arc<ClientConfig>> {
        if self.tls {
            Some(state.client_ssl_config())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelOptions {
    pub lb_strategy: TunnelLBStrategy,
    pub remote_connect_retries: u16,
    pub options: TunnelRemoteOptions,
}

static mut DEFAULT_TUNNEL_OPTIONS: TunnelOptions = TunnelOptions {
    lb_strategy: TunnelLBStrategy::Random,
    remote_connect_retries: 3,
    options: TunnelRemoteOptions {
        errors_till_dead: 1,
        connect_timeout: 10.0,
        dead_retry: 10.0,
        tls: false,
    },
};

/// Must be used only at very of beginning program before anything else
/// especially Tunnel and TunnelOptions usage
/// otherwise is UB
pub fn set_default_tunnel_options(options: TunnelOptions) {
    unsafe {
        DEFAULT_TUNNEL_OPTIONS = options;
    }
}

impl Default for TunnelOptions {
    fn default() -> Self {
        unsafe { DEFAULT_TUNNEL_OPTIONS.clone() }
    }
}

impl Display for TunnelOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "strategy={},retries={},timeout={}, errors={}",
            self.lb_strategy,
            self.remote_connect_retries,
            self.options.connect_timeout,
            self.options.errors_till_dead
        )
    }
}

#[derive(Debug, Clone)]
pub struct Tunnel {
    pub local: SocketSpec,
    pub remote: Vec<SocketSpec>,
    pub options: Option<TunnelOptions>,
}

impl std::fmt::Display for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=", self.local)?;
        for (n, addr) in self.remote.iter().enumerate() {
            if n > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}", addr)?;
        }

        Ok(())
    }
}

impl FromStr for Tunnel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        tunnel(s)
            .map_err(|e| match e {
                nom::Err::Incomplete(_) => Error::TunnelParseError("Incomplete tunnel spec".into()),
                nom::Err::Error(e) | nom::Err::Failure(e) => {
                    Error::TunnelParseError(format!("Parser: {:?}, Unparsed: {}", e.code, e.input))
                }
            })
            .map(|(_, t)| t)
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
        assert_eq!(3000, t.remote[0].port());
        assert_eq!("127.0.0.1", t.remote[0].host());
    }

    #[test]
    fn test_port_only() {
        let t: Tunnel = "3333=127.0.0.1:3000".parse().expect("valid tunnel");
        assert_eq!(3333, t.local.port());
        assert_eq!("127.0.0.1", t.local.host());
        assert_eq!(3000, t.remote[0].port());
        assert_eq!("127.0.0.1", t.remote[0].host());
    }

    #[test]
    fn test_tunnel_with_options() {
        let t_str = "localhost:3000=host1:3001,host2:3002,host3:3003[strategy=round-robin,timeout=55.5,retries=5]";
        let t: Tunnel = t_str.parse().expect("Valid tunnel spec");
        assert_eq!(t.options.unwrap().remote_connect_retries, 5);
    }
}
