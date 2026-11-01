//! The iroh door: an endpoint with a key of its own (the games know its
//! public half), reached through the relay of this same program. One
//! ask a stream, one answer; a connection's room lives as long as it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl, SecretKey};
use matchmaker_protocol::{self as protocol, Answer, Ask, Refusal};

use crate::book::{Allowance, Book};

/// Serves the book through the relay at `relay` (this program's own,
/// `token` its key) as the endpoint of `key`; the endpoint, to close.
pub async fn serve(key: SecretKey, relay: RelayUrl, token: String, book: Arc<Mutex<Book>>) -> Result<Endpoint> {
    let map = RelayMap::from_iter([relay]).with_auth_token(token);
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(key)
        .relay_mode(RelayMode::Custom(map))
        .alpns(vec![protocol::ALPN.to_vec()])
        .bind()
        .await
        .context("the matchmaker's endpoint")?;
    tokio::time::timeout(Duration::from_secs(15), endpoint.online()).await.context("the matchmaker's endpoint reached no relay within 15 s")?;
    println!("the matchmaker is reached as {}", endpoint.id());
    let serving = endpoint.clone();
    tokio::spawn(async move {
        let mut next = 0u64;
        while let Some(incoming) = serving.accept().await {
            next += 1;
            let (book, conn_id) = (book.clone(), next);
            tokio::spawn(async move {
                if let Ok(conn) = incoming.await {
                    answer(conn, conn_id, &book).await;
                    let (had, open) = {
                        let mut b = book.lock().expect("the book");
                        (b.gone(conn_id), b.len())
                    };
                    if had {
                        println!("a room closed ({open} open)");
                    }
                }
            });
        }
    });
    Ok(endpoint)
}

/// Answers a connection's asks, one a stream, until it closes.
async fn answer(conn: Connection, conn_id: u64, book: &Mutex<Book>) {
    let mut allowance = Allowance::new(Instant::now());
    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
        let Ok(bytes) = recv.read_to_end(protocol::MAX_ASK).await else { break };
        let now = Instant::now();
        let answer = if !allowance.take(now) {
            Answer::No(Refusal::TooFast)
        } else {
            match serde_json::from_slice::<Ask>(&bytes) {
                Ok(ask) => {
                    let opens = matches!(ask, Ask::Open(_));
                    let mut b = book.lock().expect("the book");
                    let answer = b.answer(conn_id, ask, now, &mut random_code);
                    if opens && matches!(answer, Answer::Code(_)) {
                        println!("a room opened ({} open)", b.len());
                    }
                    answer
                }
                Err(_) => Answer::No(Refusal::NotUnderstood),
            }
        };
        let Ok(bytes) = serde_json::to_vec(&answer) else { break };
        if send.write_all(&bytes).await.is_err() || send.finish().is_err() {
            break;
        }
    }
}

/// A new code, at random.
pub fn random_code() -> String {
    let mut bytes = [0u8; protocol::LENGTH];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    bytes.iter().map(|x| protocol::SIGNS[(x % 32) as usize] as char).collect()
}

#[cfg(test)]
mod tests {
    use iroh::{EndpointAddr, TransportAddr};
    use iroh_relay::server as relay;
    use matchmaker_protocol::{Join, Room, Search, Update};

    use super::*;

    /// A relay with a token on a free local port, the door behind it, and
    /// what a game needs to reach it.
    async fn matchmaker() -> (relay::Server, Endpoint, RelayUrl, EndpointAddr) {
        let mut relaying = relay::RelayConfig::new(std::net::SocketAddr::from(([127, 0, 0, 1], 0)));
        relaying.access = Arc::new(crate::Token("t0ken".into()));
        let mut serving = relay::ServerConfig::default();
        serving.relay = Some(relaying);
        let server = relay::Server::spawn(serving).await.unwrap();
        let url: RelayUrl = format!("http://{}", server.http_addr().unwrap()).parse().unwrap();
        let key = SecretKey::generate();
        let at = EndpointAddr::from_parts(key.public(), [TransportAddr::Relay(url.clone())]);
        let door = serve(key, url.clone(), "t0ken".into(), Arc::new(Mutex::new(Book::default()))).await.unwrap();
        (server, door, url, at)
    }

    /// A game's endpoint through the relay, connected to the matchmaker.
    async fn game(url: &RelayUrl, at: &EndpointAddr) -> (Endpoint, Connection) {
        let map = RelayMap::from_iter([url.clone()]).with_auth_token("t0ken".to_string());
        let endpoint = Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Custom(map)).bind().await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), endpoint.online()).await.unwrap();
        let conn = endpoint.connect(at.clone(), protocol::ALPN).await.unwrap();
        (endpoint, conn)
    }

    async fn ask(conn: &Connection, ask: &Ask) -> Answer {
        let (mut send, mut recv) = conn.open_bi().await.unwrap();
        send.write_all(&serde_json::to_vec(ask).unwrap()).await.unwrap();
        send.finish().unwrap();
        serde_json::from_slice(&recv.read_to_end(protocol::MAX_ANSWER).await.unwrap()).unwrap()
    }

    /// Through the relay, as games reach it: a host opens a room, a guest
    /// finds it in the list, joins it by its code and by a quick join; the
    /// host's update shows; the room goes when the host's connection does.
    #[tokio::test(flavor = "multi_thread")]
    async fn games_meet_through_the_door() {
        let (_server, door, url, at) = matchmaker().await;
        let (_h, host) = game(&url, &at).await;
        let (_g, guest) = game(&url, &at).await;
        let room = Room { app: "pulsar".into(), version: "7".into(), title: "Anna".into(), seats: 2, taken: 1, ticket: "the host's ticket".into(), ..Room::default() };
        let Answer::Code(code) = ask(&host, &Ask::Open(room)).await else { panic!("no code") };
        let search = Search { app: "pulsar".into(), version: "7".into(), ..Search::default() };
        let Answer::Rooms(rooms) = ask(&guest, &Ask::List(search.clone())).await else { panic!() };
        assert_eq!(rooms.rooms.iter().map(|r| (r.code.as_str(), r.title.as_str(), r.taken, r.seats)).collect::<Vec<_>>(), [(code.as_str(), "Anna", 1, 2)]);
        let join = Ask::Join(Join { code: code.to_lowercase(), app: Some("pulsar".into()), version: Some("7".into()) });
        let Answer::Found(found) = ask(&guest, &join).await else { panic!() };
        assert_eq!((found.code.as_str(), found.ticket.as_str()), (code.as_str(), "the host's ticket"));
        assert!(matches!(ask(&guest, &Ask::Quick(search.clone())).await, Answer::Found(f) if f.code == code));
        assert_eq!(ask(&host, &Ask::Update(Update { taken: Some(2), ..Update::default() })).await, Answer::Ok {});
        assert_eq!(ask(&guest, &Ask::Quick(search.clone())).await, Answer::No(Refusal::NoRoomFree), "full now");
        let not_json = {
            let (mut send, mut recv) = guest.open_bi().await.unwrap();
            send.write_all(b"hello?").await.unwrap();
            send.finish().unwrap();
            serde_json::from_slice::<Answer>(&recv.read_to_end(protocol::MAX_ANSWER).await.unwrap()).unwrap()
        };
        assert_eq!(not_json, Answer::No(Refusal::NotUnderstood));
        host.close(0u32.into(), b"bye");
        let mut gone = false;
        for _ in 0..50 {
            if ask(&guest, &join).await == Answer::No(Refusal::UnknownCode) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the room went with its host");
        door.close().await;
    }

    /// The relay lets only games with the token in.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_relay_wants_the_token() {
        let (_server, door, url, _) = matchmaker().await;
        let map = RelayMap::from_iter([url]).with_auth_token("wrong".to_string());
        let endpoint = Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Custom(map)).bind().await.unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(5), endpoint.online()).await.is_err(), "never online with a wrong token");
        door.close().await;
    }
}
