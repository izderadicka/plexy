use clap::Parser;
use plexy::{config::Args, controller::run_controller, start_tunnel, State};
use std::io;
use tracing::{error, info};

#[tokio::main]
async fn main() -> io::Result<()> {
    console_subscriber::init();
    //tracing_subscriber::fmt::init();
    let mut args = Args::parse();
    info!(tunnels = ?args.tunnels, "Started plexy");
    info!("Control interface listening on {}", args.control_socket);
    let tunnels = args.tunnels.take();
    let control_socket = args.control_socket;
    let state = State::new(args);
    tokio::spawn(run_controller(control_socket, state.clone()));
    // launch tunnels
    if let Some(tunnels) = tunnels {
        for tunnel in tunnels {
            let local = tunnel.local.clone();
            if let Err(e) = start_tunnel(tunnel, state.clone()).await {
                error!("Cannot start tunnel on {:?}: {}", local, e);
            };
        }
    }
    std::future::pending::<()>().await;
    Ok(())
}
