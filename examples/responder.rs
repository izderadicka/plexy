use std::net::SocketAddr;

use clap::Parser;
use futures::{StreamExt, TryFutureExt};
use plexy::tunnel::SocketSpec;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec;
use tracing::{debug, error, info};

#[derive(Debug, Parser)]
struct Args {
    #[arg(required = true, num_args=1..=1024, help = "Addresses to listen on")]
    addr: Vec<SocketSpec>,
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
    for addr in args.addr {
        let addr2 = addr.clone();
        tokio::spawn(
            async move {
                let listener = TcpListener::bind(addr.as_tuple()).await?;
                info!(address=%addr, "Started responder");
                while let Ok((socket, client_addr)) = listener.accept().await {
                    tokio::spawn(respond(socket, client_addr, addr.clone()));
                }
                Ok::<_, anyhow::Error>(())
            }
            .map_err(move |e| error!(error=%e, address=%addr2, "Error listening")),
        );
    }

    futures::future::pending::<()>().await;
    Ok(())
}
