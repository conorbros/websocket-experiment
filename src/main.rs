use futures_util::{SinkExt, StreamExt};
use std::{env, io::Error as IoError, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::unbounded_channel;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::codec::{Decoder, Encoder};

struct TestCodec {}

impl Decoder for TestCodec {
    type Item = String;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let len = src.len();
        if len == 0 {
            return Ok(None);
        }

        let data = src.split_to(src.len());
        return Ok(Some(String::from_utf8_lossy(&data).to_string()));
    }
}

impl Encoder<String> for TestCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: String, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(item.as_bytes());
        Ok(())
    }
}

async fn handle_connection(raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (mut write, mut read) = ws_stream.split();

    let (write_tx, mut write_rx) = unbounded_channel::<String>();

    // read task
    tokio::spawn(async move {
        let mut decoder = TestCodec {};

        while let Some(msg) = read.next().await {
            let msg = msg.unwrap();
            let message = decoder
                .decode(&mut bytes::BytesMut::from(msg.into_data().as_slice()))
                .unwrap()
                .unwrap();
            write_tx.send(message).unwrap();
        }
    });

    // write task
    tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            let mut encoder = TestCodec {};
            let mut bytes = bytes::BytesMut::new();
            encoder.encode(msg, &mut bytes).unwrap();
            let message = Message::binary(bytes);
            write.send(message).await.unwrap();
        }
    });
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(stream, addr));
    }

    Ok(())
}
