use std::{net::SocketAddr, time::SystemTime};

use indexmap::IndexMap;
use opentelemetry::{Context, KeyValue};
use tokio::{sync::watch, task::JoinHandle};

use crate::{
    error::{Error, Result},
    tunnel::{SocketSpec, TunnelOptions},
    State,
};

use super::{
    stats::{RemoteMetrics, RemoteStats, TunnelMetrics, TunnelStats},
    strategy::LBStrategy,
};

type RemotesMap = IndexMap<SocketSpec, RemoteInfo, fxhash::FxBuildHasher>;
type DeadRemotesMap = IndexMap<SocketSpec, DeadRemote, fxhash::FxBuildHasher>;

#[derive(Debug)]
pub struct DeadRemote {
    pub remote: RemoteInfo,
    pub join_handle: Option<JoinHandle<()>>,
}

#[cfg(feature = "metrics")]
macro_rules! metric_add {
    ($($met:expr => $val: expr),+ ;  $tun: expr) => {
        {
        let ctx = Context::current();
        let attrs = &[KeyValue::new("tunnel", $tun)];
        $(
        $met.add(&ctx, $val, attrs);
        )+
    }
    };

    ($($met:expr => $val: expr),+ ;  $tun: expr, $rem: expr) => {
        {
        let attrs = &[KeyValue::new("tunnel", $tun),  KeyValue::new("remote", $rem)];
        let ctx = Context::current();
        $(
        $met.add(&ctx, $val, attrs);
        )+
        }
    };
}

#[derive(Debug)]
pub struct TunnelInfo {
    pub stats: TunnelStats,
    #[cfg(feature = "metrics")]
    pub metrics: TunnelMetrics,
    pub close_channel: watch::Sender<bool>,
    pub remotes: RemotesMap,
    pub dead_remotes: DeadRemotesMap,
    pub options: TunnelOptions,
    lb_strategy: Box<dyn LBStrategy + Send + Sync + 'static>,
    pub last_selected_index: Option<usize>,
}

impl TunnelInfo {
    pub fn new(
        close_channel: watch::Sender<bool>,
        remotes: Vec<SocketSpec>,
        options: TunnelOptions,
        state: &State,
    ) -> Self {
        let lb_strategy = options.lb_strategy.create();
        TunnelInfo {
            stats: TunnelStats::default(),
            close_channel,
            remotes: remotes
                .into_iter()
                .map(|k| (k, RemoteInfo::new(state)))
                .collect(),
            dead_remotes: IndexMap::with_hasher(fxhash::FxBuildHasher::default()),
            lb_strategy,
            options,
            last_selected_index: None,
            #[cfg(feature = "metrics")]
            metrics: TunnelMetrics::new(state.meter()),
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

    pub(super) fn client_connected(&mut self, tunnel: &SocketSpec, _client: &SocketAddr) {

        self.stats.total_connections += 1;
        self.stats.streams_open += 1;

        #[cfg(feature = "metrics")]
        {
            metric_add!(self.metrics.total_connections => 1 , 
                        self.metrics.streams_open => 1; 
                        tunnel);
        }
    }

    pub (super) fn client_disconnected(&mut self, tunnel: &SocketSpec, _remote: Option<&SocketSpec>, _client: &SocketAddr) {
        self.stats.streams_open -= 1;
        #[cfg(feature="metrics")]
        {
            metric_add!(self.metrics.streams_open => -1; tunnel);
        } 
    }

    pub(super) fn remote_error(&mut self, tunnel: &SocketSpec, remote: &SocketSpec, _client_addr: Option<&SocketAddr>) {
        self.stats.errors += 1;
        #[cfg(feature = "metrics")]
        {
            metric_add!(self.metrics.errors =>1 ; tunnel, remote);
        }
    }

    pub(super) fn update_moved_bytes(&mut self, sent: bool, bytes: u64, tunnel: &SocketSpec) {
        if sent {
            self.stats.bytes_sent += bytes;
            #[cfg(feature = "metrics")]
            {
                metric_add!(self.metrics.bytes_sent => bytes; tunnel)
            }
        } else {
            self.stats.bytes_received += bytes;
            #[cfg(feature = "metrics")]
            {
                metric_add!(self.metrics.bytes_sent => bytes; tunnel)
            }
        }
    }
}

#[derive(Debug)]
pub struct RemoteInfo {
    pub stats: RemoteStats,
    #[cfg(feature = "metrics")]
    pub metrics: RemoteMetrics,
}

impl RemoteInfo {
    pub fn new(_state: &State) -> Self {
        RemoteInfo {
            stats: RemoteStats::default(),
            #[cfg(feature = "metrics")]
            metrics: RemoteMetrics::new(_state.meter()),
        }
    }

    pub(super) fn new_pending_stream(&mut self, tunnel: &SocketSpec, remote: &SocketSpec) {
        self.stats.streams_pending += 1;
        #[cfg(feature = "metrics")]
        {
            metric_add!(self.metrics.streams_pending => 1 ; tunnel, remote);
        }
    }

    pub(crate) fn remote_connected(&mut self, tunnel: &SocketSpec, remote: &SocketSpec, _client_addr: &SocketAddr) {
        #[cfg(feature = "metrics")]
                {
                    metric_add!(
                        self.metrics.streams_open => 1,
                        self.metrics.total_connections => 1,
                        self.metrics.streams_pending => -1,
                        self.metrics.num_errors => - (self.stats.num_errors as i64);
                        tunnel, remote
                    )
                }

                self.stats.streams_open += 1;
                self.stats.total_connections += 1;
                self.stats.streams_pending -= 1;
                self.stats.num_errors = 0;
    }

    pub(crate) fn error(&mut self, local: &SocketSpec, remote: &SocketSpec,   client_addr: Option<&SocketAddr>) {
        self.stats.total_errors += 1;
        self.stats.num_errors += 1;
        self.stats.last_error_time = Some(SystemTime::now());
        // This is not retry on dead remote
        if client_addr.is_some() {
        self.stats.streams_pending -= 1;
        }

        #[cfg(feature = "metrics")]
        {
            let attrs = &[KeyValue::new("tunnel", local), KeyValue::new("remote", remote)];
            let ctx = Context::current();
            self.metrics.total_errors.add(&ctx, 1, attrs);
            self.metrics.num_errors.add(&ctx, 1, attrs);
            if client_addr.is_some() {
            self.metrics.streams_pending.add(&ctx, -1, attrs);
            }
            
        }
    }

    pub(crate) fn remote_recovered(&mut self, tunnel: &SocketSpec, remote: &SocketSpec) {

        #[cfg(feature = "metrics")]
        {
            metric_add!(
                self.metrics.num_errors => - (self.stats.num_errors as i64);
                tunnel, remote
            )
        }
        self.stats.num_errors = 0;
    }

    pub(crate) fn update_moved_bytes(&mut self, sent: bool, bytes: u64, local: &SocketSpec, remote:&SocketSpec) {
        if sent {
            self.stats.bytes_sent += bytes;
            #[cfg(feature = "metrics")]
            {
                metric_add!(self.metrics.bytes_sent => bytes; local, remote);
            }
        } else {
            self.stats.bytes_received += bytes;
            #[cfg(feature = "metrics")]
            {
                metric_add!(self.metrics.bytes_received => bytes; local, remote);
            }
        }
    }

    pub(crate) fn client_disconnected(&mut self, tunnel: &SocketSpec, remote: &SocketSpec, _client_addr: &SocketAddr)  {
        self.stats.streams_open -= 1;
        #[cfg(feature="metrics")]
        {
            metric_add!(self.metrics.streams_open => -1; tunnel, remote);
        }
    }
}
