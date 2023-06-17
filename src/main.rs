use clap::Parser;
use futures::TryFutureExt;
#[cfg(feature = "metrics")]
use plexy::metrics::{init_meter, init_prometheus};
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
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();
    #[cfg(not(feature = "tokio-console"))]
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
            tls: false,
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
    #[cfg(feature = "metrics")]
    let prometheus_socket = args.prometheus_socket;

    #[cfg(feature = "metrics")]
    let state = State::new(args, init_meter())?;
    #[cfg(not(feature = "metrics"))]
    let state = State::new(args)?;

    info!(tunnels = ?tunnels, "Started plexy");

    #[cfg(feature = "metrics")]
    {
        if let Some(prometheus_socket) = prometheus_socket {
            let (_, registry) = init_prometheus();
            info!(
                "Prometheus interface is running on http://{}/metrics",
                prometheus_socket
            );

            tokio::spawn(plexy::metrics::run(prometheus_socket, registry));
        }
    }

    if let Some(control_socket) = control_socket {
        info!("Control interface listening on {}", control_socket);
        tokio::spawn(
            run_controller(control_socket, state.clone())
                .map_err(|e| error!("Cannot start control interface: {}", e)),
        );
    }

    if let Some(rpc_socket) = rpc_socket {
        info!("RPC interface listening on {}", rpc_socket);
        tokio::spawn(
            run_rpc_server(rpc_socket, state.clone())
                .map_err(|e| error!("Cannot start RPC interface: {}", e)),
        );
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
