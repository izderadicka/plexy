# Plexy

Simple flexible dynamic TCP proxy, all asynchronous and written in Rust.

## About

TCP proxy, where tunnels can be opened, changed and closed in dynamically in runtime.
Also supports:
- load balancing tunnel to several backends with few simple (choosable) loadbalancing strategies
- TLS termination - tunnel can terminate TLS for backends 
- simple line base control protocol (can control proxy via telnet, netcat ...)
- JSONPRC API for programatic control
- metrics collections to Prometheus (and possibly to OpenTelemetry)


## Usage

Basically you can open as many tunnels as you want, either you specify them as argument on program start or you can add them when program is running.

Tunnel is specified by this expression: `local_socket=remote_socket[,remote_socket ...][\[options\]]`, e.g. local TCP socket address (address/host:port), 
list of remote socket addresses and and possibly some options in square brackets.  Run `plexy --help-tunnel` for more details about tunnel definition.

Once started you can interact with plexy either by simple line protocol (listening on port defined by `--control-socket` argument and/or by [JSONRPC](https://www.jsonrpc.org/specification) protocol on port defined on `rpc-socket` argument.
Via control protocol/API you can add remote ends to tunnels, create new tunnels, close tunnels, get tunnel info etc.

Metrics from plexy can be sent to Prometheus (if `--prometheus-socket` argument is passed).

### Example of interaction with plexy via simple command line protocol:

```
$ nc localhost 9999
status
OK: Tunnels: 2
status full
OK: Tunnels: 2
	127.0.0.1:4444 = open conns 0, total 0, bytes sent 0, received 0
	127.0.0.1:3000 = open conns 10, total 11, bytes sent 1540760, received 27767325
close 127.0.0.1:3000
OK
status
OK: Tunnels: 1
open 127.0.0.1:3000=127.0.0.1:3333
OK
status full
OK: Tunnels: 2
	127.0.0.1:4444 = open conns 0, total 0, bytes sent 0, received 0
	127.0.0.1:3000 = open conns 10, total 10, bytes sent 1393920, received 25121827
help
OK: commands
	OPEN tunnel
	CLOSE socket_address
	STATUS [full|long]
	EXIT
	HELP
exit

```

## License
[MIT](https://opensource.org/licenses/MIT)
