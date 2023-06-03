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
    #[error("Remote already exists")]
    RemoteExists,
    #[error("Remote does not exists")]
    RemoteDoesNotExist,
    #[error("Invalid control protocol line: {0}")]
    ControlProtocolLineError(#[from] tokio_util::codec::LinesCodecError),
    #[error("Control protocol error: {0}")]
    ControlProtocolError(String),
    #[error("No remote available")]
    NoRemote,
    #[error("Invalid loadbalancing strategy string")]
    InvalidLBStrategy,
    #[error("RPC error: {0}")]
    RPCError(#[from] jsonrpsee::core::Error),
}
const ERROR_BASE: i32 = 1000;

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::TunnelParseError(_) => ERROR_BASE + 1,
            Error::SocketSpecParseError(_) => ERROR_BASE + 2,
            Error::SocketAddrParse(_) => ERROR_BASE + 3,
            Error::IOError(_) => ERROR_BASE + 4,
            Error::TunnelExists => ERROR_BASE + 5,
            Error::TunnelDoesNotExist => ERROR_BASE + 6,
            Error::RemoteExists => ERROR_BASE + 7,
            Error::RemoteDoesNotExist => ERROR_BASE + 8,
            Error::ControlProtocolLineError(_) => ERROR_BASE + 9,
            Error::ControlProtocolError(_) => ERROR_BASE + 10,
            Error::NoRemote => ERROR_BASE + 11,
            Error::InvalidLBStrategy => ERROR_BASE + 12,
            Error::RPCError(_) => ERROR_BASE + 13,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
