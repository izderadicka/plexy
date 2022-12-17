use clap::Parser;
use std::net::SocketAddr;

use crate::Tunnel;

#[derive(Parser)]
#[command(author, version, about)]
pub struct Args {
    #[arg(
        short,
        long,
        help = "Socket address to listen for control commands",
        default_value = "127.0.0.1:9999"
    )]
    pub control_socket: SocketAddr,

    #[arg(
        num_args = 0..1024,
        help = "initial tunnels as port=>remote_addr:port, or local_addr:port=>remote_addr:port - either as separate arguments or separated by comma"
    )]
    pub tunnels: Vec<Tunnel>,
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
        assert_eq!(2, args.tunnels.len());
    }
}
