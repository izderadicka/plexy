use std::str::FromStr;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::{
    error::{Error, Result},
    start_tunnel, stop_tunnel,
    tunnel::SocketSpec,
    State, Tunnel,
};

use self::codec::CommandCodec;

mod codec;

const MAX_LINE_LENGTH: usize = 1024;

pub fn split_socket(
    socket: &mut TcpStream,
) -> (
    FramedRead<tokio::net::tcp::ReadHalf<'_>, CommandCodec>,
    FramedWrite<tokio::net::tcp::WriteHalf<'_>, CommandCodec>,
) {
    let (reader, writer) = socket.split();
    let reader = FramedRead::new(reader, CommandCodec::new_with_max_length(MAX_LINE_LENGTH));
    let writer = FramedWrite::new(writer, CommandCodec::new_with_max_length(MAX_LINE_LENGTH));
    (reader, writer)
}

#[async_trait]
pub trait Command: FromStr {
    async fn exec(self, ctx: State) -> CommandResponse; //Box<dyn std::future::Future<Output = CommandResponse> + Send + 'static>;
}
#[derive(Debug)]
pub enum CommandRequest {
    Open(Tunnel),
    Close(SocketSpec),
    Status(bool),
    Help,
    Exit,
    Invalid(Error),
}

impl FromStr for CommandRequest {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut parts = s.splitn(2, " ");
        let cmd = parts
            .next()
            .ok_or_else(|| Error::ControlProtocolError("No command".into()))?;
        let cmd = cmd.to_ascii_uppercase();
        let mut args = || {
            parts
                .next()
                .ok_or_else(|| Error::ControlProtocolError("Missing argument".into()))
        };
        match cmd.as_str() {
            "STATUS" => {
                let scale = args().map(|s| s.to_ascii_uppercase()).unwrap_or_default();
                let is_full = match scale.as_str() {
                    "LONG" | "FULL" => true,
                    "SHORT" | "" => false,
                    _ => {
                        return Err(Error::ControlProtocolError(
                            "Invalid argument to STATUS".into(),
                        ))
                    }
                };
                Ok(CommandRequest::Status(is_full))
            }
            "OPEN" => {
                let tunnel: Tunnel = args()?.parse()?;
                Ok(CommandRequest::Open(tunnel))
            }
            "HELP" => Ok(CommandRequest::Help),
            "EXIT" => Ok(CommandRequest::Exit),
            "CLOSE" => {
                let addr: SocketSpec = args()?.parse()?;
                Ok(CommandRequest::Close(addr))
            }
            _ => Err(Error::ControlProtocolError(format!(
                "Invalid command: {}",
                cmd
            ))),
        }
    }
}

#[derive(Debug)]
pub enum CommandResponse {
    OK,
    Problem(Option<Error>),
    Info {
        short: String,
        details: Option<Vec<String>>,
    },
    Done,
}

impl std::fmt::Display for CommandResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandResponse::Done => write!(f, "DONE"),
            CommandResponse::OK => write!(f, "OK"),
            CommandResponse::Problem(msg) => {
                write!(f, "SORRY")?;
                match msg {
                    Some(s) => write!(f, ": {}", s),
                    None => Ok(()),
                }
            }
            CommandResponse::Info { short, details } => {
                write!(f, "OK: {}", short)?;
                if let Some(lines) = details {
                    writeln!(f, "")?;
                    let length = lines.len();
                    for (n, l) in lines.into_iter().enumerate() {
                        write!(f, "\t{}", l)?;
                        if n < length - 1 {
                            writeln!(f, "")?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

impl<T> From<Result<T>> for CommandResponse {
    fn from(res: Result<T>) -> Self {
        match res {
            Ok(_) => CommandResponse::OK,
            Err(e) => CommandResponse::Problem(Some(e)),
        }
    }
}

#[async_trait]
impl Command for CommandRequest {
    async fn exec(self, ctx: State) -> CommandResponse {
        match self {
            CommandRequest::Open(tunnel) => start_tunnel(tunnel, ctx).await.into(),
            CommandRequest::Close(local) => stop_tunnel(&local, ctx).into(),
            CommandRequest::Invalid(e) => CommandResponse::Problem(Some(e)),
            CommandRequest::Exit => CommandResponse::Done,
            CommandRequest::Status(long) => {
                if ctx.number_of_tunnels() == 0 {
                    CommandResponse::Info {
                        short: format!("No tunnels"),
                        details: None,
                    }
                } else {
                    let short = format!("Tunnels: {}", ctx.number_of_tunnels());
                    let details = if long {
                        let details: Vec<String> = ctx
                            .stats_iter()
                            .map(|(local, stats)| {
                                format!(
                                    "{} = open conns {}, total {}, bytes sent {}, received {}",
                                    local,
                                    stats.streams_open,
                                    stats.total_connections,
                                    stats.bytes_sent,
                                    stats.bytes_received
                                )
                            })
                            .collect();
                        Some(details)
                    } else {
                        None
                    };
                    CommandResponse::Info { short, details }
                }
            }
            CommandRequest::Help => {
                let help = &[
                    "OPEN tunnel",
                    "CLOSE socket_address",
                    "STATUS [full|long]",
                    "EXIT",
                    "HELP",
                ];
                let help = help.into_iter().map(|s| s.to_string()).collect();
                CommandResponse::Info {
                    short: "commands".into(),
                    details: Some(help),
                }
            }
        }
    }
}
