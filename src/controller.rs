use std::{io, net::SocketAddr, str::FromStr};

use bytes::BytesMut;
use futures::SinkExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;
use tokio_util::codec;
use tracing::{debug, error};

use crate::{error::Error, Tunnel};

const MAX_LINE_LENGTH: usize = 1024;

enum Command {
    Open(Tunnel),
    Close(SocketAddr),
    Status,
    Help,
}

impl FromStr for Command {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
            "STATUS" => Ok(Command::Status),
            "OPEN" => {
                let tunnel: Tunnel = args()?.parse()?;
                Ok(Command::Open(tunnel))
            }
            _ => Err(Error::ControlProtocolError(format!(
                "Invalid command: {}",
                cmd
            ))),
        }
    }
}

enum CommandResponse {
    OK,
    Problem(Option<String>),
    Info {
        short: String,
        details: Option<Vec<String>>,
    },
}

struct CommandCodec {
    lines_codec: codec::LinesCodec,
}

impl codec::Decoder for CommandCodec {
    type Item = Command;

    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let res = self.lines_codec.decode(src)?;
        match res {
            Some(line) => {
                let cmd: Command = line.parse()?;
                Ok(Some(cmd))
            }
            None => Ok(None),
        }
    }
}

async fn control_loop(mut socket: TcpStream) {
    let (reader, writer) = socket.split();
    let mut reader = codec::FramedRead::new(
        reader,
        codec::LinesCodec::new_with_max_length(MAX_LINE_LENGTH),
    );
    let mut writer = codec::FramedWrite::new(
        writer,
        codec::LinesCodec::new_with_max_length(MAX_LINE_LENGTH),
    );

    while let Some(line) = reader.next().await {
        match line {
            Ok(line) => {
                debug!(cmd = line, "Command received");
            }
            Err(e) => error!(error = e.to_string(), "Protocol error"),
        }

        if let Err(e) = writer.send("OK".to_string()).await {
            error!(error = e.to_string(), "Cannot send response");
        }
    }
}

pub async fn run_controller(socket_addr: SocketAddr) -> io::Result<()> {
    let listener = TcpListener::bind(socket_addr).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(control_loop(socket));
    }
}
