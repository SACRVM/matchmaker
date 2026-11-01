//! matchmaker: game codes and a relay in one program, for the machines of
//! a peer-to-peer game in different homes. The relay (iroh's) listens on a
//! local port with plain HTTP; a reverse proxy in front of it (Caddy, say)
//! takes the public name on 443 and holds its certificate. The game codes
//! are an iroh endpoint reached through that relay, so nothing else needs
//! a port. Husky may run it (`husky.rs`); `README.md` says how it is set up.
//!
//! Its config is `data/matchmaker.toml` beside the program (or
//! `--config FILE`), made on the first start or by `--new`, with a new
//! token and key, on the server itself: the secret key never leaves it,
//! the game gets the token and the key's public half.

mod book;
mod door;
mod husky;
mod ws;

use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use iroh::{RelayUrl, SecretKey};
use iroh_relay::server::{self as relay, Access, AccessControl, ClientRateLimit, ClientRequest};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    /// Where the relay listens, behind the reverse proxy.
    #[serde(default = "default_relay")]
    relay: SocketAddr,
    /// Where the WebSocket door listens, behind the reverse proxy too.
    #[serde(default = "default_websocket")]
    websocket: SocketAddr,
    /// The token every game shows the relay.
    token: String,
    /// The codes' secret key; the games know its public half.
    key: String,
}

/// Writes a new config at `path` (never over one: the games know its
/// token and key), and says what the game takes from it.
fn make(path: &std::path::Path) -> Result<()> {
    anyhow::ensure!(!path.exists(), "{} is there already: the games know its token and key", path.display());
    let mut token = [0u8; 24];
    getrandom::fill(&mut token).expect("the system's random numbers");
    let token: String = token.iter().map(|b| format!("{b:02x}")).collect();
    let key = SecretKey::generate();
    let secret: String = key.to_bytes().iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (relay, websocket) = (default_relay(), default_websocket());
    std::fs::write(path, format!("# matchmaker's config: keep it on the server, never in a repository.\nrelay = \"{relay}\"\nwebsocket = \"{websocket}\"\ntoken = \"{token}\"\nkey = \"{secret}\"\n"))?;
    println!("wrote {}", path.display());
    println!("for the games (the relay's token, the codes' public key):");
    println!("  token: {token}");
    println!("  codes: {}", key.public());
    Ok(())
}

fn default_websocket() -> SocketAddr {
    ([127, 0, 0, 1], 7341).into()
}

fn default_relay() -> SocketAddr {
    ([127, 0, 0, 1], 7340).into()
}

/// Only those who show the token are relayed.
#[derive(Debug)]
pub(crate) struct Token(pub(crate) String);

impl AccessControl for Token {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        match request.auth_token() {
            Some(token) if token == self.0 => Access::Allow,
            _ => Access::Deny { reason: Some("wrong token".into()) },
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (mut path, mut new) = (None, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--new" => new = true,
            "--config" => path = Some(PathBuf::from(args.next().context("--config FILE")?)),
            _ => bail!("matchmaker [--config FILE] [--new]"),
        }
    }
    let beside = path.is_none();
    let path = match path {
        Some(path) => path,
        None => std::env::current_exe()?.parent().context("the program's folder")?.join("data").join("matchmaker.toml"),
    };
    if new {
        return make(&path);
    }
    // The first start under Husky makes its own (`data/` outlives updates).
    if beside && !path.exists() {
        println!("first start: no config yet");
        make(&path)?;
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {} (matchmaker --new makes its token and key)", path.display()))?;
    let cfg: Config = toml::from_str(&text).with_context(|| format!("{} is no config", path.display()))?;
    let key: SecretKey = cfg.key.parse().context("key: a secret key in hex")?;
    println!("matchmaker {}", env!("CARGO_PKG_VERSION"));

    let stop = husky::attach().await?;

    // The relay: plain HTTP on a local port, the reverse proxy in front.
    let mut relaying = relay::RelayConfig::new(cfg.relay);
    relaying.access = Arc::new(Token(cfg.token.clone()));
    let mut limits = relay::Limits::default();
    limits.accept_conn_limit = Some(20.0);
    limits.accept_conn_burst = Some(100);
    // Some 10 KB/s per machine carries a whole lockstep game; files come
    // in bursts. Generous, but no one fills the line.
    let mut rate = ClientRateLimit::new(NonZeroU32::new(256 * 1024).expect("not 0"));
    rate.max_burst_bytes = NonZeroU32::new(4 * 1024 * 1024);
    limits.client_rx = Some(rate);
    relaying.limits = limits;
    let mut serving = relay::ServerConfig::default();
    serving.relay = Some(relaying);
    let mut server = relay::Server::spawn(serving).await.context("the relay")?;
    let local: RelayUrl = format!("http://{}", server.http_addr().context("the relay's address")?).parse()?;
    println!("the relay listens on {local}");

    // The rooms, reached through the relay and by WebSocket.
    let book = std::sync::Arc::new(std::sync::Mutex::new(book::Book::default()));
    ws::serve(cfg.websocket, cfg.token.clone(), book.clone()).await?;
    let codes = door::serve(key, local, cfg.token.clone(), book).await?;

    let why = tokio::select! {
        _ = tokio::signal::ctrl_c() => "ctrl-c".to_string(),
        reason = async { match stop { Some(stop) => stop.await.unwrap_or_else(|_| "husky gone".into()), None => std::future::pending().await } } => reason,
        _ = server.join() => "the relay ended".to_string(),
    };
    println!("stopping: {why}");
    codes.close().await;
    server.shutdown().await.ok();
    Ok(())
}
