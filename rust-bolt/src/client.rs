use std::collections::HashMap;
use std::convert::TryFrom;
use std::iter::FromIterator;
use std::net::IpAddr;

use bytes::*;
use failure::Error;
use tokio::net::TcpStream;
use tokio::prelude::*;

use crate::message::{Chunk, Init, Message};
use crate::value;
use crate::value::Value;

const PREAMBLE: [u8; 4] = [0x60, 0x60, 0xB0, 0x17];
const SUPPORTED_VERSIONS: [u32; 4] = [1, 0, 0, 0];

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub async fn new(host: IpAddr, port: usize) -> Result<Self, Error> {
        let client = Client {
            stream: TcpStream::connect(format!("{}:{}", host, port)).await?,
        };
        Ok(client)
    }

    pub async fn handshake(&mut self) -> Result<u32, Error> {
        let mut allowed_versions = BytesMut::with_capacity(16);
        SUPPORTED_VERSIONS
            .iter()
            .for_each(|&v| allowed_versions.put_u32(v));
        self.stream.write(&PREAMBLE).await?;
        self.stream.write_buf(&mut allowed_versions).await?;
        self.stream.flush().await?;
        Ok(self.stream.read_u32().await?)
    }

    // TODO: Clean this up, this is just an experiment
    // Have to implement conversion from Bytes to value types before we can implement this
    pub async fn init(&mut self) -> Result<Message, Error> {
        println!("Starting init.");
        let init: Init<value::String, value::String> = Init::new(
            "rust-bolt/0.1.0",
            HashMap::from_iter(vec![
                ("scheme", "basic"),
                ("principal", "neo4j"),
                ("credentials", "invalid"),
            ]),
        );
        let bytes = init.try_into_bytes()?;
        let mut message = Message::with_capacity(bytes.len());
        message.add_chunk(Chunk::try_from(bytes)?);
        println!("Created message.");
        let mut bytes: Bytes = message.into();
        self.stream.write_buf(&mut bytes).await?;
        self.stream.flush().await?;
        println!("Wrote init.");
        let mut buf = Vec::new();
        self.stream.read_to_end(&mut buf).await?;
        println!("Read response: {:?}", &buf[..]);
        Ok(Message::try_from(Bytes::from(buf))?)
    }
}
