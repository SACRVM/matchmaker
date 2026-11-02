# What matchmaker is to become

Stages 1 to 3 are built; the rest is the plan.

## Goals

- **Any peer-to-peer game** finds its players through it: by a code said
  aloud, by a list of open games, or by "put me in any game".
- **One program, no database, no accounts.** A game exists while its host
  is connected. Everything lives in memory and is gone after a restart.
- **The game decides** what a match means. The matchmaker keeps opaque
  tickets and attributes and compares them only as the game asked. Nothing
  in it knows one game from another.
- **Easy to run**: one file, a reverse proxy in front, optionally Husky.

## Not its job

Accounts and logins, friends lists, chat, ranking and skill-based
matchmaking, hosting game servers, storing anything across restarts,
anti-cheat. A game that needs them keeps them elsewhere.

## Concepts

- **App**: a game, named by the game (`voxcade/pulsar`), with a version
  string. Only games of the same app and the same version meet. Voxcade
  puts its data's fingerprint there.
- **Room**: an open game. Its host gives a title, the seats (how many, how
  many taken), attributes (`mode = "coop"`, `lang = "de"`, all strings)
  and the ticket. The host changes seats and attributes while it plays. A
  room has a code and lives as long as the host's connection.
- **Ways in**:
  - by code (stage 1): the ticket of the room with this code;
  - the list: the rooms of an app and version, filtered by attributes,
    newest first, a page at a time;
  - quick join: the room of an app and version with a free seat and all
    the given attributes: the fullest, the oldest of equals.
- **Ticket**: whatever the guest needs to reach the host, opaque.

## Doors

- **iroh** (stage 1): reached through the built-in relay, no port of its
  own. For games that use iroh already (Rust).
- **WebSocket with JSON** (`wss://.../matchmaker/1?token=TOKEN`, the
  protocol's version in the path: a subprotocol would have to be echoed
  only when asked, which the handshake does before the path is read):
  through the same reverse proxy. For every engine (Unity and C#, Godot,
  a browser). The room lives as long as the socket.

Both doors lead to the same rooms, so a game reachable both ways meets
its players on either.

## The relay

iroh's relay stays built in, behind the token. Games with other
transports bring their own (a TURN server for WebRTC).

## Limits against misuse

Built: the token for each deployment, one room for each connection, a
cap on rooms (10,000), asks for each connection (20 at once, then 2 a
second), new connections to the relay (20 a second). Planned: a cap on
connections, connections from one address per minute (behind the proxy
from its forwarded address). Optional: a list of the apps that may open
rooms.

## Stages

1. **Codes and relay** (built): a host's ticket under a code while it is
   connected; the iroh door; Husky; releases for Linux and Windows.
   Tried under Husky 0.5.1 (Linux, a local manifest): installed, its
   config made on the first start, an update cut over in a moment with
   the key kept, and a game's room opened again with a new code.
2. **Apps and rooms** over iroh (built, 0.2): app and version, seats,
   attributes, the list, quick join, refusals as words. The rooms live
   apart from the door (`server/src/book.rs`), so the next door shares
   them. Voxcade's NETWORK page lists open games across the internet
   beside those on the LAN.
3. **The WebSocket door** (built, 0.3): the same rooms, a page in a
   browser speaking it (`examples/browser/lobby.html`), the protocol in
   the README for other languages. Pings every 20 s; a socket silent for
   a minute is closed.
4. **Examples for Unity and Godot**, when someone asks for them.
