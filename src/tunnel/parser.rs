use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while_m_n},
    character::{
        complete::{alpha1, alphanumeric1, char, u8},
        is_alphanumeric,
    },
    combinator::{map, recognize, verify},
    multi::many0_count,
    sequence::{pair, tuple},
    IResult,
};

use super::SocketSpec;

fn port(i: &str) -> IResult<&str, u16> {
    nom::character::complete::u16(i)
}

fn is_other_hostname_char(c: char) -> bool {
    ".-".contains(c)
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

fn ipv6(i: &str) -> IResult<&str, &str> {
    todo!()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
