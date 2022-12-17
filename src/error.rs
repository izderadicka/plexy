#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Tunnel definition parsing error: {0}")]
    TunnelParseError(String),
    #[error("Socket address parse error: {0}")]
    SocketAddrParse(#[from] std::net::AddrParseError),
}
