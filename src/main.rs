use clap::Parser;
use plexy::{config::Args, controller::run_controller, run_tunnel};
use std::io;
use tracing::info;

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    info!(tunnels = args.tunnels.len(), "Started plexy");
    info!("Control interface listening on {}", args.control_socket);
    tokio::spawn(run_controller(args.control_socket));
    // launch tunnels
    for tunnel in args.tunnels {
        tokio::spawn(run_tunnel(tunnel));
    }
    std::future::pending::<()>().await;
    Ok(())
}
