# Plexy

Simple flexible dynamic TCP proxy, all asynchronous and written in Rust.

## About

TCP proxy, where tunnels can be opened and closed in runtime.
Tunnel is specified by this expression: `local_address:local_port=remote_address:remote_port`
Plexy listens on port (9999 by default) for simple control commands (telnet like protocol).

Example of interaction with plexy:

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