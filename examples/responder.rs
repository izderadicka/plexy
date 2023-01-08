use std::net::SocketAddr;

use clap::Parser;
use futures::{StreamExt, TryFutureExt};
use plexy::tunnel::SocketSpec;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec;
use tracing::{debug, error, info};

#[derive(Debug, Parser)]
struct Args {
    #[arg(required = true, num_args=1..=1024, help = "Addresses to listen on")]
    addr: Vec<SocketSpec>,

    #[arg(long, help = "use HTTP protocol (simple)")]
    http: bool,
}

async fn respond(socket: TcpStream, client_addr: SocketAddr, my_addr: SocketSpec) {
    debug!(client = %client_addr, "Client connected");
    let line_codec = codec::LinesCodec::new_with_max_length(8192);

    let framed = codec::Framed::new(socket, line_codec);
    let (sink, stream) = framed.split::<String>();
    let responses = stream.map(|req| match req {
        Ok(msg) => Ok(format!("[{}] ECHO: {}", my_addr, msg)),
        Err(e) => Ok(format!("[{}] ERROR: {}", my_addr, e)),
    });
    if let Err(e) = responses.forward(sink).await {
        error!(error=%e, "Error in protocol")
    }
    debug!(client = %client_addr, "Client disconnected");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    for addr in args.addr {
        let addr2 = addr.clone();
        tokio::spawn(
            async move {
                let listener = TcpListener::bind(addr.as_tuple()).await?;
                info!(address=%addr, "Started responder");
                while let Ok((socket, client_addr)) = listener.accept().await {
                    if args.http {
                        tokio::spawn(simple_http::respond_http(socket, client_addr, addr.clone()));
                    } else {
                        tokio::spawn(respond(socket, client_addr, addr.clone()));
                    }
                }
                Ok::<_, anyhow::Error>(())
            }
            .map_err(move |e| error!(error=%e, address=%addr2, "Error listening")),
        );
    }

    futures::future::pending::<()>().await;
    Ok(())
}

mod simple_http {
    use std::{fmt, io, net::SocketAddr};

    use bytes::BytesMut;
    use futures::{SinkExt, StreamExt};
    use http::{HeaderValue, Request, Response};
    use plexy::tunnel::SocketSpec;
    use tokio::net::TcpStream;
    use tokio_util::codec::{Decoder, Encoder, Framed};
    use tracing::{debug, error};

    pub struct Http;

    pub async fn respond_http(socket: TcpStream, client_addr: SocketAddr, my_addr: SocketSpec) {
        debug!(client = %client_addr, "Client connected");
        let mut transport = Framed::new(socket, Http);

        while let Some(request) = transport.next().await {
            match request {
                Ok(_request) => {
                    let response = {
                        let mut response = Response::builder();
                        response = response.header("Content-Type", "text/plain");
                        response.body(format!("From {}", my_addr)).unwrap()
                    };
                    if let Err(e) = transport.send(response).await {
                        error!("Error sending response: {}", e);
                    }
                }
                Err(e) => {
                    error!("Request error: {}", e);
                    break;
                }
            }
        }
        debug!(client = %client_addr, "Client disconnected");
    }
    /// Implementation of encoding an HTTP response into a `BytesMut`, basically
    /// just writing out an HTTP/1.1 response.
    impl Encoder<Response<String>> for Http {
        type Error = io::Error;

        fn encode(&mut self, item: Response<String>, dst: &mut BytesMut) -> io::Result<()> {
            use std::fmt::Write;

            write!(
                BytesWrite(dst),
                "\
             HTTP/1.1 {}\r\n\
             Server: Example\r\n\
             Content-Length: {}\r\n\
             Date: {}\r\n\
             ",
                item.status(),
                item.body().len(),
                date::now()
            )
            .unwrap();

            for (k, v) in item.headers() {
                dst.extend_from_slice(k.as_str().as_bytes());
                dst.extend_from_slice(b": ");
                dst.extend_from_slice(v.as_bytes());
                dst.extend_from_slice(b"\r\n");
            }

            dst.extend_from_slice(b"\r\n");
            dst.extend_from_slice(item.body().as_bytes());

            return Ok(());

            // Right now `write!` on `Vec<u8>` goes through io::Write and is not
            // super speedy, so inline a less-crufty implementation here which
            // doesn't go through io::Error.
            struct BytesWrite<'a>(&'a mut BytesMut);

            impl fmt::Write for BytesWrite<'_> {
                fn write_str(&mut self, s: &str) -> fmt::Result {
                    self.0.extend_from_slice(s.as_bytes());
                    Ok(())
                }

                fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
                    fmt::write(self, args)
                }
            }
        }
    }

    /// Implementation of decoding an HTTP request from the bytes we've read so far.
    /// This leverages the `httparse` crate to do the actual parsing and then we use
    /// that information to construct an instance of a `http::Request` object,
    /// trying to avoid allocations where possible.
    impl Decoder for Http {
        type Item = Request<()>;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Request<()>>> {
            // TODO: we should grow this headers array if parsing fails and asks
            //       for more headers
            let mut headers = [None; 16];
            let (method, path, version, amt) = {
                let mut parsed_headers = [httparse::EMPTY_HEADER; 16];
                let mut r = httparse::Request::new(&mut parsed_headers);
                let status = r.parse(src).map_err(|e| {
                    let msg = format!("failed to parse http request: {:?}", e);
                    io::Error::new(io::ErrorKind::Other, msg)
                })?;

                let amt = match status {
                    httparse::Status::Complete(amt) => amt,
                    httparse::Status::Partial => return Ok(None),
                };

                let toslice = |a: &[u8]| {
                    let start = a.as_ptr() as usize - src.as_ptr() as usize;
                    assert!(start < src.len());
                    (start, start + a.len())
                };

                for (i, header) in r.headers.iter().enumerate() {
                    let k = toslice(header.name.as_bytes());
                    let v = toslice(header.value);
                    headers[i] = Some((k, v));
                }

                let method = http::Method::try_from(r.method.unwrap())
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                (
                    method,
                    toslice(r.path.unwrap().as_bytes()),
                    r.version.unwrap(),
                    amt,
                )
            };
            if version != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "only HTTP/1.1 accepted",
                ));
            }
            let data = src.split_to(amt).freeze();
            let mut ret = Request::builder();
            ret = ret.method(method);
            let s = data.slice(path.0..path.1);
            let s = unsafe { String::from_utf8_unchecked(Vec::from(s.as_ref())) };
            ret = ret.uri(s);
            ret = ret.version(http::Version::HTTP_11);
            for header in headers.iter() {
                let (k, v) = match *header {
                    Some((ref k, ref v)) => (k, v),
                    None => break,
                };
                let value = HeaderValue::from_bytes(data.slice(v.0..v.1).as_ref())
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "header decode error"))?;
                ret = ret.header(&data[k.0..k.1], value);
            }

            let req = ret
                .body(())
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            Ok(Some(req))
        }
    }

    mod date {
        use std::cell::RefCell;
        use std::fmt::{self, Write};
        use std::str;
        use std::time::SystemTime;

        use httpdate::HttpDate;

        pub struct Now(());

        /// Returns a struct, which when formatted, renders an appropriate `Date`
        /// header value.
        pub fn now() -> Now {
            Now(())
        }

        // Gee Alex, doesn't this seem like premature optimization. Well you see
        // there Billy, you're absolutely correct! If your server is *bottlenecked*
        // on rendering the `Date` header, well then boy do I have news for you, you
        // don't need this optimization.
        //
        // In all seriousness, though, a simple "hello world" benchmark which just
        // sends back literally "hello world" with standard headers actually is
        // bottlenecked on rendering a date into a byte buffer. Since it was at the
        // top of a profile, and this was done for some competitive benchmarks, this
        // module was written.
        //
        // Just to be clear, though, I was not intending on doing this because it
        // really does seem kinda absurd, but it was done by someone else [1], so I
        // blame them!  :)
        //
        // [1]: https://github.com/rapidoid/rapidoid/blob/f1c55c0555007e986b5d069fe1086e6d09933f7b/rapidoid-commons/src/main/java/org/rapidoid/commons/Dates.java#L48-L66

        struct LastRenderedNow {
            bytes: [u8; 128],
            amt: usize,
            unix_date: u64,
        }

        thread_local!(static LAST: RefCell<LastRenderedNow> = RefCell::new(LastRenderedNow {
            bytes: [0; 128],
            amt: 0,
            unix_date: 0,
        }));

        impl fmt::Display for Now {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                LAST.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    let now = SystemTime::now();
                    let now_unix = now
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|since_epoch| since_epoch.as_secs())
                        .unwrap_or(0);
                    if cache.unix_date != now_unix {
                        cache.update(now, now_unix);
                    }
                    f.write_str(cache.buffer())
                })
            }
        }

        impl LastRenderedNow {
            fn buffer(&self) -> &str {
                str::from_utf8(&self.bytes[..self.amt]).unwrap()
            }

            fn update(&mut self, now: SystemTime, now_unix: u64) {
                self.amt = 0;
                self.unix_date = now_unix;
                write!(LocalBuffer(self), "{}", HttpDate::from(now)).unwrap();
            }
        }

        struct LocalBuffer<'a>(&'a mut LastRenderedNow);

        impl fmt::Write for LocalBuffer<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                let start = self.0.amt;
                let end = start + s.len();
                self.0.bytes[start..end].copy_from_slice(s.as_bytes());
                self.0.amt += s.len();
                Ok(())
            }
        }
    }
}
