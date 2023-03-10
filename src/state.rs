use indexmap::IndexMap;
use parking_lot::RwLock;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, sync::watch, task::JoinHandle, time};
use tracing::{debug, instrument};

use crate::{
    config::Args,
    error::{Error, Result},
    tunnel::{SocketSpec, TunnelOptions, TunnelRemoteOptions},
    Tunnel,
};

use self::strategy::LBStrategy;

pub mod strategy;

#[derive(Debug, Default, Clone)]
pub struct TunnelStats {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    pub errors: u64,
}

type RemotesMap = IndexMap<SocketSpec, RemoteInfo, fxhash::FxBuildHasher>;
type DeadRemotesMap = IndexMap<SocketSpec, DeadRemote, fxhash::FxBuildHasher>;

#[derive(Debug)]
pub struct TunnelInfo {
    pub stats: TunnelStats,
    pub close_channel: watch::Sender<bool>,
    pub remotes: RemotesMap,
    pub dead_remotes: DeadRemotesMap,
    pub options: TunnelOptions,
    lb_strategy: Box<dyn LBStrategy + Send + Sync + 'static>,
    last_selected_index: Option<usize>,
}

impl TunnelInfo {
    pub fn new(
        close_channel: watch::Sender<bool>,
        remotes: Vec<SocketSpec>,
        options: TunnelOptions,
    ) -> Self {
        let lb_strategy = options.lb_strategy.create();
        TunnelInfo {
            stats: TunnelStats::default(),
            close_channel,
            remotes: remotes
                .into_iter()
                .map(|k| (k, RemoteInfo::default()))
                .collect(),
            dead_remotes: IndexMap::with_hasher(fxhash::FxBuildHasher::default()),
            lb_strategy,
            options,
            last_selected_index: None,
        }
    }
}

impl TunnelInfo {
    pub fn select_remote(&mut self) -> Result<SocketSpec> {
        let size = self.remotes.len();
        let idx = if size == 0 {
            return Err(Error::NoRemote);
        } else if size == 1 {
            0
        } else {
            self.lb_strategy.select_remote(self)?
        };
        self.last_selected_index = Some(idx);
        self.remotes
            .get_index(idx)
            .map(|(k, _)| k)
            .ok_or(Error::NoRemote)
            .cloned()
    }
}

#[derive(Debug, Default, Clone)]
pub struct RemoteInfo {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub streams_pending: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    pub last_error_time: Option<Instant>,
    pub num_errors: u64,
    pub total_errors: u64,
}

#[derive(Debug, Default)]
pub struct DeadRemote {
    pub remote: RemoteInfo,
    pub join_handle: Option<JoinHandle<()>>,
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

    pub fn select_remote(
        &self,
        tunnel_key: &SocketSpec,
    ) -> Result<(SocketSpec, TunnelRemoteOptions)> {
        let mut ti = self
            .inner
            .tunnels
            .get_mut(tunnel_key)
            .ok_or(Error::TunnelDoesNotExist)?;
        let selected = ti.select_remote()?;
        let remote = ti
            .remotes
            .get_mut(&selected)
            .ok_or_else(|| Error::NoRemote)?;

        remote.streams_pending += 1;
        Ok((selected, ti.options.options.clone()))
    }

    pub fn remote_retries(&self, tunnel_key: &SocketSpec) -> Result<u16> {
        let ti = self
            .inner
            .tunnels
            .get(tunnel_key)
            .ok_or(Error::TunnelDoesNotExist)?;
        Ok(ti.options.remote_connect_retries)
    }

    pub(crate) fn add_tunnel(
        &self,
        tunnel: Tunnel,
        close_channel: watch::Sender<bool>,
    ) -> Result<()> {
        if self.inner.tunnels.contains_key(&tunnel.local) {
            return Err(Error::TunnelExists);
        }
        let info = TunnelInfo::new(
            close_channel,
            tunnel.remote,
            tunnel.options.unwrap_or_default(),
        );
        self.inner.tunnels.insert(tunnel.local, info);
        Ok(())
    }

    pub(crate) fn remove_tunnel(&self, local: &SocketSpec) -> Result<TunnelInfo> {
        self.inner
            .tunnels
            .remove(local)
            .map(|(_, mut t)| {
                t.dead_remotes.iter_mut().for_each(|(_, dead)| {
                    if let Some(handle) = dead.join_handle.take() {
                        handle.abort()
                    }
                });
                t
            })
            .ok_or(Error::TunnelDoesNotExist)
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
                rec.streams_pending -= 1;
                rec.num_errors = 0;
            }
        };
    }

    pub fn remote_error(
        &self,
        local: &SocketSpec,
        remote: &SocketSpec,
        _client_addr: &SocketAddr,
        options: &TunnelRemoteOptions,
    ) {
        if let Some(mut tunnel) = self.inner.tunnels.get_mut(local) {
            tunnel.stats.errors += 1;
            let mut is_dead = false;
            if let Some(remote_info) = tunnel.remotes.get_mut(remote) {
                remote_info.total_errors += 1;
                remote_info.num_errors += 1;
                remote_info.last_error_time = Some(Instant::now());
                remote_info.streams_pending -= 1;
                is_dead = remote_info.num_errors >= options.errors_till_dead;
            }

            if is_dead {
                if let Some(rec) = tunnel.remotes.remove(remote) {
                    let join_handle = self.check_dead(
                        local.clone(),
                        remote.clone(),
                        Duration::from_secs_f32(options.connect_timeout),
                        Duration::from_secs_f32(10.0),
                    ); //TODO: from options
                    tunnel.dead_remotes.insert(
                        remote.clone(),
                        DeadRemote {
                            remote: rec,
                            join_handle: Some(join_handle),
                        },
                    );
                    debug!("Tunnel remote {} moved to dead remotes", remote);
                }
            }
        }
    }

    #[instrument(skip_all, fields(tunnel=%local, remote=%remote))]
    fn check_dead(
        &self,
        local: SocketSpec,
        remote: SocketSpec,
        timeout: Duration,
        after: Duration,
    ) -> JoinHandle<()> {
        // spawn task after given duration
        // check that can connect to remote, which should be in dead remotes
        // if OK move Remote info to active remotes and reset error count
        // if not cannot connect just increase error count and timestamp and reschedule check_dead
        let remote = remote.clone();
        let state = self.clone();
        let local = local.clone();
        let f = async move {
            time::sleep(after).await;

            match time::timeout(timeout, TcpStream::connect(remote.as_tuple())).await {
                Ok(Ok(_conn)) => {
                    if let Some(mut tunnel) = state.inner.tunnels.get_mut(&local) {
                        if let Some(DeadRemote {
                            remote: mut rec, ..
                        }) = tunnel.dead_remotes.remove(&remote)
                        {
                            rec.num_errors = 0;
                            debug!(
                                "Tunnel remote {} is live again, removed from dead remotes",
                                remote
                            );
                            tunnel.remotes.insert(remote, rec);
                        }
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    if let Some(mut tunnel) = state.inner.tunnels.get_mut(&local) {
                        if let Some(DeadRemote {
                            remote: ref mut remote_info,
                            ref mut join_handle,
                        }) = tunnel.dead_remotes.get_mut(&remote)
                        {
                            remote_info.total_errors += 1;
                            remote_info.num_errors += 1;
                            remote_info.last_error_time = Some(Instant::now());

                            let new_handle = state.check_dead(local, remote, timeout, after);
                            *join_handle = Some(new_handle);
                        }
                    }
                }
            }
        };
        return tokio::spawn(f);
    }

    pub fn update_transferred(
        &self,
        local: &SocketSpec,
        remote: &SocketSpec,
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
            if let Some(rec) = rec.remotes.get_mut(remote) {
                if sent {
                    rec.bytes_sent += bytes;
                } else {
                    rec.bytes_received += bytes;
                }
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

    pub fn stats(&self) -> Vec<(SocketSpec, TunnelStats)> {
        let iter = self.inner.tunnels.iter();
        iter.map(|i| (i.key().clone(), i.value().stats.clone()))
            .collect()
    }

    pub fn remotes(&self, local: &SocketSpec) -> Result<(Vec<(SocketSpec, RemoteInfo)>, usize)> {
        self.inner
            .tunnels
            .get(local)
            .map(|t| {
                (
                    t.remotes
                        .iter()
                        .map(|r| (r.0.clone(), r.1.clone()))
                        .collect(),
                    t.dead_remotes.len(),
                )
            })
            .ok_or(Error::TunnelDoesNotExist)
    }

    pub fn tunnel_options(&self, local: &SocketSpec) -> Result<TunnelOptions> {
        self.inner
            .tunnels
            .get(local)
            .map(|ti| ti.options.clone())
            .ok_or(Error::TunnelDoesNotExist)
    }

    pub fn copy_buffer_size(&self) -> usize {
        let config = self.inner.config.read();
        config.copy_buffer_size
    }

    pub fn establish_remote_connection_timeout(&self) -> f32 {
        self.inner.config.read().remote_timeout
    }

    pub fn establish_remote_connection_retries(&self) -> u16 {
        self.inner.config.read().remote_retries
    }
}
