use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::Rng;
use std::{net::SocketAddr, sync::Arc, time::Instant};
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
    pub remotes: IndexMap<SocketSpec, RemoteInfo, fxhash::FxBuildHasher>,
}

impl TunnelInfo {
    pub fn new(close_channel: watch::Sender<bool>, remotes: Vec<SocketSpec>) -> Self {
        TunnelInfo {
            stats: TunnelStats::default(),
            close_channel,
            remotes: remotes
                .into_iter()
                .map(|k| (k, RemoteInfo::default()))
                .collect(),
        }
    }
}

impl TunnelInfo {
    pub fn select_remote(&self) -> Result<&SocketSpec> {
        let size = self.remotes.len();
        if size == 0 {
            return Err(Error::NoRemote);
        }
        let idx: usize = rand::thread_rng().gen_range(0..size);
        self.remotes
            .get_index(idx)
            .map(|(k, _)| k)
            .ok_or_else(|| Error::NoRemote)
    }
}

#[derive(Debug, Default, Clone)]
pub struct RemoteInfo {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    pub last_error_time: Option<Instant>,
    pub num_errors: u64,
    pub total_errors: u64,
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

    pub fn select_remote(&self, tunnel_key: &SocketSpec) -> Result<SocketSpec> {
        self.inner
            .tunnels
            .get(tunnel_key)
            .ok_or_else(|| Error::TunnelDoesNotExist)
            .and_then(|info| info.select_remote().map(|socket| socket.clone()))
    }

    pub(crate) fn add_tunnel(
        &self,
        tunnel: Tunnel,
        close_channel: watch::Sender<bool>,
    ) -> Result<()> {
        if self.inner.tunnels.contains_key(&tunnel.local) {
            return Err(Error::TunnelExists);
        }
        let info = TunnelInfo::new(close_channel, tunnel.remote);
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

    pub fn client_connected(&self, local: &SocketSpec, _client_addr: &SocketAddr) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            rec.stats.total_connections += 1;
            rec.stats.streams_open += 1;
        };
    }

    pub fn remote_connected(
        &self,
        local: &SocketSpec,
        remote: &SocketSpec,
        _client_addr: &SocketAddr,
    ) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            if let Some(rec) = rec.remotes.get_mut(remote) {
                rec.streams_open += 1;
                rec.total_connections += 1;
                rec.num_errors = 0;
            }
        };
    }

    pub fn remote_error(&self, local: &SocketSpec, remote: &SocketSpec, _client_addr: &SocketAddr) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            if let Some(rec) = rec.remotes.get_mut(remote) {
                rec.total_errors += 1;
                rec.num_errors += 1;
                rec.last_error_time = Some(Instant::now());
            }
        }
    }

    pub fn update_transferred(
        &self,
        local: &SocketSpec,
        sent: bool,
        bytes: u64,
        _client_addr: SocketAddr,
    ) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            if sent {
                rec.stats.bytes_sent += bytes;
            } else {
                rec.stats.bytes_received += bytes;
            }
        };
    }

    pub fn client_disconnected(
        &self,
        local: &SocketSpec,
        remote: Option<&SocketSpec>,
        _client_addr: &SocketAddr,
    ) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            rec.stats.streams_open -= 1;
            //TODO: refactor when if let chain will become stable
            if let Some(remote) = remote {
                if let Some(rec) = rec.remotes.get_mut(remote) {
                    rec.streams_open -= 1;
                }
            }
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

    pub fn establish_remote_connection_retries(&self) -> u16 {
        self.inner.config.read().establish_remote_connection_retries
    }
}
