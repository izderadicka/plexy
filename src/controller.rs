use std::{io, net::SocketAddr};

use futures::SinkExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;
use tokio_util::codec;
use tracing::{debug, error};

use crate::Tunnel;

const MAX_LINE_LENGTH: usize = 1024;

pub(crate) enum Command {
    Open(Tunnel),
    Close(SocketAddr),
    Status { long: bool },
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
