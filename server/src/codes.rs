//! The game codes: a host's ticket under six signs (DX67A3) while its
//! connection lasts, a guest's code answered with the ticket. An iroh
//! endpoint with a key of its own (the games know its public half),
//! reached through the relay of this same program.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl, SecretKey};

use matchmaker_protocol as wire;
use wire::{Answer, Ask};

/// At most this many games open at once, and lookups a connection makes.
const MOST_GAMES: usize = 10_000;
const MOST_LOOKUPS: u32 = 20;

/// How many games are open (for Husky's console, `husky.rs`).
pub static OPEN: AtomicUsize = AtomicUsize::new(0);

/// The open games by code: the ticket and the connection that keeps it.
#[derive(Default)]
struct Book {
    games: HashMap<String, (String, usize)>,
}

/// Serves the codes through the relay at `relay` (this program's own,
/// `token` its key) as the endpoint of `key`; the endpoint, to close.
pub async fn serve(key: SecretKey, relay: RelayUrl, token: String) -> Result<Endpoint> {
    let map = RelayMap::from_iter([relay]).with_auth_token(token);
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(key)
        .relay_mode(RelayMode::Custom(map))
        .alpns(vec![wire::ALPN.to_vec()])
        .bind()
        .await
        .context("the codes' endpoint")?;
    tokio::time::timeout(Duration::from_secs(15), endpoint.online()).await.context("the codes' endpoint reached no relay within 15 s")?;
    println!("the codes are served as {}", endpoint.id());
    let book = Arc::new(Mutex::new(Book::default()));
    let serving = endpoint.clone();
    tokio::spawn(async move {
        let mut next = 0usize;
        while let Some(incoming) = serving.accept().await {
            next += 1;
            let (book, id) = (book.clone(), next);
            tokio::spawn(async move {
                if let Ok(conn) = incoming.await {
                    answer(conn, id, &book).await;
                    let gone = {
                        let mut b = book.lock().expect("the book");
                        let before = b.games.len();
                        b.games.retain(|_, (_, by)| *by != id);
                        OPEN.store(b.games.len(), Ordering::Relaxed);
                        before - b.games.len()
                    };
                    if gone > 0 {
                        println!("a game closed ({} open)", book.lock().expect("the book").games.len());
                    }
                }
            });
        }
    });
    Ok(endpoint)
}

/// Answers a connection's asks, one a stream, until it closes (a host's
/// connection keeps its game's code).
async fn answer(conn: Connection, id: usize, book: &Mutex<Book>) {
    let mut lookups = 0;
    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
        let Ok(bytes) = recv.read_to_end(wire::MAX_LINE).await else { break };
        let answer = match serde_json::from_slice::<Ask>(&bytes) {
            Ok(Ask::Host(ticket)) => host(ticket, id, book),
            Ok(Ask::Join(code)) => {
                lookups += 1;
                if lookups > MOST_LOOKUPS {
                    break;
                }
                let found = wire::tidy(&code).and_then(|c| book.lock().expect("the book").games.get(&c).map(|(t, _)| t.clone()));
                found.map_or_else(|| Answer::No("no game with this code".into()), Answer::Ticket)
            }
            Err(_) => Answer::No("not understood".into()),
        };
        let Ok(line) = serde_json::to_vec(&answer) else { break };
        if send.write_all(&line).await.is_err() || send.finish().is_err() {
            break;
        }
    }
}

/// A host's ticket kept under a new code (one a connection).
fn host(ticket: String, id: usize, book: &Mutex<Book>) -> Answer {
    if ticket.is_empty() || ticket.len() > wire::MAX_TICKET {
        return Answer::No("no ticket".into());
    }
    let mut b = book.lock().expect("the book");
    if let Some((code, _)) = b.games.iter().find(|(_, (_, by))| *by == id) {
        return Answer::Code(code.clone());
    }
    if b.games.len() >= MOST_GAMES {
        return Answer::No("the server is full".into());
    }
    let code = loop {
        let mut bytes = [0u8; wire::LENGTH];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        let code: String = bytes.iter().map(|x| wire::SIGNS[(x % 32) as usize] as char).collect();
        if !b.games.contains_key(&code) {
            break code;
        }
    };
    b.games.insert(code.clone(), (ticket, id));
    OPEN.store(b.games.len(), Ordering::Relaxed);
    println!("a game opened ({} open)", b.games.len());
    Answer::Code(code)
}
