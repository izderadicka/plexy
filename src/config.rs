use crate::error::Result;
use crate::Tunnel;
use clap::Parser;
use std::net::SocketAddr;

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
        help = "initial tunnels as tunnel specification like local_addr:port=remote_addr:port,other_address:other_port, use --help-tunnel for details"
    )]
    pub tunnels: Option<Vec<String>>,

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

    #[arg(
        long,
        default_value = "3",
        help = "number of retries for establishing remote connection"
    )]
    pub establish_remote_connection_retries: u16,

    #[arg(long, help = "detailed help on tunnel specification syntax")]
    pub help_tunnel: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            control_socket: "127.0.0.1:9999".parse().unwrap(),
            tunnels: None,
            copy_buffer_size: 8192,
            establish_remote_connection_timeout: 10.0,
            establish_remote_connection_retries: 3,
            help_tunnel: false,
        }
    }
}

impl Args {
    pub fn take_tunnels(&mut self) -> Result<Vec<Tunnel>> {
        let tunnels = self.tunnels.take();
        if let Some(tunnels) = tunnels {
            tunnels.into_iter().map(|s| s.parse()).collect()
        } else {
            Ok(vec![])
        }
    }

    pub fn tunnel_help() {
        println!("
    Tunnel specification consists of three parts, local_socket, where program is listening,
    list of remote_sockets, where connections are proxied and eventually options for this tunnel 
    in square brackets, so it looks like:

        local_socket=remote_socket[,remote_socket ...][\\[options\\]]

    socket is specified either by port number only, then address part is automatically IPv4 local loop - 127.0.0.1,
    or it's host IP address (IPv4 or IPv6) or host name (that resolves locally to IP address). 
    You can have more then 1 remote socket addresses, in that case connections are load balanced between 
    remote hosts.
    
    Options must be in [ ] at the end of tunnel specification and they are key value parts separated by comma,
    like key1=value1,... Valid options are:
    
    # Load balancing strategy
    strategy=[random|round-robin|minimum-open-connections]
    # Timeout for remote connection - seconds, allows decimals
    timeout=<seconds>
    # Retries for remote connection before failing the connection
    retries=<n>

    Examples of tunnel specifications:
        localhost:4444=some.remote.host.net:3333
        0.0.0.0:4444=192.168.33.5:3333,192.168.34.23:3333[strategy=random]
        3000=3001,3002,3003[strategy=min-open-connections]
        [::1]:3000=[::1]:3001,[::1]:3002,[::1]:3003[strategy=round-robin,timeout=2]

        ")
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
