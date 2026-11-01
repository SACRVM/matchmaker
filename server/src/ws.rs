//! The WebSocket door, for every engine (a browser, Unity, Godot): the
//! same rooms as the iroh door (`door.rs`), one JSON ask a text message
//! and one answer, in turn; a socket's room lives as long as it. At
//! `/matchmaker/1?token=TOKEN` on a local port, behind the reverse proxy
//! beside the relay. A socket silent for a minute (no answer to the pings
//! every 20 s) is closed.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use matchmaker_protocol as protocol;
use tokio::net::{TcpListener, TcpStream};
use tokio_websockets::{CloseCode, Limits, Message, ServerBuilder};

use crate::book::{self, Allowance, Book};

/// The path a socket asks for: the protocol's version in it.
pub const PATH: &str = "/matchmaker/1";

const PING: Duration = Duration::from_secs(20);
const SILENT: Duration = Duration::from_secs(60);

/// Serves the book on `at` to sockets that show `token`; where it
/// listens.
pub async fn serve(at: SocketAddr, token: String, book: Arc<Mutex<Book>>) -> Result<SocketAddr> {
    let listener = TcpListener::bind(at).await.with_context(|| format!("the WebSocket door on {at}"))?;
    let local = listener.local_addr()?;
    println!("the WebSocket door listens on ws://{local}{PATH}");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let (token, book) = (token.clone(), book.clone());
            tokio::spawn(async move {
                let _ = socket(stream, &token, &book).await;
            });
        }
    });
    Ok(local)
}

/// One socket's life.
async fn socket(stream: TcpStream, token: &str, book: &Mutex<Book>) -> Result<()> {
    let limits = Limits::default().max_payload_len(Some(protocol::MAX_ASK));
    let builder = ServerBuilder::new().limits(limits);
    let (request, mut ws) = tokio::time::timeout(Duration::from_secs(10), builder.accept(stream)).await??;
    if request.uri().path() != PATH || shown_token(request.uri().query()) != Some(token) {
        ws.send(Message::close(Some(CloseCode::POLICY_VIOLATION), "wrong path or token")).await?;
        return Ok(());
    }
    let id = book::next_conn();
    let answered = answer(&mut ws, id, book).await;
    book::gone(book, id);
    answered
}

/// Answers a socket's asks until it closes or falls silent.
async fn answer(ws: &mut tokio_websockets::WebSocketStream<TcpStream>, id: u64, book: &Mutex<Book>) -> Result<()> {
    let mut allowance = Allowance::new(Instant::now());
    let mut heard = Instant::now();
    let mut ping = tokio::time::interval(PING);
    ping.tick().await;
    loop {
        tokio::select! {
            message = ws.next() => {
                let Some(message) = message else { return Ok(()) };
                let message = message?;
                heard = Instant::now();
                if message.is_close() {
                    return Ok(());
                }
                let Some(text) = message.as_text() else { continue };
                let answer = book::reply(book, id, text.as_bytes(), &mut allowance, "WebSocket");
                ws.send(Message::text(serde_json::to_string(&answer)?)).await?;
            }
            _ = ping.tick() => {
                if heard.elapsed() >= SILENT {
                    ws.send(Message::close(Some(CloseCode::GOING_AWAY), "silent")).await?;
                    return Ok(());
                }
                ws.send(Message::ping("")).await?;
            }
        }
    }
}

/// The token in a query (`token=...` among others).
fn shown_token(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| pair.strip_prefix("token="))
}

#[cfg(test)]
mod tests {
    use matchmaker_protocol::{Answer, Ask, Join, Refusal, Room, Search};
    use tokio_websockets::ClientBuilder;

    use super::*;

    async fn door() -> (SocketAddr, Arc<Mutex<Book>>) {
        let book = Arc::new(Mutex::new(Book::default()));
        let at = serve(([127, 0, 0, 1], 0).into(), "t0ken".into(), book.clone()).await.unwrap();
        (at, book)
    }

    type Socket = tokio_websockets::WebSocketStream<tokio_websockets::MaybeTlsStream<TcpStream>>;

    async fn connect(at: SocketAddr, path: &str) -> Socket {
        let uri: http::Uri = format!("ws://{at}{path}").parse().unwrap();
        ClientBuilder::from_uri(uri).connect().await.unwrap().0
    }

    async fn ask(ws: &mut Socket, ask: &Ask) -> Answer {
        ws.send(Message::text(serde_json::to_string(ask).unwrap())).await.unwrap();
        loop {
            let message = ws.next().await.unwrap().unwrap();
            if let Some(text) = message.as_text() {
                return serde_json::from_str(text).unwrap();
            }
        }
    }

    /// A host opens a room through the WebSocket door, a guest lists it
    /// and joins it by its code; the room goes when the host's socket
    /// closes.
    #[tokio::test(flavor = "multi_thread")]
    async fn games_meet_through_websockets() {
        let (at, book) = door().await;
        let mut host = connect(at, "/matchmaker/1?token=t0ken").await;
        let mut guest = connect(at, "/matchmaker/1?x=1&token=t0ken").await;
        let room = Room { app: "browser-game".into(), version: "1".into(), title: "Ben".into(), seats: 4, taken: 1, ticket: "offer".into(), ..Room::default() };
        let Answer::Code(code) = ask(&mut host, &Ask::Open(room)).await else { panic!("no code") };
        let search = Search { app: "browser-game".into(), version: "1".into(), ..Search::default() };
        let Answer::Rooms(rooms) = ask(&mut guest, &Ask::List(search)).await else { panic!() };
        assert_eq!(rooms.rooms.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(), [code.as_str()]);
        let join = Ask::Join(Join { code: code.clone(), ..Join::default() });
        assert!(matches!(ask(&mut guest, &join).await, Answer::Found(f) if f.ticket == "offer"));
        guest.send(Message::text("hello?")).await.unwrap();
        assert_eq!(serde_json::from_str::<Answer>(guest.next().await.unwrap().unwrap().as_text().unwrap()).unwrap(), Answer::No(Refusal::NotUnderstood));
        host.close().await.unwrap();
        for _ in 0..50 {
            if book.lock().unwrap().len() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(ask(&mut guest, &join).await, Answer::No(Refusal::UnknownCode), "the room went with its host's socket");
    }

    /// Without the token, or at another path, a socket is closed at once.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_door_wants_the_token() {
        let (at, _) = door().await;
        for path in ["/matchmaker/1?token=wrong", "/matchmaker/1", "/other?token=t0ken"] {
            let mut ws = connect(at, path).await;
            let message = ws.next().await.unwrap().unwrap();
            assert!(message.is_close(), "{path}");
        }
    }
}
