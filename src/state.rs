use std::{net::SocketAddr, sync::Arc};
use tokio::sync::oneshot;

use crate::{
    error::{Error, Result},
    Tunnel,
};

#[derive(Debug)]
pub struct TunnelInfo {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    pub close_channel: oneshot::Sender<()>,
}

impl TunnelInfo {
    pub fn new(close_channel: oneshot::Sender<()>) -> Self {
        TunnelInfo {
            streams_open: 0,
            bytes_sent: 0,
            bytes_received: 0,
            total_connections: 0,
            close_channel,
        }
    }
}

#[derive(Clone)]
pub struct State {
    tunnels: Arc<dashmap::DashMap<SocketAddr, TunnelInfo, fxhash::FxBuildHasher>>,
}

impl State {
    pub fn new() -> Self {
        State {
            tunnels: Arc::new(dashmap::DashMap::with_hasher(
                fxhash::FxBuildHasher::default(),
            )),
        }
    }

    pub(crate) fn add_tunnel(
        &self,
        tunnel: Tunnel,
        close_channel: oneshot::Sender<()>,
    ) -> Result<()> {
        if self.tunnels.contains_key(&tunnel.local) {
            return Err(Error::TunnelExists);
        }
        let info = TunnelInfo::new(close_channel);
        self.tunnels.insert(tunnel.local, info);
        Ok(())
    }

    pub(crate) fn remove_tunnel(&self, local: &SocketAddr) -> Result<TunnelInfo> {
        self.tunnels
            .remove(local)
            .map(|(_, t)| t)
            .ok_or_else(|| Error::TunnelDoesNotExist)
    }

    pub fn number_of_tunnels(&self) -> usize {
        self.tunnels.len()
    }
}
