//! What a game and a matchmaker say to each other (protocol 1).
//!
//! A host opens a room: its game (the app and its version), a title, its
//! seats, attributes of the game's own and a ticket, whatever its guests
//! need to reach the host (an iroh address, an IP and port), which the
//! matchmaker keeps and never reads. The room gets a code of six signs a
//! person can say or type on a pad (DX67A3) and lives as long as the
//! host's connection. Guests find it by its code, in the list of open rooms
//! of their app and version, or by asking for any room with a free seat.
//!
//! Over iroh, on a connection of its own (`ALPN`): one JSON value asked on
//! a stream, one answered. Every ask and answer is an object with one key,
//! the ask's or the answer's name:
//!
//! | Ask | Answer |
//! |---|---|
//! | `{"open": Room}` | `{"code": "DX67A3"}` |
//! | `{"update": Update}` | `{"ok": {}}` |
//! | `{"join": Join}` | `{"found": Found}` |
//! | `{"list": Search}` | `{"rooms": Rooms}` |
//! | `{"quick": Search}` | `{"found": Found}` |
//!
//! and any of them may be answered `{"no": "unknown-code"}` (a `Refusal`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What the connections to a matchmaker speak (QUIC's ALPN).
pub const ALPN: &[u8] = b"matchmaker/1";

/// The signs of a code: no 0 and O, no 1 and I.
pub const SIGNS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// How many signs a code has (30 bits: a billion codes).
pub const LENGTH: usize = 6;

/// The longest ask a matchmaker reads, and the longest answer a game
/// reads (a page of the list).
pub const MAX_ASK: usize = 4096;
pub const MAX_ANSWER: usize = 64 * 1024;

/// The most a room may say of itself.
pub const MAX_APP: usize = 64;
pub const MAX_VERSION: usize = 128;
pub const MAX_TITLE: usize = 64;
pub const MAX_TICKET: usize = 1024;
pub const MAX_SEATS: u32 = 64;
pub const MAX_ATTRIBUTES: usize = 16;
pub const MAX_KEY: usize = 32;
pub const MAX_VALUE: usize = 64;

/// Rooms a page of the list holds.
pub const PAGE: usize = 20;

/// An ask.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ask {
    /// A host opens its room (one a connection): answered with its code.
    Open(Room),
    /// The host's room changes (seats taken, attributes): answered `ok`.
    Update(Update),
    /// The ticket of the room with a code.
    Join(Join),
    /// The rooms of an app and version (listed ones), newest first.
    List(Search),
    /// The room of an app and version with a free seat that fits best:
    /// all the attributes asked for, the fullest, the oldest.
    Quick(Search),
}

/// An answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Code(String),
    Ok {},
    Found(Found),
    Rooms(Rooms),
    No(Refusal),
}

/// A room as its host opens it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    /// The game (`voxcade/pulsar`) and its version: only the same meet.
    pub app: String,
    pub version: String,
    pub title: String,
    pub seats: u32,
    #[serde(default)]
    pub taken: u32,
    /// The game's own words about the room (`mode = "coop"`): shown in
    /// the list, and what a search or a quick join asks for.
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    pub ticket: String,
    /// In the list and found by a quick join; else by its code only.
    #[serde(default = "listed")]
    pub listed: bool,
}

fn listed() -> bool {
    true
}

/// Listed, as a room left without `listed` is.
impl Default for Room {
    fn default() -> Self {
        Self { app: String::new(), version: String::new(), title: String::new(), seats: 0, taken: 0, attributes: BTreeMap::new(), ticket: String::new(), listed: listed() }
    }
}

/// What changes of the host's room (only what is given).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Update {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listed: Option<bool>,
}

/// A code typed by a guest, and (if given) the app and version it plays:
/// a room of another is refused as such.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Join {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The rooms asked for: an app and version, the attributes they must
/// have, the page of the list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Search {
    pub app: String,
    pub version: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub page: u32,
}

/// A room found: its code and title, and the ticket to reach its host.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub code: String,
    pub title: String,
    pub ticket: String,
}

/// A page of the list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rooms {
    pub rooms: Vec<Listed>,
    /// More pages follow.
    pub more: bool,
}

/// A room in the list (its ticket only to whoever joins it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub code: String,
    pub title: String,
    pub seats: u32,
    pub taken: u32,
    pub attributes: BTreeMap<String, String>,
    /// Seconds since it opened.
    pub age: u64,
}

/// Why an ask is refused, for the game to say in its own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// No open room has this code (or it was no code).
    UnknownCode,
    /// The room is another game's, or another version's.
    OtherApp,
    OtherVersion,
    /// No room with a free seat fits.
    NoRoomFree,
    /// This connection has a room already, or none to update.
    HasRoom,
    NoRoom,
    /// The room says too much, or something impossible (more taken than
    /// seats, an empty ticket).
    Invalid,
    /// The ask was not understood.
    NotUnderstood,
    /// Too many asks too fast: wait a moment.
    TooFast,
    /// The matchmaker holds as many rooms as it may.
    Full,
}

impl Refusal {
    /// In plain English, for logs and games that say no more.
    pub fn text(self) -> &'static str {
        match self {
            Refusal::UnknownCode => "no game with this code",
            Refusal::OtherApp => "the code is another game's",
            Refusal::OtherVersion => "the code is another version's",
            Refusal::NoRoomFree => "no game with a free seat",
            Refusal::HasRoom => "this connection has a room already",
            Refusal::NoRoom => "this connection has no room",
            Refusal::Invalid => "the room says something impossible",
            Refusal::NotUnderstood => "not understood",
            Refusal::TooFast => "too many asks: wait a moment",
            Refusal::Full => "the matchmaker is full",
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text())
    }
}

/// A code as someone typed it (small letters, spaces, a dash), as a
/// matchmaker keeps it; None if it is no code.
pub fn tidy(text: &str) -> Option<String> {
    let code: String = text.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
    (code.len() == LENGTH && code.bytes().all(|b| SIGNS.contains(&b))).then_some(code)
}

impl Room {
    /// Whether it keeps within the limits and makes sense.
    pub fn is_valid(&self) -> bool {
        !self.app.is_empty()
            && self.app.len() <= MAX_APP
            && self.version.len() <= MAX_VERSION
            && self.title.len() <= MAX_TITLE
            && !self.ticket.is_empty()
            && self.ticket.len() <= MAX_TICKET
            && (1..=MAX_SEATS).contains(&self.seats)
            && self.taken <= self.seats
            && attributes_valid(&self.attributes)
    }
}

/// Whether attributes keep within the limits.
pub fn attributes_valid(attributes: &BTreeMap<String, String>) -> bool {
    attributes.len() <= MAX_ATTRIBUTES && attributes.iter().all(|(k, v)| !k.is_empty() && k.len() <= MAX_KEY && v.len() <= MAX_VALUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A code is taken as people type it (small letters, a space, a
    /// dash), and nothing else is a code: no 0, O, 1 or I, six signs.
    #[test]
    fn a_code_is_taken_as_typed() {
        assert_eq!(tidy("dx6-7a3").as_deref(), Some("DX67A3"));
        assert_eq!(tidy(" DX6 7A3\n").as_deref(), Some("DX67A3"));
        for no in ["DX67A", "DX67A33", "DX67A0", "DO67A3", "DX67I3", "192.168.1.5", "vxc1abcd"] {
            assert_eq!(tidy(no), None, "{no}");
        }
        assert!(SIGNS.len() == 32 && !SIGNS.iter().any(|s| b"01OI".contains(s)));
    }

    /// The wire as the docs have it: one key an ask or answer, the
    /// refusals as words.
    #[test]
    fn the_wire_reads_as_documented() {
        let ask: Ask = serde_json::from_str(r#"{"join": {"code": "dx6-7a3"}}"#).unwrap();
        assert_eq!(ask, Ask::Join(Join { code: "dx6-7a3".into(), ..Join::default() }));
        let open: Ask = serde_json::from_str(r#"{"open": {"app": "a", "version": "1", "title": "T", "seats": 2, "ticket": "t"}}"#).unwrap();
        let Ask::Open(room) = open else { panic!() };
        assert!(room.listed && room.taken == 0 && room.attributes.is_empty() && room.is_valid());
        assert!(Room::default().listed, "listed in Rust as in JSON");
        assert_eq!(serde_json::to_string(&Answer::Ok {}).unwrap(), r#"{"ok":{}}"#);
        assert_eq!(serde_json::to_string(&Answer::No(Refusal::UnknownCode)).unwrap(), r#"{"no":"unknown-code"}"#);
        assert_eq!(serde_json::to_string(&Answer::Code("DX67A3".into())).unwrap(), r#"{"code":"DX67A3"}"#);
        let update = serde_json::to_string(&Ask::Update(Update { taken: Some(2), ..Update::default() })).unwrap();
        assert_eq!(update, r#"{"update":{"taken":2}}"#);
    }

    /// A room keeps within its limits: seats, more taken than there are,
    /// an empty ticket, too many attributes.
    #[test]
    fn a_room_keeps_within_its_limits() {
        let good = Room { app: "voxcade/pulsar".into(), version: "1".into(), title: "Pulsar".into(), seats: 2, taken: 1, ticket: "t".into(), ..Room::default() };
        assert!(good.is_valid());
        assert!(!Room { taken: 3, ..good.clone() }.is_valid());
        assert!(!Room { seats: 0, taken: 0, ..good.clone() }.is_valid());
        assert!(!Room { ticket: String::new(), ..good.clone() }.is_valid());
        assert!(!Room { app: String::new(), ..good.clone() }.is_valid());
        let many = (0..=MAX_ATTRIBUTES).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert!(!Room { attributes: many, ..good }.is_valid());
    }
}
