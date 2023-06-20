#[cfg(feature = "metrics")]
use opentelemetry::metrics::{Meter, UpDownCounter};

use parking_lot::RwLock;
use rustls::ClientConfig;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time};
use tracing::{debug, instrument};

use crate::{
    config::Args,
    connect_remote,
    error::{Error, Result},
    state::tls::create_client_config,
    tunnel::{SocketSpec, TunnelOptions, TunnelRemoteOptions},
    Tunnel,
};

use self::{
    info::{DeadRemote, RemoteInfo, TunnelInfo},
    stats::{RemoteStats, TunnelStats},
};

pub mod info;
pub mod stats;
pub mod strategy;
mod tls;

struct StateInner {
    tunnels: dashmap::DashMap<SocketSpec, TunnelInfo, fxhash::FxBuildHasher>,
    config: RwLock<Args>,
    client_ssl_config: RwLock<Arc<ClientConfig>>,
    #[cfg(feature = "metrics")]
    meter: Meter,
    #[cfg(feature = "metrics")]
    tunnels_counter: UpDownCounter<i64>,
}

#[derive(Clone)]
pub struct State {
    inner: Arc<StateInner>,
}

impl State {
    #[cfg(feature = "metrics")]
    pub fn new(args: Args, meter: Meter) -> Result<Self> {
        Ok(State {
            inner: Arc::new(StateInner {
                tunnels: dashmap::DashMap::with_hasher(fxhash::FxBuildHasher::default()),

                client_ssl_config: RwLock::new(Arc::new(create_client_config(&args)?)),
                config: RwLock::new(args),
                tunnels_counter: meter
                    .i64_up_down_counter("number_of_tunnels")
                    .with_description("Number of tunnels open")
                    .init(),
                meter,
            }),
        })
    }

    #[cfg(not(feature = "metrics"))]
    pub fn new(args: Args) -> Result<Self> {
        Ok(State {
            inner: Arc::new(StateInner {
                tunnels: dashmap::DashMap::with_hasher(fxhash::FxBuildHasher::default()),

                client_ssl_config: RwLock::new(Arc::new(create_client_config(&args)?)),
                config: RwLock::new(args),
            }),
        })
    }

    #[cfg(feature = "metrics")]
    pub fn meter(&self) -> &Meter {
        &self.inner.meter
    }

    pub fn client_ssl_config(&self) -> Arc<ClientConfig> {
        self.inner.client_ssl_config.read().clone()
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

        remote.new_pending_stream(tunnel_key, &selected);
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
            self,
        );
        self.inner.tunnels.insert(tunnel.local, info);
        #[cfg(feature = "metrics")]
        {
            self.inner
                .tunnels_counter
                .add(&opentelemetry::Context::current(), 1, &[]);
        }
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
            .and_then(|ti| {
                #[cfg(feature = "metrics")]
                {
                    self.inner
                        .tunnels_counter
                        .add(&opentelemetry::Context::current(), -1, &[]);
                }
                Ok(ti)
            })
    }

    pub(crate) fn add_remote_to_tunnel(
        &self,
        tunnel: &SocketSpec,
        remote: SocketSpec,
    ) -> Result<()> {
        let mut ti = self
            .inner
            .tunnels
            .get_mut(tunnel)
            .ok_or_else(|| Error::TunnelDoesNotExist)?;
        if !ti.remotes.contains_key(&remote) && !ti.dead_remotes.contains_key(&remote) {
            ti.remotes.insert(remote, RemoteInfo::new(self));
            Ok(())
        } else {
            Err(Error::RemoteExists)
        }
    }

    pub(crate) fn remove_remote_from_tunnel(
        &self,
        tunnel: &SocketSpec,
        remote: &SocketSpec,
    ) -> Result<RemoteInfo> {
        let mut ti = self
            .inner
            .tunnels
            .get_mut(tunnel)
            .ok_or_else(|| Error::TunnelDoesNotExist)?;

        ti.remotes
            .remove(remote)
            .or_else(|| ti.dead_remotes.remove(remote).map(|d| d.remote))
            .ok_or_else(|| Error::RemoteDoesNotExist)
    }

    pub fn tunnel_exists(&self, tunnel: &SocketSpec) -> bool {
        self.inner.tunnels.contains_key(tunnel)
    }

    pub fn number_of_tunnels(&self) -> usize {
        self.inner.tunnels.len()
    }

    pub fn list_tunnels(&self) -> Vec<SocketSpec> {
        self.inner
            .tunnels
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    pub fn client_connected(&self, local: &SocketSpec, client_addr: &SocketAddr) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            rec.client_connected(local, client_addr);
        };
    }

    pub fn remote_connected(
        &self,
        local: &SocketSpec,
        remote: &SocketSpec,
        client_addr: &SocketAddr,
    ) {
        if let Some(mut rec) = self.inner.tunnels.get_mut(local) {
            if let Some(rec) = rec.remotes.get_mut(remote) {
                rec.remote_connected(local, remote, client_addr);
            }
        };
    }

    pub fn remote_error(
        &self,
        local: &SocketSpec,
        remote: &SocketSpec,
        client_addr: &SocketAddr,
        options: &TunnelRemoteOptions,
    ) {
        if let Some(mut tunnel) = self.inner.tunnels.get_mut(local) {
            tunnel.remote_error(local, remote, Some(client_addr));

            let mut is_dead = false;
            if let Some(remote_info) = tunnel.remotes.get_mut(remote) {
                remote_info.error(local, remote, Some(client_addr));
                is_dead = remote_info.stats.num_errors >= options.errors_till_dead;
            }

            if is_dead {
                if let Some(rec) = tunnel.remotes.remove(remote) {
                    let join_handle = self.check_dead(
                        local.clone(),
                        remote.clone(),
                        Duration::from_secs_f32(options.connect_timeout),
                        Duration::from_secs_f32(10.0),
                        options.tls_config(self),
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
        tls_config: Option<Arc<ClientConfig>>,
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

            match time::timeout(timeout, connect_remote(&remote, tls_config.clone())).await {
                Ok(Ok(_conn)) => {
                    if let Some(mut tunnel) = state.inner.tunnels.get_mut(&local) {
                        if let Some(DeadRemote {
                            remote: mut rec, ..
                        }) = tunnel.dead_remotes.remove(&remote)
                        {
                            rec.remote_recovered(&local, &remote);

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
                        tunnel.remote_error(&local, &remote, None);
                        if let Some(DeadRemote {
                            remote: ref mut remote_info,
                            ref mut join_handle,
                        }) = tunnel.dead_remotes.get_mut(&remote)
                        {
                            remote_info.error(&local, &remote, None);

                            let new_handle =
                                state.check_dead(local, remote, timeout, after, tls_config);
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
            rec.update_moved_bytes(sent, bytes, local);
            if let Some(rec) = rec.remotes.get_mut(remote) {
                rec.update_moved_bytes(sent, bytes, local, remote);
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
            rec.client_disconnected(local, remote, _client_addr);
            //TODO: refactor when if let chain will become stable
            if let Some(remote) = remote {
                if let Some(rec) = rec.remotes.get_mut(remote) {
                    rec.client_disconnected(local, remote, _client_addr);
                }
            }
        }
    }

    pub fn stats(&self) -> Vec<(SocketSpec, TunnelStats)> {
        let iter = self.inner.tunnels.iter();
        iter.map(|i| (i.key().clone(), i.value().stats.clone()))
            .collect()
    }

    pub fn info_to<T>(&self, tunnel: &SocketSpec) -> Result<T>
    where
        T: for<'a> From<&'a TunnelInfo> + 'static,
    {
        let ti = self
            .inner
            .tunnels
            .get(tunnel)
            .ok_or(Error::TunnelDoesNotExist)?;
        Ok(ti.value().into())
    }

    pub fn remotes(&self, local: &SocketSpec) -> Result<(Vec<(SocketSpec, RemoteStats)>, usize)> {
        self.inner
            .tunnels
            .get(local)
            .map(|t| {
                (
                    t.remotes
                        .iter()
                        .map(|r| (r.0.clone(), r.1.stats.clone()))
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
