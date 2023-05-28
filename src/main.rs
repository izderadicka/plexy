use clap::Parser;
use plexy::{
    config::Args,
    controller::run_controller,
    rpc::run_rpc_server,
    start_tunnel,
    tunnel::{set_default_tunnel_options, TunnelOptions, TunnelRemoteOptions},
    State,
};
use tracing::{error, info};

#[tokio::main]
async fn main() -> plexy::error::Result<()> {
    //console_subscriber::init();
    tracing_subscriber::fmt::init();
    let mut args = Args::parse();
    if args.help_tunnel {
        Args::tunnel_help();
        return Ok(());
    }
    set_default_tunnel_options(TunnelOptions {
        lb_strategy: Default::default(),
        remote_connect_retries: args.remote_retries,
        options: TunnelRemoteOptions {
            connect_timeout: args.remote_timeout,
            errors_till_dead: args.remote_errors,
            dead_retry: args.remote_dead_check_interval,
        },
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
    let rpc_socket = args.rpc_socket;
    let state = State::new(args);
    info!(tunnels = ?tunnels, "Started plexy");
    if let Some(control_socket) = control_socket {
        info!("Control interface listening on {}", control_socket);
        tokio::spawn(run_controller(control_socket, state.clone()));
    }

    if let Some(rpc_socket) = rpc_socket {
        info!("RPC interface listening on {}", rpc_socket);
        tokio::spawn(run_rpc_server(rpc_socket, state.clone()));
    }
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
