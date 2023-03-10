#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Tunnel definition parsing error: {0}")]
    TunnelParseError(String),
    #[error("Socket spec parse error: {0}")]
    SocketSpecParseError(String),
    #[error("Socket address parse error: {0}")]
    SocketAddrParse(#[from] std::net::AddrParseError),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Tunnel already exists")]
    TunnelExists,
    #[error("Tunnel doesn't exist")]
    TunnelDoesNotExist,
    #[error("Invalid control protocol line: {0}")]
    ControlProtocolLineError(#[from] tokio_util::codec::LinesCodecError),
    #[error("Control protocol error: {0}")]
    ControlProtocolError(String),
    #[error("No remote available")]
    NoRemote,
    #[error("Invalid loadbalancing strategy string")]
    InvalidLBStrategy,
}

pub type Result<T> = std::result::Result<T, Error>;
