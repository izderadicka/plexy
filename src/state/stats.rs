use std::time::SystemTime;

use opentelemetry::metrics::{self, Meter};
use serde::{Serialize, Serializer};

#[derive(Debug, Default, Clone, Serialize)]
pub struct TunnelStats {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    pub errors: u64,
}

#[cfg(feature = "metrics")]
#[derive(Debug)]
pub struct TunnelMetrics {
    pub bytes_sent: metrics::Counter<u64>,
    pub streams_open: metrics::UpDownCounter<i64>,
    pub bytes_received: metrics::Counter<u64>,
    pub total_connections: metrics::Counter<u64>,
    pub errors: metrics::Counter<u64>,
}

#[cfg(feature = "metrics")]
impl TunnelMetrics {
    pub fn new(meter: &opentelemetry::metrics::Meter) -> Self {
        TunnelMetrics {
            bytes_sent: meter
                .u64_counter("tunnel_bytes_sent")
                .with_description("total bytes sent by tunnel")
                .init(),
            streams_open: meter
                .i64_up_down_counter("tunnel_streams_open")
                .with_description("number of currently opened connections")
                .init(),
            bytes_received: meter
                .u64_counter("tunnel_bytes_received")
                .with_description("total bytes received by tunnel")
                .init(),
            total_connections: meter
                .u64_counter("tunnel_total_connections")
                .with_description("total number of connections per whole tunnel life")
                .init(),
            errors: meter
                .u64_counter("tunnel_errors")
                .with_description("total number of errors per whole tunnel life")
                .init(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct RemoteStats {
    pub bytes_sent: u64,
    pub streams_open: usize,
    pub streams_pending: usize,
    pub bytes_received: u64,
    pub total_connections: u64,
    #[serde(serialize_with = "to_epoch_millis")]
    pub last_error_time: Option<SystemTime>,
    pub num_errors: u64,
    pub total_errors: u64,
}

#[cfg(feature = "metrics")]
#[derive(Debug)]
pub struct RemoteMetrics {
    pub bytes_sent: opentelemetry::metrics::Counter<u64>,
    pub streams_open: opentelemetry::metrics::UpDownCounter<i64>,
    pub streams_pending: opentelemetry::metrics::UpDownCounter<i64>,
    pub bytes_received: opentelemetry::metrics::Counter<u64>,
    pub total_connections: opentelemetry::metrics::Counter<u64>,
    pub num_errors: opentelemetry::metrics::UpDownCounter<i64>,
    pub total_errors: opentelemetry::metrics::Counter<u64>,
}

#[cfg(feature = "metrics")]
impl RemoteMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            bytes_sent: meter
                .u64_counter("remote_bytes_sent")
                .with_description("bytes send via remote connection")
                .init(),
            streams_open: meter
                .i64_up_down_counter("remote_streams_open")
                .with_description("number of currently opened remote connections")
                .init(),
            streams_pending: meter
                .i64_up_down_counter("remote_streams_pending")
                .with_description("number of remote connections waiting to open")
                .init(),
            bytes_received: meter
                .u64_counter("remote_bytes_received")
                .with_description("bytes receive from remote connection")
                .init(),
            total_connections: meter
                .u64_counter("remote_total_connections")
                .with_description("total remote connections")
                .init(),
            num_errors: meter
                .i64_up_down_counter("remote_errors")
                .with_description("number of consequent errors")
                .init(),
            total_errors: meter
                .u64_counter("retote_total_error")
                .with_description("total number of errors")
                .init(),
        }
    }
}

fn to_epoch_millis<S>(
    time: &Option<SystemTime>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ts = time.and_then(|t| {
        t.duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .ok()
    });
    match ts {
        Some(v) => serializer.serialize_some(&v),
        None => serializer.serialize_none(),
    }
}
