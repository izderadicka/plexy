use clap::Parser;
use plexy::{
    config::Args,
    controller::run_controller,
    start_tunnel,
    tunnel::{set_default_tunnel_options, TunnelOptions},
    State,
};
use tracing::{error, info};

#[tokio::main]
async fn main() -> plexy::error::Result<()> {
    console_subscriber::init();
    //tracing_subscriber::fmt::init();
    let mut args = Args::parse();

    set_default_tunnel_options(TunnelOptions {
        lb_strategy: Default::default(),
        remote_connect_retries: args.establish_remote_connection_retries,
        remote_connect_timeout: args.establish_remote_connection_timeout,
    });

    let tunnels = match args.take_tunnels() {
        Ok(t) => t,
        Err(e) => {
            error!("Invalid initial tunnels specification: {}", e);
            eprintln!("Invalid initial tunnels specification: {}", e);
            return Err(e);
        }
    };
    let control_socket = args.control_socket;
    let state = State::new(args);
    info!(tunnels = ?tunnels, "Started plexy");
    info!("Control interface listening on {}", control_socket);
    tokio::spawn(run_controller(control_socket, state.clone()));
    // launch tunnels
    for tunnel in tunnels {
        let local = tunnel.local.clone();
        if let Err(e) = start_tunnel(tunnel, state.clone()).await {
            error!("Cannot start tunnel on {:?}: {}", local, e);
        };
    }
    std::future::pending::<()>().await;
    Ok(())
}
