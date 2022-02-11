use std::env;
use std::str;

use async_std::{io, net::UdpSocket, task};

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

fn handle_command(line: &str) -> String {
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

async fn run_math_server_async() -> io::Result<()> {
    let listener = UdpSocket::bind(MATH_LISTENER_ADDRESS).await?;
    println!("Math listener running on {}", MATH_LISTENER_ADDRESS);

    let mut buf = [0u8; 1024];
    while let Ok((bytes, addr)) = listener.recv_from(&mut buf).await {
        let slice = &buf[..bytes];
        let cmd = str::from_utf8(slice);
        println!("<< '{}'", cmd.expect("INVALID UTF-8"));
        let resp = str::from_utf8(slice)
            .map(|x| handle_command(x))
            .expect("ERROR: Value not valid UTF-8");

        println!(">> '{}'", resp);

        listener.send_to(resp.as_bytes(), addr).await?;
    }

    Ok(())
}

fn run_math_server() {
    task::block_on(run_math_server_async()).unwrap();
}

async fn run_math_client_async(address: String) -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(address).await?;

    let mut buf = [0u8; 1024];
    let mut line = String::new();
    loop {
        io::stdin().read_line(&mut line).await?;
        let cmd = &line[..line.len() - 1];
        socket.send(cmd.as_bytes()).await?;
        let bytes = socket.recv(&mut buf).await?;
        let resp = str::from_utf8(&buf[..bytes]).unwrap();
        println!("{}", resp);
        line.clear();
    }
}

fn run_math_client(address: String) {
    task::block_on(run_math_client_async(address)).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "-s" => run_math_server(),
        "-c" => run_math_client(args[2].clone()),
        _ => panic!("You must specify a client/server argument (-c or -s)"),
    };
}
