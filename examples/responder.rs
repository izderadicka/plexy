use std::net::SocketAddr;

use clap::Parser;
use futures::StreamExt;
use plexy::tunnel::SocketSpec;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec;
use tracing::{debug, error, info};

#[derive(Debug, Parser)]
struct Args {
    #[arg(required = true, help = "Address to listen on")]
    addr: SocketSpec,
}

async fn respond(socket: TcpStream, client_addr: SocketAddr, my_addr: SocketSpec) {
    debug!(client = %client_addr, "Client connected");
    let line_codec = codec::LinesCodec::new_with_max_length(8192);

    let framed = codec::Framed::new(socket, line_codec);
    let (sink, stream) = framed.split::<String>();
    let responses = stream.map(|req| match req {
        Ok(msg) => Ok(format!("[{}] ECHO: {}", my_addr, msg)),
        Err(e) => Ok(format!("[{}] ERROR: {}", my_addr, e)),
    });
    if let Err(e) = responses.forward(sink).await {
        error!(error=%e, "Error in protocol")
    }
    debug!(client = %client_addr, "Client disconnected");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let listener = TcpListener::bind(args.addr.as_tuple()).await?;
    info!(address=%args.addr, "Started responder");
    while let Ok((socket, client_addr)) = listener.accept().await {
        tokio::spawn(respond(socket, client_addr, args.addr.clone()));
    }
    Ok(())
}
