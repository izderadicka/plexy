use std::{io, net::SocketAddr};

use futures::SinkExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;
use tracing::{debug, error};

use crate::{
    controller::protocol::{Command, CommandResponse},
    State,
};

use self::protocol::split_socket;

mod protocol;

async fn control_loop(mut socket: TcpStream, ctx: State) {
    let (mut reader, mut writer) = split_socket(&mut socket);

    while let Some(cmd) = reader.next().await {
        let resp = match cmd {
            Ok(cmd) => {
                debug!("Command received: {:?}", cmd);
                cmd.exec(ctx.clone()).await
            }
            Err(e) => {
                error!(error = %e, "Protocol error");
                CommandResponse::Problem(Some(e))
            }
        };
        if matches!(resp, CommandResponse::Done) {
            break;
        }

        if let Err(e) = writer.send(resp).await {
            error!(error = %e, "Cannot send response");
        }
    }
    debug!("Closed control connection")
}

pub async fn run_controller(socket_addr: SocketAddr, ctx: State) -> io::Result<()> {
    let listener = TcpListener::bind(socket_addr).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(control_loop(socket, ctx.clone()));
    }
}
