use std::collections::HashMap;
use std::ops::Add;

use async_std::io::prelude::BufReadExt;
use async_std::io::{self, BufReader, WriteExt};
use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};
use async_std::prelude::*;
use base64ct::{Base64, Encoding};
use sha1::{Digest, Sha1};

use crate::ws::WebSocket;

pub struct WSListener {
    listener: TcpListener,
}

impl WSListener {
    pub async fn bind<A: ToSocketAddrs>(addrs: A) -> io::Result<WSListener> {
        let listener = TcpListener::bind(addrs).await?;
        Ok(WSListener { listener })
    }

    async fn read_handshake(
        reader: &mut BufReader<TcpStream>,
    ) -> io::Result<(String, HashMap<String, Vec<String>>)> {
        let mut initial_line = String::new();
        reader.read_line(&mut initial_line).await?;
        initial_line = initial_line
            .trim_end_matches(|x| x == '\r' || x == '\n')
            .to_string();

        let mut headers: HashMap<String, Vec<String>> = HashMap::new();

        let mut buf = String::new();
        while reader.read_line(&mut buf).await? > 2 {
            let line = buf.trim_end_matches(|x| x == '\r' || x == '\n');
            let split = line.split_once(':');
            if let Some(split) = split {
                let (key, value) = (split.0.to_ascii_lowercase(), split.1.trim().to_string());
                match headers.get_mut(&key) {
                    Some(vec) => vec.push(value),
                    None => {
                        headers.insert(key, vec![value]);
                    }
                };
            }
            buf.clear()
        }

        Ok((initial_line, headers))
    }

    fn handshake(headers: HashMap<String, Vec<String>>) -> io::Result<String> {
        let get_header = |name| headers.get(name).map(|x| x[0].as_str());
        let get_header_or_err = |name| {
            get_header(name).ok_or(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Client used invalid {} header", name),
            ))
        };
        let get_header_containing = |name, v| {
            get_header(name)
                .filter(|x| x.contains(x))
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Client used invalid {} header", name),
                ))
        };
        get_header_containing("connection", "Upgrade")?;
        get_header_containing("upgrade", "WebSocket")?;
        let ws_key = get_header_or_err("sec-websocket-key")?;

        let mut hasher = Sha1::new();

        hasher.update(
            ws_key
                .to_string()
                .add("258EAFA5-E914-47DA-95CA-C5AB0DC85B11")
                .as_bytes(),
        );

        let accept = Base64::encode_string(&hasher.finalize());

        Ok(accept)
    }

    async fn write_ws_error(
        stream: &mut TcpStream,
        code: u16,
        code_msg: &str,
        msg: &str,
    ) -> io::Result<()> {
        let data = format!(
            "HTTP/1.1 {} {}\r\n\
            Content-Length: {}\r\n\
            \r\n\
            {}",
            code,
            code_msg,
            msg.as_bytes().len(),
            msg,
        );

        stream.write(data.as_bytes()).await?;

        Ok(())
    }

    async fn write_ok(stream: &mut TcpStream, accept: String) -> io::Result<()> {
        stream
            .write(
                format!(
                    "HTTP/1.1 101 Switching Protocols\r\n\
                    Upgrade: websocket\r\n\
                    Connection: Upgrade\r\n\
                    Sec-WebSocket-Accept: {}\r\n\
                    \r\n",
                    accept
                )
                .as_bytes(),
            )
            .await?;

        Ok(())
    }

    pub async fn accept(&self) -> io::Result<(String, WebSocket)> {
        let mut stream = self.listener.accept().await?.0;
        let mut reader = io::BufReader::new(stream.clone());

        let (initial_line, headers) = Self::read_handshake(&mut reader).await?;
        let accept = match Self::handshake(headers) {
            Ok(accept) => accept,
            Err(err) => {
                Self::write_ws_error(&mut stream, 400, "Bad Request", &err.to_string()).await?;
                return Err(err);
            }
        };

        Self::write_ok(&mut stream, accept).await?;

        let path = initial_line
            .split(' ')
            .nth(1)
            .ok_or(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid initial line",
            ))?
            .to_string();

        Ok((path, WebSocket::new(stream, reader)))
    }
}
