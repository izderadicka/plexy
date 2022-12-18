use clap::Parser;
use plexy::{config::Args, controller::run_controller, start_tunnel, State};
use std::io;
use tracing::{error, info};

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    info!(tunnels = args.tunnels.len(), "Started plexy");
    info!("Control interface listening on {}", args.control_socket);
    let state = State::new();
    tokio::spawn(run_controller(args.control_socket));
    // launch tunnels
    for tunnel in args.tunnels {
        if let Err(e) = start_tunnel(tunnel.clone(), state.clone()).await {
            error!("Cannot start tunnel {:?}: {}", tunnel, e);
        };
    }
    std::future::pending::<()>().await;
    Ok(())
}
