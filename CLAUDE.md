# matchmaker

Game codes and a relay for peer-to-peer games, in one program (Rust,
iroh). Open source, any game; Voxcade (a separate project) is its first
user. `README.md` says what it does, `docs/design.md` what it is to
become (stages; build a stage only once the user agreed to it).

## Layout

- `protocol/`: `matchmaker-protocol`, the words a game and a matchmaker
  share (serde only). A change to it changes the ALPN's version.
- `server/`: `matchmaker`, the program: `main.rs` (config, the relay,
  stopping), `book.rs` (the rooms and what each ask does to them, apart
  from any door; most tests), `door.rs` (the iroh door, its tests through
  a real relay in one process), `ws.rs` (the WebSocket door, its tests
  with a WebSocket client), `husky.rs` (Husky's pipe, LEASH).
- `examples/browser/lobby.html`: a page that speaks the WebSocket door.
  Node's WebSocket is a browser's: `node` tries a door as a page would.
- `husky.config.json`: where Husky finds the releases; `.github/workflows/
  release.yml` builds them, and publishes them on a version tag.

## Commands

- Build and test: `cargo build --release`, `cargo test --release`,
  `cargo clippy --release`.
- Run locally: `target/release/matchmaker --new --config FILE` once, then
  `--config FILE` (127.0.0.1:7340). A Voxcade game tries it with
  `VOXCADE_RELAY=http://127.0.0.1:7340 VOXCADE_RELAY_TOKEN=... VOXCADE_CODES=...`.
- Releases: raise the workspace version, push a tag `vX.Y.Z` (only when
  the user says so).

## Rules

- Nothing in it knows one game from another: tickets and attributes are
  opaque, a game's meaning stays in the game.
- No deployment details in this repository (addresses, host names, the
  user's server): those are in the knowledge base.

## Firepit inbox

At the start of a session, read any pending messages in `.firepit/inbox/*.md` — cross-project notes Firepit routes here. Act on each, then mark it done with the `firepit_inbox_complete` MCP tool, passing the message's filename as the `id`.

## Firepit knowledge

Before researching something that may already be known, query the knowledge base with the `firepit_knowledge_search` MCP tool (scope `both` covers this project plus the global base). Save durable findings with `firepit_knowledge_add` — written in English, per the indexing convention. This repository is public: `.firepit/knowledge` is a pointer, and the documents live in the private central store.

## Firepit pinned knowledge

@.firepit/knowledge-pinned.md

The import above auto-loads the knowledge docs marked `pin: true` in their frontmatter — always-on rules that apply every session without a search. Firepit regenerates the file from the pinned docs; don't edit it directly. Pin/unpin via the pinned flag on `firepit_knowledge_add` / `firepit_knowledge_update`, and keep the pinned set small — everything else stays reachable through `firepit_knowledge_search`.

## Firepit conventions

<!-- claude-firepit-fragments -->

@../.firepit/projects/claude.md
@../.firepit/projects/claude-github-public.md

The two imports above are shared files in the Firepit central repo — edit them there and every project follows. They carry policy; the tools themselves are described by Firepit's MCP server at the handshake, so nothing is duplicated between the two.
