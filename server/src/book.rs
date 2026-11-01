//! The open rooms, apart from how hosts and guests reach the matchmaker
//! (its doors: iroh's in `door.rs`): what each ask does to them. A room
//! belongs to the connection that opened it and goes with it.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use matchmaker_protocol::{self as protocol, Answer, Ask, Found, Join, Listed, Refusal, Room, Rooms, Search, Update};

/// At most this many rooms open at once.
pub const MOST_ROOMS: usize = 10_000;

/// How many rooms are open (for Husky's console, `husky.rs`).
pub static OPEN: AtomicUsize = AtomicUsize::new(0);

/// A connection's asks: a burst of `BURST`, then `RATE` a second.
const BURST: f64 = 20.0;
const RATE: f64 = 2.0;

pub struct Allowance {
    left: f64,
    at: Instant,
}

impl Allowance {
    pub fn new(now: Instant) -> Self {
        Self { left: BURST, at: now }
    }

    /// Takes one ask; false if there is none left now.
    pub fn take(&mut self, now: Instant) -> bool {
        self.left = (self.left + now.saturating_duration_since(self.at).as_secs_f64() * RATE).min(BURST);
        self.at = now;
        if self.left < 1.0 {
            return false;
        }
        self.left -= 1.0;
        true
    }
}

struct Open {
    room: Room,
    opened: Instant,
    /// The order rooms opened in (newest first in the list).
    order: u64,
}

#[derive(Default)]
pub struct Book {
    rooms: BTreeMap<String, Open>,
    by_conn: HashMap<u64, String>,
    opened: u64,
}

impl Book {
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// Answers `ask` from connection `conn`; `code` makes a new code (a
    /// random one; it is asked again while the code is taken).
    pub fn answer(&mut self, conn: u64, ask: Ask, now: Instant, code: &mut dyn FnMut() -> String) -> Answer {
        let answer = match ask {
            Ask::Open(room) => self.open(conn, room, now, code),
            Ask::Update(update) => self.update(conn, update),
            Ask::Join(join) => self.join(&join),
            Ask::List(search) => self.list(&search, now),
            Ask::Quick(search) => self.quick(&search),
        };
        OPEN.store(self.rooms.len(), Ordering::Relaxed);
        answer
    }

    /// The connection is gone, and its room with it; whether it had one.
    pub fn gone(&mut self, conn: u64) -> bool {
        let had = self.by_conn.remove(&conn).and_then(|code| self.rooms.remove(&code)).is_some();
        OPEN.store(self.rooms.len(), Ordering::Relaxed);
        had
    }

    fn open(&mut self, conn: u64, room: Room, now: Instant, code: &mut dyn FnMut() -> String) -> Answer {
        if self.by_conn.contains_key(&conn) {
            return Answer::No(Refusal::HasRoom);
        }
        if !room.is_valid() {
            return Answer::No(Refusal::Invalid);
        }
        if self.rooms.len() >= MOST_ROOMS {
            return Answer::No(Refusal::Full);
        }
        let code = loop {
            let c = code();
            if !self.rooms.contains_key(&c) {
                break c;
            }
        };
        self.opened += 1;
        self.rooms.insert(code.clone(), Open { room, opened: now, order: self.opened });
        self.by_conn.insert(conn, code.clone());
        Answer::Code(code)
    }

    fn update(&mut self, conn: u64, update: Update) -> Answer {
        let Some(open) = self.by_conn.get(&conn).and_then(|code| self.rooms.get_mut(code)) else {
            return Answer::No(Refusal::NoRoom);
        };
        let mut room = open.room.clone();
        if let Some(title) = update.title {
            room.title = title;
        }
        if let Some(seats) = update.seats {
            room.seats = seats;
        }
        if let Some(taken) = update.taken {
            room.taken = taken;
        }
        if let Some(attributes) = update.attributes {
            room.attributes = attributes;
        }
        if let Some(listed) = update.listed {
            room.listed = listed;
        }
        if !room.is_valid() {
            return Answer::No(Refusal::Invalid);
        }
        open.room = room;
        Answer::Ok {}
    }

    fn join(&self, join: &Join) -> Answer {
        let Some((code, open)) = protocol::tidy(&join.code).and_then(|c| self.rooms.get_key_value(&c)) else {
            return Answer::No(Refusal::UnknownCode);
        };
        if join.app.as_ref().is_some_and(|app| *app != open.room.app) {
            return Answer::No(Refusal::OtherApp);
        }
        if join.version.as_ref().is_some_and(|version| *version != open.room.version) {
            return Answer::No(Refusal::OtherVersion);
        }
        Answer::Found(found(code, open))
    }

    /// The listed rooms of the search's app and version with all its
    /// attributes, newest first.
    fn matching<'a>(&'a self, search: &'a Search) -> impl Iterator<Item = (&'a String, &'a Open)> + 'a {
        self.rooms.iter().filter(move |(_, open)| {
            let room = &open.room;
            room.listed && room.app == search.app && room.version == search.version && search.attributes.iter().all(|(k, v)| room.attributes.get(k) == Some(v))
        })
    }

    fn list(&self, search: &Search, now: Instant) -> Answer {
        let mut rooms: Vec<(&String, &Open)> = self.matching(search).collect();
        rooms.sort_by_key(|(_, open)| std::cmp::Reverse(open.order));
        let start = (search.page as usize).saturating_mul(protocol::PAGE);
        let page = rooms
            .iter()
            .skip(start)
            .take(protocol::PAGE)
            .map(|(code, open)| Listed {
                code: (*code).clone(),
                title: open.room.title.clone(),
                seats: open.room.seats,
                taken: open.room.taken,
                attributes: open.room.attributes.clone(),
                age: now.saturating_duration_since(open.opened).as_secs(),
            })
            .collect();
        Answer::Rooms(Rooms { rooms: page, more: rooms.len() > start.saturating_add(protocol::PAGE) })
    }

    fn quick(&self, search: &Search) -> Answer {
        let best = self.matching(search).filter(|(_, open)| open.room.taken < open.room.seats).max_by_key(|(_, open)| (open.room.taken, std::cmp::Reverse(open.order)));
        match best {
            Some((code, open)) => Answer::Found(found(code, open)),
            None => Answer::No(Refusal::NoRoomFree),
        }
    }
}

fn found(code: &str, open: &Open) -> Found {
    Found { code: code.to_string(), title: open.room.title.clone(), ticket: open.room.ticket.clone() }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn room(app: &str, version: &str, seats: u32, taken: u32) -> Room {
        Room { app: app.into(), version: version.into(), title: format!("{app} {taken}/{seats}"), seats, taken, ticket: format!("ticket of {app}"), listed: true, ..Room::default() }
    }

    /// The `n`th code of the tests (the doors make random ones).
    fn code(n: u32) -> String {
        (0..protocol::LENGTH).rev().map(|i| protocol::SIGNS[((n >> (5 * i)) & 31) as usize] as char).collect()
    }

    /// Codes in turn: `code(1)`, `code(2)` ...
    fn codes() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            code(n)
        }
    }

    fn open(book: &mut Book, conn: u64, room: Room, codes: &mut dyn FnMut() -> String) -> String {
        match book.answer(conn, Ask::Open(room), Instant::now(), codes) {
            Answer::Code(code) => code,
            other => panic!("{other:?}"),
        }
    }

    fn search(app: &str, version: &str) -> Search {
        Search { app: app.into(), version: version.into(), ..Search::default() }
    }

    /// A host's room is found by its code (typed any way), by a guest of
    /// the same game and version, and goes when the host's connection
    /// does.
    #[test]
    fn a_room_is_found_by_its_code_while_its_host_is_there() {
        let (mut book, mut codes, now) = (Book::default(), codes(), Instant::now());
        let code = open(&mut book, 1, room("pulsar", "7", 2, 1), &mut codes);
        let join = |code: &str, app: Option<&str>, version: Option<&str>| Ask::Join(Join { code: code.into(), app: app.map(Into::into), version: version.map(Into::into) });
        let found = book.answer(2, join(&code.to_lowercase(), Some("pulsar"), Some("7")), now, &mut codes);
        assert_eq!(found, Answer::Found(Found { code: code.clone(), title: "pulsar 1/2".into(), ticket: "ticket of pulsar".into() }));
        assert_eq!(book.answer(2, join(&code, None, None), now, &mut codes), found, "app and version are the guest's to give");
        assert_eq!(book.answer(2, join(&code, Some("sokovox"), None), now, &mut codes), Answer::No(Refusal::OtherApp));
        assert_eq!(book.answer(2, join(&code, Some("pulsar"), Some("6")), now, &mut codes), Answer::No(Refusal::OtherVersion));
        assert_eq!(book.answer(2, join("NOCODE", None, None), now, &mut codes), Answer::No(Refusal::UnknownCode));
        assert!(!book.gone(2), "a guest had no room");
        assert!(book.gone(1));
        assert_eq!(book.answer(2, join(&code, None, None), now, &mut codes), Answer::No(Refusal::UnknownCode));
        assert_eq!(book.len(), 0);
    }

    /// One room a connection; only its own is updated, and only within
    /// the limits; a code taken already is never given twice.
    #[test]
    fn a_host_has_one_room_and_updates_only_its_own() {
        let (mut book, now) = (Book::default(), Instant::now());
        let mut same = || "SAMECD".to_string();
        let mut codes = codes();
        assert_eq!(open(&mut book, 1, room("a", "1", 2, 1), &mut same), "SAMECD");
        let mut twice = 0;
        let mut once_more = || {
            twice += 1;
            if twice == 1 { "SAMECD".to_string() } else { "ZWEIXY".to_string() }
        };
        assert_eq!(open(&mut book, 2, room("a", "1", 2, 1), &mut once_more), "ZWEIXY", "a code in use is made again");
        assert_eq!(book.answer(1, Ask::Open(room("a", "1", 2, 1)), now, &mut codes), Answer::No(Refusal::HasRoom));
        assert_eq!(book.answer(3, Ask::Update(Update { taken: Some(2), ..Update::default() }), now, &mut codes), Answer::No(Refusal::NoRoom));
        assert_eq!(book.answer(1, Ask::Update(Update { taken: Some(3), ..Update::default() }), now, &mut codes), Answer::No(Refusal::Invalid), "more taken than seats");
        assert_eq!(book.answer(1, Ask::Update(Update { taken: Some(2), title: Some("full".into()), ..Update::default() }), now, &mut codes), Answer::Ok {});
        let Answer::Rooms(rooms) = book.answer(9, Ask::List(search("a", "1")), now, &mut codes) else { panic!() };
        assert_eq!(rooms.rooms.iter().map(|r| (r.code.as_str(), r.title.as_str(), r.taken)).collect::<Vec<_>>(), [("ZWEIXY", "a 1/2", 1), ("SAMECD", "full", 2)]);
        assert_eq!(book.answer(4, Ask::Open(Room { seats: 0, ..room("a", "1", 0, 0) }), now, &mut codes), Answer::No(Refusal::Invalid));
    }

    /// The list holds the listed rooms of one app and version with the
    /// attributes asked for, newest first, a page at a time.
    #[test]
    fn the_list_shows_the_rooms_that_fit_newest_first() {
        let (mut book, mut codes) = (Book::default(), codes());
        let start = Instant::now();
        for n in 0..25u64 {
            let mut r = room("pulsar", "7", 2, 1);
            r.attributes.insert("mode".into(), if n % 5 == 0 { "versus" } else { "coop" }.into());
            book.answer(n, Ask::Open(r), start + Duration::from_secs(n), &mut codes);
        }
        book.answer(100, Ask::Open(room("pulsar", "6", 2, 1)), start, &mut codes);
        book.answer(101, Ask::Open(room("sokovox", "7", 2, 1)), start, &mut codes);
        book.answer(102, Ask::Open(Room { listed: false, ..room("pulsar", "7", 2, 1) }), start, &mut codes);
        let now = start + Duration::from_secs(30);
        let Answer::Rooms(first) = book.answer(9, Ask::List(search("pulsar", "7")), now, &mut codes) else { panic!() };
        assert_eq!((first.rooms.len(), first.more), (protocol::PAGE, true));
        assert_eq!((first.rooms[0].code.clone(), first.rooms[0].age), (code(25), 6), "the newest first, its age in seconds");
        let Answer::Rooms(second) = book.answer(9, Ask::List(Search { page: 1, ..search("pulsar", "7") }), now, &mut codes) else { panic!() };
        assert_eq!((second.rooms.len(), second.more), (5, false), "another version, another game and an unlisted room are not in it");
        let versus = Search { attributes: [("mode".to_string(), "versus".to_string())].into(), ..search("pulsar", "7") };
        let Answer::Rooms(versus) = book.answer(9, Ask::List(versus), now, &mut codes) else { panic!() };
        assert_eq!(versus.rooms.iter().map(|r| r.code.clone()).collect::<Vec<_>>(), [code(21), code(16), code(11), code(6), code(1)]);
    }

    /// A quick join finds a free seat in the fullest room that fits, the
    /// oldest of equals; never a full one.
    #[test]
    fn a_quick_join_fills_the_fullest_room_first() {
        let (mut book, mut codes, now) = (Book::default(), codes(), Instant::now());
        book.answer(1, Ask::Open(room("a", "1", 4, 1)), now, &mut codes);
        book.answer(2, Ask::Open(room("a", "1", 4, 4)), now, &mut codes);
        book.answer(3, Ask::Open(room("a", "1", 4, 2)), now, &mut codes);
        book.answer(4, Ask::Open(room("a", "1", 4, 2)), now, &mut codes);
        let quick = |book: &mut Book, codes: &mut dyn FnMut() -> String| match book.answer(9, Ask::Quick(search("a", "1")), now, codes) {
            Answer::Found(found) => found.code,
            other => panic!("{other:?}"),
        };
        assert_eq!(quick(&mut book, &mut codes), code(3), "the fullest with a free seat, the older of two");
        book.gone(3);
        book.gone(4);
        assert_eq!(quick(&mut book, &mut codes), code(1));
        book.gone(1);
        assert_eq!(book.answer(9, Ask::Quick(search("a", "1")), now, &mut codes), Answer::No(Refusal::NoRoomFree));
    }

    /// A connection may ask a burst, then a few a second.
    #[test]
    fn a_connection_asks_a_burst_then_slowly() {
        let start = Instant::now();
        let mut allowance = Allowance::new(start);
        assert!((0..20).all(|_| allowance.take(start)));
        assert!(!allowance.take(start));
        assert!(allowance.take(start + Duration::from_millis(500)));
        assert!(!allowance.take(start + Duration::from_millis(600)));
        assert!((0..20).all(|_| allowance.take(start + Duration::from_secs(60))), "it fills up to the burst again");
    }
}
