use std::env;

use async_std::io::prelude::BufReadExt;
use async_std::io::{BufReader, WriteExt};
use async_std::prelude::*;
use async_std::{io, net::TcpListener, net::TcpStream, task};

const HTTP_LISTENER_ADDRESS: &str = "0.0.0.0:8080";
const MATH_LISTENER_ADDRESS: &str = "0.0.0.0:43000";

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
    let listener = TcpListener::bind(MATH_LISTENER_ADDRESS).await?;
    println!("Math listener running on {}", MATH_LISTENER_ADDRESS);
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
    let body_bytes = body.as_bytes();
    stream
        .write(format!("Content-Length: {}\n\n", body_bytes.len()).as_bytes())
        .await?;
    stream.write(body_bytes).await?;
    return Ok(());
}

async fn run_http_server() -> io::Result<()> {
    let listener = TcpListener::bind(HTTP_LISTENER_ADDRESS).await?;
    println!("HTTP Listener running on {}", HTTP_LISTENER_ADDRESS);
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        task::spawn(handle_http_client(stream));
    }

    Ok(())
}

fn run_servers() {
    let mut tasks = vec![
        task::spawn(run_math_server()),
        task::spawn(run_http_server()),
    ];
    while tasks.len() > 0 {
        task::block_on(tasks.pop().unwrap()).unwrap();
    }
}

async fn run_math_client_async(address: String) -> io::Result<()> {
    let mut stream = TcpStream::connect(address).await?;
    let mut reader = BufReader::new(stream.clone());
    let mut line = String::new();

    loop {
        io::stdin().read_line(&mut line).await?;
        stream.write(line.as_bytes()).await?;
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        print!("{}", line);
        line.clear();
    }

    Ok(())
}

fn run_math_client(address: String) {
    task::block_on(run_math_client_async(address)).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "-s" => run_servers(),
        "-c" => run_math_client(args[2].clone()),
        _ => panic!("You must specify a client/server argument (-c or -s)"),
    };
}
