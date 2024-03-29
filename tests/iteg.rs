#!(cfg(test))
use plexy::config::Args;
#[cfg(feature = "metrics")]
use plexy::metrics::init_meter;
use plexy::{error::Result, start_tunnel, stop_tunnel, State, Tunnel};

#[tokio::test(flavor = "current_thread")]
async fn start_stop_tunnel() -> Result<()> {
    #[cfg(feature = "metrics")]
    let state = State::new(Args::default(), init_meter()).unwrap();
    #[cfg(not(feature = "metrics"))]
    let state = State::new(Args::default()).unwrap();
    let tunnel: Tunnel = "3928=127.0.0.1:3927".parse()?;
    let join = start_tunnel(tunnel.clone(), state.clone()).await?;
    stop_tunnel(&tunnel.local, state.clone())?;
    join.await.unwrap();
    assert_eq!(0, state.number_of_tunnels());
    Ok(())
}
