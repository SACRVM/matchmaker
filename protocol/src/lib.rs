//! What a game and a matchmaker say to each other.
//!
//! A host's game goes by a code of six signs a person can say or type on
//! a pad (DX67A3) while the host stays connected; the code stands for the
//! host's ticket, which the matchmaker keeps and never reads (whatever a
//! game's guests need to reach the host: an iroh address, an IP and port).
//!
//! Over iroh, on a connection of its own (`ALPN`): one JSON value asked on
//! a stream, one answered. A host asks `{"host": TICKET}` and is answered
//! `{"code": "DX67A3"}`; the code lives as long as that connection. A guest
//! asks `{"join": "DX67A3"}` and is answered `{"ticket": TICKET}` or
//! `{"no": WHY}`.

use serde::{Deserialize, Serialize};

/// What the connections to a matchmaker speak (QUIC's ALPN).
pub const ALPN: &[u8] = b"matchmaker/codes/1";

/// The signs of a code: no 0 and O, no 1 and I.
pub const SIGNS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// How many signs a code has (30 bits: a billion codes).
pub const LENGTH: usize = 6;

/// The longest ticket a matchmaker keeps, and the longest ask it reads.
pub const MAX_TICKET: usize = 1024;
pub const MAX_LINE: usize = MAX_TICKET + 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ask {
    Host(String),
    Join(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Code(String),
    Ticket(String),
    No(String),
}

/// A code as someone typed it (small letters, spaces, a dash), as a
/// matchmaker keeps it; None if it is no code.
pub fn tidy(text: &str) -> Option<String> {
    let code: String = text.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
    (code.len() == LENGTH && code.bytes().all(|b| SIGNS.contains(&b))).then_some(code)
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
        let ask = serde_json::to_string(&Ask::Join("DX67A3".into())).unwrap();
        assert_eq!(ask, r#"{"join":"DX67A3"}"#, "the wire as the docs have it");
    }
}
