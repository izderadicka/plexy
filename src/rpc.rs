use std::{collections::HashMap, net::SocketAddr};

use jsonrpsee::{proc_macros::rpc, server::ServerBuilder, types::ErrorObject};
use serde::Serialize;

use crate::{
    error::Error,
    start_tunnel,
    state::{RemoteInfo, TunnelInfo, TunnelStats},
    stop_tunnel,
    tunnel::{SocketSpec, TunnelOptions},
    State, Tunnel,
};

type RPCResult<T> = Result<T, Error>;

impl From<Error> for ErrorObject<'static> {
    fn from(value: Error) -> Self {
        ErrorObject::owned::<()>(1, value.to_string(), None)
    }
}

#[derive(Clone, Serialize)]
pub struct RPCTunnelInfo {
    stats: TunnelStats,
    num_remotes: usize,
    num_dead_remotes: usize,
    options: TunnelOptions,
}

impl From<&TunnelInfo> for RPCTunnelInfo {
    fn from(ti: &TunnelInfo) -> Self {
        RPCTunnelInfo {
            stats: ti.stats.clone(),
            num_remotes: ti.remotes.len(),
            num_dead_remotes: ti.dead_remotes.len(),
            options: ti.options.clone(),
        }
    }
}

#[rpc(server)]
trait Interface {
    #[method(name = "numberOfTunnels")]
    fn number_of_tunnels(&self) -> usize;
    #[method(name = "tunnelInfo")]
    fn tunnel_info(&self, tunnel_socket: String) -> RPCResult<RPCTunnelInfo>;
    #[method(name = "remotes")]
    fn remotes(&self, tunnel_socket: String) -> RPCResult<HashMap<String, RemoteInfo>>;
    #[method(name = "openTunnel")]
    fn open_tunnel(
        &self,
        tunnel_socket: String,
        remotes: Vec<String>,
        options: Option<TunnelOptions>,
    ) -> RPCResult<()>;
    #[method(name = "closeTunnel")]
    fn close_tunnel(&self, tunnel_socket: String) -> RPCResult<()>;
}

pub struct ControlRpc {
    state: State,
}

impl InterfaceServer for ControlRpc {
    fn number_of_tunnels(&self) -> usize {
        return self.state.number_of_tunnels();
    }

    fn tunnel_info(&self, tunnel_socket: String) -> RPCResult<RPCTunnelInfo> {
        let addr: SocketSpec = tunnel_socket.parse()?;
        self.state.info_to(&addr)
    }

    fn remotes(&self, tunnel_socket: String) -> RPCResult<HashMap<String, RemoteInfo>> {
        let addr: SocketSpec = tunnel_socket.parse()?;
        self.state
            .remotes(&addr)
            .map(|(r, _)| r)?
            .into_iter()
            .map(|(k, v)| Ok((k.to_string(), v)))
            .collect()
    }

    fn open_tunnel(
        &self,
        tunnel_socket: String,
        remotes: Vec<String>,
        options: Option<TunnelOptions>,
    ) -> RPCResult<()> {
        let local = tunnel_socket.parse()?;
        let remote = remotes
            .into_iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()?;
        let tunnel = Tunnel {
            local,
            options,
            remote,
        };
        let _join_handle = start_tunnel(tunnel, self.state.clone());
        Ok(())
    }

    fn close_tunnel(&self, tunnel_socket: String) -> RPCResult<()> {
        let local = tunnel_socket.parse()?;
        stop_tunnel(&local, self.state.clone())
    }
}

pub async fn run_rpc_server(addr: SocketAddr, state: State) -> Result<(), Error> {
    let server = ServerBuilder::default().build(addr).await?;
    let rpc = ControlRpc { state };
    let handle = server.start(rpc.into_rpc())?;
    Ok(handle.stopped().await)
}
