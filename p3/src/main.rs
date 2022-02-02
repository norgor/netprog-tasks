use std::collections::VecDeque;

use async_std::io::prelude::BufReadExt;
use async_std::io::WriteExt;
use async_std::prelude::*;
use async_std::{io, net::TcpListener, net::TcpStream, task};

struct LineReader {
    buf: [u8; 256],
    queue: VecDeque<u8>,
    stream: TcpStream,
}

impl LineReader {
    pub fn new(stream: TcpStream) -> LineReader {
        LineReader {
            buf: [0; 256],
            queue: VecDeque::new(),
            stream,
        }
    }

    pub async fn read(&mut self) -> io::Result<String> {
        let mut bytes = 1;
        while bytes != 0 {
            bytes = self.stream.read(&mut self.buf).await?;
            let offset = self.queue.len();
            self.queue.extend(self.buf[0..bytes].iter());
            for i in 0..bytes {
                if self.buf[i] == '\n' as u8 {
                    let raw = self
                        .queue
                        .drain(0..offset + i + 1)
                        .take(offset + i)
                        .collect();
                    return match String::from_utf8(raw) {
                        Ok(str) => Ok(str),
                        Err(_) => Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "ERROR: Invalid UTF-8".to_string(),
                        )),
                    };
                }
            }
        }

        Ok("".to_string())
    }
}

fn calculate(operator: char, lhs: f64, rhs: f64) -> f64 {
    match operator {
        '*' => lhs * rhs,
        '/' => lhs / rhs,
        '+' => lhs + rhs,
        '-' => lhs - rhs,
        '^' => lhs.powf(rhs),
        _ => panic!("operator {} not implemented", operator),
    }
}

fn handle_command(line: String) -> String {
    let operators = vec!['*', '/', '+', '-', '^'];
    let pair = operators
        .iter()
        .map(|x| (*x, line.find(*x)))
        .filter(|x| x.1.is_some())
        .map(|x| (x.0, x.1.unwrap()))
        .next();

    match pair {
        Some((operator, index)) => {
            let lhs = line[0..index].parse::<f64>();
            let rhs = line[index + 1..].parse::<f64>();
            let values = lhs.and_then(|x| Ok((x, rhs?)));
            match values {
                Ok(values) => calculate(operator, values.0, values.1).to_string(),
                Err(_) => "ERROR: Value not a number.".to_string(),
            }
        }
        None => match line.parse::<f64>() {
            Ok(value) => value.to_string(),
            Err(_) => "ERROR: Value not a number.".to_string(),
        },
    }
}

async fn handle_math_client(mut stream: TcpStream) -> io::Result<()> {
    let mut reader = io::BufReader::new(stream.clone());

    let mut line = String::new();
    while reader.read_line(&mut line).await? != 0 {
        let cmd = line.trim().to_string();
        println!("<< '{}'", cmd);
        let mut resp = handle_command(cmd);
        println!(">> '{}'", resp);
        resp += "\n";
        stream.write(resp.as_bytes()).await?;
        line = String::new();
    }

    Ok(())
}

async fn run_math_server() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:43000").await?;
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        task::spawn(handle_math_client(stream));
    }

    Ok(())
}
async fn handle_http_client(mut stream: TcpStream) -> io::Result<()> {
    let mut reader = io::BufReader::new(stream.clone());
    let mut initial = String::new();
    if reader.read_line(&mut initial).await? == 0 {
        return Ok(());
    }

    stream
        .write(
            b"\
        HTTP/1.0 200 OK\n\
        Content-Type: text/html; charset=utf-8\n",
        )
        .await?;

    let mut body = String::new();
    body += "\
    <html>\n\
        <body>\n\
            <h1>Welcome to the ultra-simple web server!</h1>\n\
            <ul>\n";
    let mut line = String::new();
    while reader.read_line(&mut line).await? != 0 {
        let trimmed = line.trim();
        if trimmed.len() == 0 {
            break;
        }
        body += format!("<li>{}</li>\n", line).as_str();
        line = String::new();
    }
    body += "\
            </ul>\n\
        </body>\n\
    </html>\n\
    \n\
    \n";
    let bodyBytes = body.as_bytes();
    stream
        .write(format!("Content-Length: {}\n\n", bodyBytes.len()).as_bytes())
        .await?;
    stream.write(bodyBytes).await?;
    return Ok(());
}

async fn run_http_server() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        task::spawn(handle_http_client(stream));
    }

    Ok(())
}

fn main() {
    let mut tasks = vec![
        task::spawn(run_math_server()),
        task::spawn(run_http_server()),
    ];
    while tasks.len() > 0 {
        task::block_on(tasks.pop().unwrap()).unwrap();
    }
}
