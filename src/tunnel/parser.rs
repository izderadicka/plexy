use nom::{
    branch::alt,
    bytes::complete::{tag, take_till, take_while, take_while_m_n},
    character::complete::{alpha1, char, u8},
    combinator::{all_consuming, map, opt, recognize, verify},
    multi::separated_list1,
    sequence::{delimited, pair, separated_pair, tuple},
    IResult,
};

use crate::Tunnel;

use super::{SocketSpec, TunnelOptions};

fn port(i: &str) -> IResult<&str, u16> {
    nom::character::complete::u16(i)
}

fn is_other_hostname_char(c: char) -> bool {
    ".-".contains(c)
}

fn is_option_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || '_' == c
}

fn host_name(i: &str) -> IResult<&str, &str> {
    let rest = take_while(|x: char| x.is_ascii_alphanumeric() || is_other_hostname_char(x));
    verify(recognize(pair(alpha1, rest)), |x: &str| {
        x.chars()
            .last()
            .map(|c| !is_other_hostname_char(c))
            .unwrap_or(false)
    })(i)
}

fn ipv6_segment(i: &str) -> IResult<&str, &str> {
    take_while_m_n(0, 4, |c: char| c.is_ascii_hexdigit())(i)
}

fn ipv6(i: &str) -> IResult<&str, &str> {
    delimited(
        tag("["),
        recognize(separated_list1(tag(":"), ipv6_segment)),
        tag("]"),
    )(i)
}

fn ipv4(i: &str) -> IResult<&str, &str> {
    recognize(tuple((u8, char('.'), u8, char('.'), u8, char('.'), u8)))(i)
}
fn socket_spec1(i: &str) -> IResult<&str, SocketSpec> {
    map(port, |port| SocketSpec {
        port,
        host: "127.0.0.1".into(),
    })(i)
}

fn socket_spec2(i: &str) -> IResult<&str, SocketSpec> {
    map(
        separated_pair(alt((host_name, ipv4, ipv6)), char(':'), port),
        |(host, port)| SocketSpec {
            host: host.into(),
            port,
        },
    )(i)
}

pub(super) fn socket_spec(i: &str) -> IResult<&str, SocketSpec> {
    alt((socket_spec2, socket_spec1))(i)
}

fn options(i: &str) -> IResult<&str, TunnelOptions> {
    fn err(input: &str) -> nom::Err<nom::error::Error<&str>> {
        nom::Err::Failure(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Verify,
        })
    }
    separated_list1(
        char(','),
        separated_pair(
            take_while(is_option_name_char),
            char('='),
            take_till(|c| ",]".contains(c)),
        ),
    )(i)
    .and_then(|(rest, items)| {
        let mut options = TunnelOptions::default();
        for (k, v) in items {
            match k.to_lowercase().as_str() {
                "strategy" => options.lb_strategy = v.parse().map_err(|_| err(v))?,
                "retries" => options.remote_connect_retries = v.parse().map_err(|_| err(v))?,
                "timeout" => {
                    options.options.remote_connect_timeout = v.parse().map_err(|_| err(v))?
                }
                _ => return Err(err(k)),
            }
        }
        Ok((rest, options))
    })
}

pub(super) fn tunnel(i: &str) -> IResult<&str, Tunnel> {
    all_consuming(map(
        separated_pair(
            socket_spec,
            char('='),
            tuple((
                separated_list1(char(','), socket_spec),
                opt(delimited(char('['), options, char(']'))),
            )),
        ),
        |(local, (remote, options))| Tunnel {
            local,
            remote,
            options,
        },
    ))(i)
}

#[cfg(test)]
mod tests {
    use crate::state::strategy::TunnelLBStrategy;

    use super::*;

    #[test]
    fn test_ipv6() {
        let s = |s: &'static str| &s[1..s.len() - 1];
        let ip1 = "[2001:db8:3333:4444:5555:6666:7777:8888]";
        let (_rest, res) = ipv6(ip1).expect("valid ipv6 addr");
        assert_eq!(s(ip1), res);
        let ip2 = "[2001:db8:3333:4444:CCCC:DDDD:EEEE:FFFF]";
        let (_rest, res) = ipv6(ip2).expect("valid ipv6 addr");
        assert_eq!(s(ip2), res);
        let ip3 = "[::]";
        let (_rest, res) = ipv6(ip3).expect("valid ipv6 addr");
        assert_eq!(s(ip3), res);

        let ip4 = "[::1234:5678]";
        let (_rest, res) = ipv6(ip4).expect("valid ipv6 addr");
        assert_eq!(s(ip4), res);

        let ip5 = "[2001:db8::]";
        let (_rest, res) = ipv6(ip5).expect("valid ipv6 addr");
        assert_eq!(s(ip5), res);

        let ip6 = "[2001:db8::1234:5678]";
        let (_rest, res) = ipv6(ip6).expect("valid ipv6 addr");
        assert_eq!(s(ip6), res);
    }

    #[test]
    fn test_ipv4() {
        let x = "12.138.34.5";
        let (rest, num) = ipv4(x).expect("valid IP address");
        assert_eq!(x, num);
        assert_eq!("", rest);

        let x = "12.138.34.500";
        let res = ipv4(x);
        assert!(res.is_err());
    }

    #[test]
    fn test_port() {
        let x = "12345";
        let (rest, num) = port(x).expect("valid number");
        assert_eq!(x.parse::<u16>().unwrap(), num);
        assert_eq!("", rest);
    }

    #[test]
    fn test_hostname() {
        let x = "localhost";
        let (_rest, name) = host_name(x).expect("valid hostname");
        assert_eq!(x, name);

        let y = "doma.ume.cz";

        let (_rest, name) = host_name(y).expect("valid hostname");
        assert_eq!(y, name);

        let z = "neplatne-";
        let res = host_name(z);
        assert!(res.is_err());
    }

    #[test]
    fn test_socket_spec() {
        let x = "localhost:3333";
        let (_rest, s) = socket_spec(x).expect("valid socket address");
        assert_eq!("localhost", s.host.as_ref());
        assert_eq!(3333, s.port);

        let y = "127.0.0.1:3000";
        let (_rest, s) = socket_spec(y).expect("valid socket address");
        assert_eq!("127.0.0.1", s.host.as_ref());
        assert_eq!(3000, s.port);
    }

    #[test]
    fn test_options() {
        let options_str = "strategy=random,retries=3,timeout=10.0";
        let (_rest, res) = options(options_str).unwrap();
        assert_eq!(3, res.remote_connect_retries);
        assert!(matches!(
            res.options
                .remote_connect_timeout
                .partial_cmp(&10.0)
                .unwrap(),
            std::cmp::Ordering::Equal
        ));
        assert!(matches!(res.lb_strategy, TunnelLBStrategy::Random));
    }
}
