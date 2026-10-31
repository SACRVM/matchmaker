//! Husky (github.com/SACRVM/husky, its LEASH.md) may run this program on
//! a server: start it, watch it, swap in each new release. Its pipe
//! speaks JSON lines: hello and welcome, a heartbeat every 5 s, ping and
//! pong, shutdown and its ack. Without `HUSKY_PIPE` the program runs on
//! its own.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

/// Attaches to Husky if it started this program; then what resolves once
/// Husky asks it to stop (the reason: update, manual, launcher-stopping).
pub async fn attach() -> Result<Option<oneshot::Receiver<String>>> {
    let Ok(name) = std::env::var("HUSKY_PIPE") else { return Ok(None) };
    let (read, write) = connect(name.clone()).await.with_context(|| format!("Husky's pipe {name}"))?;
    let mut lines = BufReader::new(read).lines();
    let mut write = write;
    let hello = json!({
        "id": guid(),
        "type": "hello",
        "data": {
            "protocolVersion": 1,
            "appVersion": env!("CARGO_PKG_VERSION"),
            "appName": "matchmaker",
            "pid": std::process::id(),
            "capabilities": [],
        }
    });
    send(&mut write, &hello).await?;
    let welcome: Value = serde_json::from_str(&lines.next_line().await?.context("Husky said nothing")?)?;
    if welcome["type"] != "welcome" {
        bail!("Husky answered {} to hello", welcome["type"]);
    }
    if welcome["data"]["accepted"] == false {
        bail!("Husky would not have it: {}", welcome["data"]["reason"]);
    }
    println!("attached to Husky {}", welcome["data"]["launcherVersion"]);
    let (stop, stopped) = oneshot::channel();
    tokio::spawn(async move {
        let mut stop = Some(stop);
        let mut beat = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = beat.tick() => {
                    if send(&mut write, &json!({ "type": "heartbeat" })).await.is_err() {
                        break;
                    }
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    let id = message["id"].clone();
                    match message["type"].as_str() {
                        Some("ping") => {
                            let _ = send(&mut write, &json!({ "id": guid(), "replyTo": id, "type": "pong", "data": { "status": "healthy", "details": { "games": crate::codes::OPEN.load(std::sync::atomic::Ordering::Relaxed) } } })).await;
                        }
                        Some("shutdown") => {
                            let _ = send(&mut write, &json!({ "id": guid(), "replyTo": id, "type": "shutdown-ack" })).await;
                            let reason = message["data"]["reason"].as_str().unwrap_or("manual").to_string();
                            if let Some(stop) = stop.take() {
                                let _ = stop.send(reason);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // The pipe closed: Husky is gone.
        if let Some(stop) = stop.take() {
            let _ = stop.send("launcher-stopping".into());
        }
    });
    Ok(Some(stopped))
}

async fn send(write: &mut (impl AsyncWrite + Unpin), message: &Value) -> Result<()> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    write.write_all(line.as_bytes()).await?;
    write.flush().await?;
    Ok(())
}

/// A GUID's form, lower case, as Husky's ids are.
fn guid() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the system's random numbers");
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// The pipe Husky made: a named pipe on Windows, the socket .NET makes of
/// one elsewhere (`CoreFxPipe_NAME` in the temporary folder).
#[cfg(windows)]
async fn connect(name: String) -> Result<(impl AsyncRead + Unpin, impl AsyncWrite + Unpin)> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let path = format!(r"\\.\pipe\{name}");
    let pipe = loop {
        match ClientOptions::new().open(&path) {
            Ok(pipe) => break pipe,
            // Busy: Husky has not taken the last connection in yet.
            Err(e) if e.raw_os_error() == Some(231) => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(e) => return Err(e.into()),
        }
    };
    Ok(tokio::io::split(pipe))
}

#[cfg(not(windows))]
async fn connect(name: String) -> Result<(impl AsyncRead + Unpin, impl AsyncWrite + Unpin)> {
    let path = std::env::temp_dir().join(format!("CoreFxPipe_{name}"));
    let socket = tokio::net::UnixStream::connect(&path).await?;
    Ok(socket.into_split())
}
