use bytes::BytesMut;
use tokio_util::codec;

use crate::error::Error;

use super::{CommandRequest, CommandResponse};

pub struct CommandCodec {
    lines_codec: codec::LinesCodec,
}

impl codec::Decoder for CommandCodec {
    type Item = CommandRequest;

    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let res = self.lines_codec.decode(src)?;
        match res {
            Some(line) => {
                let cmd: CommandRequest = line.parse().unwrap_or_else(CommandRequest::Invalid);
                Ok(Some(cmd))
            }
            None => Ok(None),
        }
    }
}

impl codec::Encoder<CommandResponse> for CommandCodec {
    type Error = Error;

    fn encode(&mut self, item: CommandResponse, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let s = item.to_string();
        self.lines_codec.encode(s, dst)?;
        Ok(())
    }
}

impl CommandCodec {
    pub fn new_with_max_length(max_length: usize) -> Self {
        CommandCodec {
            lines_codec: codec::LinesCodec::new_with_max_length(max_length),
        }
    }
}
