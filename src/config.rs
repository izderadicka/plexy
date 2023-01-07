use clap::Parser;
use std::net::SocketAddr;

use crate::Tunnel;

#[derive(Parser, Clone, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[arg(
        short,
        long,
        help = "socket address to listen for control commands",
        default_value = "127.0.0.1:9999"
    )]
    pub control_socket: SocketAddr,

    #[arg(
        num_args = 0..1024,
        help = "initial tunnels as port=>remote_addr:port, or local_addr:port=>remote_addr:port - either as separate arguments or separated by comma"
    )]
    pub tunnels: Option<Vec<Tunnel>>,

    #[arg(
        long,
        default_value = "8192",
        help = "size of buffer used in tunnel stream copying"
    )]
    pub copy_buffer_size: usize,

    #[arg(
        long,
        default_value = "10",
        help = "timeout for establishing remote connection is seconds (decimals allowed)"
    )]
    pub establish_remote_connection_timeout: f32,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            control_socket: "127.0.0.1:9999".parse().unwrap(),
            tunnels: None,
            copy_buffer_size: 8192,
            establish_remote_connection_timeout: 10.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_cli() {
        let args = Args::try_parse_from(&[
            "plexy",
            "--control-socket",
            "0.0.0.0:9999",
            "3333=127.0.0.1:3000",
            "0.0.0.0:4444=127.0.0.1:4000",
        ])
        .expect("valid params");
        assert_eq!(2, args.tunnels.unwrap().len());
    }
}
