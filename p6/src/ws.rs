use core::panic;
use std::{
    borrow::BorrowMut,
    cell::{Cell, RefCell, RefMut},
    io::{Error, ErrorKind},
};

use async_std::{
    io::{self, BufReader, ReadExt, WriteExt},
    net::TcpStream,
    sync::Mutex,
};

struct Wrap<'a, T>(RefMut<'a, T>);
unsafe impl<'a, T> Send for Wrap<'a, T> {}

struct WebSocketRX {
    reader: RefCell<BufReader<TcpStream>>,
    buf: RefCell<(Vec<u8>, usize)>,
}

struct WebSocketTX {
    stream: TcpStream,
    write_buf: Vec<u8>,
}
pub struct WebSocket {
    rx: Mutex<WebSocketRX>,
    tx: Mutex<WebSocketTX>,
}

#[derive(PartialEq, Eq)]
enum Opcode {
    Continuation = 0,
    Text = 1,
    Binary = 2,
    Close = 8,
    Ping = 9,
    Pong = 10,
}

impl Opcode {
    fn as_num(&self) -> u8 {
        match self {
            Opcode::Continuation => 0,
            Opcode::Text => 1,
            Opcode::Binary => 2,
            Opcode::Close => 8,
            Opcode::Ping => 9,
            Opcode::Pong => 10,
        }
    }

    fn from_num(v: u8) -> io::Result<Opcode> {
        match v {
            0 => Ok(Opcode::Continuation),
            1 => Ok(Opcode::Text),
            2 => Ok(Opcode::Binary),
            8 => Ok(Opcode::Close),
            9 => Ok(Opcode::Ping),
            10 => Ok(Opcode::Pong),
            _ => Err(Error::new(ErrorKind::InvalidData, "invalid opcode")),
        }
    }
}

enum FrameReadState {
    BitsOpcodePayloadLength,
    ExtendedPayloadLength16,
    ExtendedPayloadLength64,
    MaskingKey,
    Payload,
    Done,
}

struct Frame {
    state: FrameReadState,

    fin: bool,
    rsv: u8,
    opcode: Opcode,
    mask: bool,
    payload_len: u64,
    masking_key: [u8; 4],
    payload: Vec<u8>,
}

pub enum MessageType {
    Text,
    Binary,
    Ping,
    Pong,
}

pub struct Message {
    pub data: Vec<u8>,
    pub r#type: MessageType,
}

impl Frame {
    fn new() -> Frame {
        Frame {
            state: FrameReadState::BitsOpcodePayloadLength,
            fin: false,
            rsv: 0,
            opcode: Opcode::Continuation,
            mask: false,
            payload_len: 0,
            masking_key: [0; 4],
            payload: vec![],
        }
    }

    fn want_bytes(&self) -> usize {
        match &self.state {
            FrameReadState::BitsOpcodePayloadLength => 2,
            FrameReadState::ExtendedPayloadLength16 => 2,
            FrameReadState::ExtendedPayloadLength64 => 4,
            FrameReadState::MaskingKey => 4,
            FrameReadState::Payload => self.payload_len as usize,
            FrameReadState::Done => 0,
        }
    }

    fn read(&mut self, chunk: &[u8]) -> io::Result<(usize, usize)> {
        let mut consumed = 0;
        let mut chunk = chunk;
        loop {
            let want_bytes = self.want_bytes();
            if chunk.len() < want_bytes {
                return Ok((consumed, want_bytes));
            }

            let new_state = match self.state {
                FrameReadState::BitsOpcodePayloadLength => {
                    self.fin = chunk[0] & 0b10000000 != 0;
                    self.rsv = (chunk[0] & 0b01110000) >> 4;
                    self.opcode = Opcode::from_num(chunk[0] & 0b00001111)?;
                    self.mask = chunk[1] & 0b10000000 != 0;
                    self.payload_len = (chunk[1] & 0b01111111) as u64;

                    if self.rsv != 0 {
                        return Err(Error::new(ErrorKind::InvalidData, "used reserved bits"));
                    } else if self.mask != true {
                        return Err(Error::new(ErrorKind::InvalidData, "must use mask"));
                    }

                    match self.payload_len {
                        0..=125 => FrameReadState::MaskingKey,
                        126 => FrameReadState::ExtendedPayloadLength16,
                        127 => FrameReadState::ExtendedPayloadLength64,
                        _ => panic!("payload length was not an u8"),
                    }
                }
                FrameReadState::ExtendedPayloadLength16 => {
                    self.payload_len = (chunk[0] as u64) << 8 | chunk[1] as u64;
                    FrameReadState::MaskingKey
                }
                FrameReadState::ExtendedPayloadLength64 => {
                    self.payload_len = 0;
                    for n in 0usize..8usize {
                        self.payload_len |= (chunk[7 - n] as u64) << n * 8;
                    }
                    FrameReadState::MaskingKey
                }
                FrameReadState::MaskingKey => {
                    for (src, dst) in chunk.iter().zip(self.masking_key.iter_mut()) {
                        *dst = *src;
                    }
                    FrameReadState::Payload
                }
                FrameReadState::Payload => {
                    let range = 0..self.payload_len as usize;
                    self.payload = chunk[range.clone()]
                        .iter()
                        .zip(range)
                        .map(|(v, i)| v ^ self.masking_key[i % 4])
                        .collect();
                    FrameReadState::Done
                }
                FrameReadState::Done => return Ok((consumed, 0)),
            };
            self.state = new_state;
            consumed += want_bytes;
            chunk = &chunk[want_bytes..];
        }
    }

    fn payload_size(len: u64) -> usize {
        match len {
            0..=125 => 0usize,
            126..=0xFFFF => 2usize,
            0x10000.. => 8usize,
        }
    }

    fn write_len(&self) -> usize {
        let first_2_bytes = 2;
        let payload_size = Self::payload_size(self.payload_len);
        let masking_key_size = if self.mask { 4 } else { 0 };
        first_2_bytes + payload_size + masking_key_size + self.payload_len as usize
    }

    fn write(&self, buf: &mut [u8]) {
        if buf.len() < self.write_len() {
            panic!("buffer too small")
        }
        let mut i = 0;
        let mut w = |v| {
            buf[i] = v;
            i += 1;
        };

        let b = |x| if x { 1u8 } else { 0u8 };
        w(b(self.fin) << 7 | self.rsv << 4 | self.opcode.as_num());
        let mask_bit = b(self.mask) << 7;

        match Self::payload_size(self.payload_len) {
            0 => {
                w(mask_bit | self.payload_len as u8);
            }
            2 => {
                w(mask_bit | 126u8);
                w(((self.payload_len & (0xFF << 8)) >> 8) as u8);
                w((self.payload_len & 0xFF) as u8);
            }
            8 => {
                w(mask_bit | 127u8);
                for i in 0usize..8usize {
                    let n = 7 - i;
                    w(((self.payload_len & (0xFF << n * 8)) >> n * 8) as u8);
                }
            }
            _ => panic!("unknown payload size"),
        }

        if self.mask {
            for v in self.masking_key.iter() {
                w(*v);
            }
        }

        for v in self.payload.iter() {
            w(*v);
        }
    }
}

impl WebSocket {
    pub(crate) fn new(stream: TcpStream, reader: BufReader<TcpStream>) -> WebSocket {
        WebSocket {
            tx: Mutex::new(WebSocketTX {
                stream,
                write_buf: Vec::new(),
            }),
            rx: Mutex::new(WebSocketRX {
                reader: RefCell::new(reader),
                buf: RefCell::new((vec![0; 1024], 0)),
            }),
        }
    }

    pub async fn send(&self, msg: &Message) -> io::Result<()> {
        let mut frame = Frame::new();
        frame.fin = true;
        frame.opcode = match msg.r#type {
            MessageType::Binary => Opcode::Binary,
            MessageType::Text => Opcode::Text,
            MessageType::Ping => Opcode::Ping,
            MessageType::Pong => Opcode::Pong,
        };
        frame.payload_len = msg.data.len() as u64;
        frame.payload = msg.data.clone();

        let mut tx = self.tx.lock().await;
        let mut stream = tx.stream.clone();
        let buf = &mut tx.write_buf;

        buf.resize(frame.write_len(), 0);
        frame.write(buf);
        stream.write_all(buf).await?;

        Ok(())
    }

    pub async fn recv(&self) -> io::Result<Option<Message>> {
        let mut buf: Vec<u8> = Vec::new();
        let mut fin = false;
        let mut opcode: Option<Opcode> = None;

        while !fin {
            let frame = self.read_frame().await?;
            buf.extend(frame.payload);

            if frame.opcode != Opcode::Continuation {
                let equal_opcode = match opcode {
                    Some(opcode) => opcode == frame.opcode,
                    None => true,
                };
                if !equal_opcode {
                    return Err(Error::new(ErrorKind::InvalidData, "inconsistent opcode"));
                }

                opcode = Some(frame.opcode);
            }
            fin = frame.fin;
        }

        Ok(
            match opcode.ok_or(Error::new(ErrorKind::InvalidData, "no opcode specified"))? {
                Opcode::Binary => Some(Message {
                    data: buf,
                    r#type: MessageType::Binary,
                }),
                Opcode::Ping => Some(Message {
                    data: buf,
                    r#type: MessageType::Ping,
                }),
                Opcode::Pong => Some(Message {
                    data: buf,
                    r#type: MessageType::Pong,
                }),
                Opcode::Text => Some(Message {
                    data: buf,
                    r#type: MessageType::Text,
                }),
                Opcode::Close => None,
                _ => panic!("reached invalid opcode"),
            },
        )
    }

    async fn read_frame(&self) -> io::Result<Frame> {
        let rx = self.rx.lock().await;
        let (buf, read_bytes) = &mut *rx.buf.borrow_mut();

        let mut frame = Frame::new();
        let mut want_bytes = frame.want_bytes();

        loop {
            while *read_bytes < want_bytes {
                let bytes_read =
                    async_std::task::block_on(rx.reader.borrow_mut().read(buf)).unwrap();
                if bytes_read == 0 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected eof"));
                }
                *read_bytes += bytes_read;
            }

            let (consumed, want) = frame.read(&buf[0..*read_bytes])?;
            want_bytes = want;
            *read_bytes -= consumed;

            if buf.capacity() < want {
                buf.resize(want, 0);
            }
            if want == 0 {
                return Ok(frame);
            }
        }
    }
}
