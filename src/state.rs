use parking_lot::RwLock;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::watch;

use crate::{
    config::Args,
    error::{Error, Result},
    tunnel::SocketSpec,
    Tunnel,
};

#[derive(Debug, Default, Clone)]
pub struct TunnelStats {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
}

#[derive(Debug)]
pub struct TunnelInfo {
    pub stats: TunnelStats,
    pub close_channel: watch::Sender<bool>,
}

impl TunnelInfo {
    pub fn new(close_channel: watch::Sender<bool>) -> Self {
        TunnelInfo {
            stats: TunnelStats::default(),
            close_channel,
        }
    }
}

struct StateInner {
    tunnels: dashmap::DashMap<SocketSpec, TunnelInfo, fxhash::FxBuildHasher>,
    config: RwLock<Args>,
}

#[derive(Clone)]
pub struct State {
    inner: Arc<StateInner>,
}

impl State {
    pub fn new(args: Args) -> Self {
        State {
            inner: Arc::new(StateInner {
                tunnels: dashmap::DashMap::with_hasher(fxhash::FxBuildHasher::default()),
                config: RwLock::new(args),
            }),
        }
    }

    pub(crate) fn add_tunnel(
        &self,
        tunnel: Tunnel,
        close_channel: watch::Sender<bool>,
    ) -> Result<()> {
        if self.inner.tunnels.contains_key(&tunnel.local) {
            return Err(Error::TunnelExists);
        }
        let info = TunnelInfo::new(close_channel);
        self.inner.tunnels.insert(tunnel.local, info);
        Ok(())
    }

    pub(crate) fn remove_tunnel(&self, local: &SocketSpec) -> Result<TunnelInfo> {
        self.inner
            .tunnels
            .remove(local)
            .map(|(_, t)| t)
            .ok_or_else(|| Error::TunnelDoesNotExist)
    }

    pub fn number_of_tunnels(&self) -> usize {
        self.inner.tunnels.len()
    }

    pub fn client_connected(&self, local: &SocketSpec, _client_addr: Option<&SocketAddr>) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            rec.stats.total_connections += 1;
            rec.stats.streams_open += 1;
        };
    }

    pub fn update_transferred(
        &self,
        local: &SocketSpec,
        sent: bool,
        bytes: u64,
        _client_addr: Option<&SocketAddr>,
    ) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            if sent {
                rec.stats.bytes_sent += bytes;
            } else {
                rec.stats.bytes_received += bytes;
            }
        };
    }

    pub fn client_disconnected(&self, local: &SocketSpec, _client_addr: Option<&SocketAddr>) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            rec.stats.streams_open -= 1;
        }
    }

    pub fn stats_iter(&self) -> impl Iterator<Item = (SocketSpec, TunnelStats)> + '_ {
        let iter = self.inner.tunnels.iter();
        iter.map(|i| (i.key().clone(), i.value().stats.clone()))
    }

    pub fn copy_buffer_size(&self) -> usize {
        let config = self.inner.config.read();
        config.copy_buffer_size
    }

    pub fn establish_remote_connection_timeout(&self) -> f32 {
        self.inner.config.read().establish_remote_connection_timeout
    }
}
