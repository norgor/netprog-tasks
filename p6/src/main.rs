use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;

use async_std::channel::{unbounded, Receiver, Sender};
use async_std::sync::Mutex;
use async_std::task::block_on;
use async_std::{channel, io};
use p6::ws::{Message, MessageType, WebSocket};
use p6::wslistener::WSListener;

struct WebSockets {
    map: HashMap<usize, Arc<WebSocket>>,
    next_id: usize,
}
struct Room {
    web_sockets: Arc<Mutex<WebSockets>>,
    tx: Sender<Message>,
}
impl Room {
    fn new() -> Room {
        let (tx, rx) = unbounded();
        let room = Room {
            web_sockets: Arc::new(Mutex::new(WebSockets {
                map: HashMap::new(),
                next_id: 0,
            })),
            tx,
        };
        async_std::task::spawn(Self::run(room.web_sockets.clone(), rx));
        room
    }

    async fn run(web_sockets: Arc<Mutex<WebSockets>>, rx: Receiver<Message>) {
        while let Ok(msg) = rx.recv().await {
            let sockets: Vec<(usize, Arc<WebSocket>)> = {
                web_sockets
                    .lock()
                    .await
                    .map
                    .iter()
                    .map(|(id, ws)| (*id, ws.clone()))
                    .collect()
            };
            for (id, socket) in sockets {
                match socket.send(&msg).await {
                    Ok(_) => {}
                    Err(error) => {
                        println!("failed to send to socket {}: {}", id, error);
                    }
                }
            }
        }
    }

    async fn handle_stream_result(&self, path: String, ws: Arc<WebSocket>) -> io::Result<()> {
        loop {
            let msg = ws.recv().await?;
            match msg {
                Some(msg) => match msg.r#type {
                    MessageType::Binary => {
                        println!("<< {:?}", msg.data);
                        self.tx.send(msg).await.unwrap();
                    }
                    MessageType::Text => {
                        println!("<< {:?}", String::from_utf8_lossy(&msg.data));
                        self.tx.send(msg).await.unwrap();
                    }
                    MessageType::Ping => {
                        println!("<< PING")
                    }
                    MessageType::Pong => {
                        println!("<< PONG")
                    }
                },
                None => break,
            }
        }
        Ok(())
    }

    async fn handle_stream(&self, path: String, ws: WebSocket) {
        let ws = Arc::new(ws);
        let id = {
            let mut guard = self.web_sockets.lock().await;
            let id = guard.next_id;
            guard.map.insert(id, ws.clone());
            guard.next_id += 1;
            id
        };

        match self.handle_stream_result(path, ws).await {
            Ok(_) => {}
            Err(error) => {
                println!("WebSocket error: {}", error);
            }
        }

        let mut guard = self.web_sockets.lock().await;
        guard.map.remove(&id).unwrap();
    }
}

async fn async_main() {
    let listener = WSListener::bind("0.0.0.0:8080").await.unwrap();
    let room = Arc::new(Box::new(Room::new()));
    loop {
        match listener.accept().await {
            Ok((path, ws)) => {
                let room: &'static Arc<Box<Room>> = unsafe { std::mem::transmute(&room.clone()) };
                async_std::task::spawn(room.handle_stream(path, ws));
            }
            Err(error) => print!("Unable to accept client: {}", error),
        }
    }
}

fn main() {
    block_on(async_main());
}
