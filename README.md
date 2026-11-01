# matchmaker

Game codes and a relay for peer-to-peer games, in one program.

A host opens a room for its game and gets a code of six signs, like
`DX67A3`, to say on a call or send in a chat. Guests find the room by
its code, in the list of open rooms of their game, or by asking for any
room with a free seat; the matchmaker answers with what they need to
reach the host. Machines whose routers will not let them reach each other
directly talk through the relay. There are no accounts and no database:
a room lives exactly as long as its host stays connected.

It was made for [Voxcade](https://voxca.de)'s network games and is meant
for any peer-to-peer game. Status: 0.2, rooms, the list, quick join and
the relay over iroh. A door for every engine (WebSocket) is planned
([docs/design.md](docs/design.md)).

## How it fits together

```
           443 (TLS)                      plain HTTP
Internet ───────────> reverse proxy ──────────────────> matchmaker
                      (Caddy, ...)        127.0.0.1:7340   ├── relay (iroh)
                                                           └── codes (an iroh endpoint,
                                                               reached through the relay)
```

- **The relay** is [iroh](https://iroh.computer)'s. Every machine meets the
  others there first, and stays on it where no direct path can be found.
  Only machines that show the token are relayed.
- **The rooms** are reached through an iroh endpoint with a key of its
  own. Games know the key's public half and reach it through the relay,
  so the program needs no port beyond the relay's.
- **The ticket** a host leaves in its room is opaque to the matchmaker
  (up to 1,024 bytes): an iroh address, an IP and port, whatever the game
  joins by. Only a guest who joins the room is given it.

## Running it

```
matchmaker                   # config: data/matchmaker.toml beside the program
matchmaker --config FILE
matchmaker --new [--config FILE]
```

The first start (or `--new`) writes the config with a new token and key
and prints what the games need:

```
for the games (the relay's token, the codes' public key):
  token: 6c1f...
  codes: 8886b3d3...
```

The secret key stays in the config file. Keep that file on the server and
out of every repository. A new key or token means new builds of every
game that uses it.

The config:

```toml
relay = "127.0.0.1:7340"   # where the relay listens, behind the reverse proxy
token = "..."              # the token games show the relay
key = "..."                # the codes' secret key
```

### Behind a reverse proxy

The proxy holds the certificate and passes everything on, upgrades
included. With Caddy:

```
relay.example.com {
    reverse_proxy 127.0.0.1:7340
}
```

Games then use `https://relay.example.com` as their relay.

### Under Husky

[Husky](https://github.com/SACRVM/husky) can run it on a server, restart
it and install new releases. `husky.config.json` in this repository tells
Husky where the releases are. On Windows wrap Husky in a service with
NSSM, as Husky's README shows:

```powershell
nssm install Husky-Matchmaker "C:\Apps\matchmaker\Husky.exe" "--repo SACRVM/matchmaker"
nssm set Husky-Matchmaker AppDirectory "C:\Apps\matchmaker"
nssm set Husky-Matchmaker AppStdout    "C:\Apps\matchmaker\logs\husky.out.log"
nssm set Husky-Matchmaker AppStderr    "C:\Apps\matchmaker\logs\husky.err.log"
nssm start Husky-Matchmaker
```

The config then lives in `app\data\`, which Husky never touches. When
Husky asks the matchmaker to stop for an update, it stops at once; open
games lose their codes, and guests already in play on.

## The protocol

`matchmaker-protocol` (in [protocol/](protocol/)) holds the words both
sides share. Over iroh, on the ALPN `matchmaker/1`, one JSON value is
asked on a stream and one answered:

| Who | Asks | Answer |
|---|---|---|
| a host | `{"open": {"app": "voxcade/pulsar", "version": "7f3a", "title": "Anna", "seats": 2, "taken": 1, "attributes": {"mode": "coop"}, "ticket": "..."}}` | `{"code": "DX67A3"}` |
| the host | `{"update": {"taken": 2}}` (any of title, seats, taken, attributes, listed) | `{"ok": {}}` |
| a guest | `{"join": {"code": "dx6-7a3", "app": "voxcade/pulsar", "version": "7f3a"}}` | `{"found": {"code": "DX67A3", "title": "Anna", "ticket": "..."}}` |
| a guest | `{"list": {"app": "voxcade/pulsar", "version": "7f3a", "attributes": {"mode": "coop"}, "page": 0}}` | `{"rooms": {"rooms": [{"code", "title", "seats", "taken", "attributes", "age"}], "more": false}}` |
| a guest | `{"quick": {"app": "voxcade/pulsar", "version": "7f3a"}}` | `{"found": {...}}` |

Any ask may be answered `{"no": "unknown-code"}`; the refusals are
words for the game to say in its own: `unknown-code`, `other-app`,
`other-version`, `no-room-free`, `has-room`, `no-room`, `invalid`,
`not-understood`, `too-fast`, `full`.

- A room belongs to the connection that opened it (one a connection) and
  goes when it closes. `"listed": false` keeps it out of the list and
  quick joins: found by its code only.
- Only rooms of the same app and version meet. The list holds the listed
  rooms with all the attributes asked for, newest first, 20 a page; a
  quick join takes the fullest room with a free seat, the oldest of
  equals.
- Codes are six of 32 signs (no 0, O, 1 or I); a guest may type them in
  small letters, with a space or a dash; app and version in a join are
  optional (given, another game's code is refused as such).
- Limits: 10,000 rooms; a connection asks 20 at once, then 2 a second;
  an app 64 bytes, a version 128, a title 64, a ticket 1,024, 64 seats,
  16 attributes of 32-byte keys and 64-byte values.

## Building

```
cargo build --release
```

Releases for Linux and Windows are built by the `Release` workflow when a
version tag (`v0.1.0`) is pushed.

## Licence

MIT, see [LICENSE](LICENSE).
