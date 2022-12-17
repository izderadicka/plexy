use clap::Parser;
use plexy::{config::Args, Tunnel};
use std::{io, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};

async fn process_socket(mut socket: TcpStream, fwd: SocketAddr) -> io::Result<(u64, u64)> {
    let mut stream = TcpStream::connect(fwd).await?;
    tokio::io::copy_bidirectional(&mut socket, &mut stream).await
}

async fn run_tunnel(tunnel: Tunnel) -> io::Result<()> {
    let listener = TcpListener::bind(tunnel.local).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(process_socket(socket, tunnel.remote));
    }
}

#[cfg(not(unix))]
async fn wait_terminate() {
    use tokio::signal;
    signal::ctrl_c().await.unwrap_or(());
}

#[cfg(unix)]
async fn wait_terminate() {
    use tokio::signal::unix::*;
    let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM cannot be listened");
    let mut sigint = signal(SignalKind::terminate()).expect("SIGINT cannot be listened");

    let term_reason = tokio::select! {
        _ = sigint.recv() => "SIGINT",
        _ = sigterm.recv() => "SIGTERM",
    };

    eprintln!("App terminated by {}", term_reason);
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    // launch tunnels
    for tunnel in args.tunnels {
        tokio::spawn(run_tunnel(tunnel));
    }
    wait_terminate().await;
    Ok(())
}
