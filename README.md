# matchmaker

Game codes and a relay for peer-to-peer games, in one program.

A host opens a game and gets a code of six signs, like `DX67A3`, to say
on a call or send in a chat. A guest types the code, and the matchmaker
answers with what the guest needs to reach the host. Machines whose
routers will not let them reach each other directly talk through the
relay. There are no accounts and no database: a code lives exactly as
long as its host stays connected.

It was made for [Voxcade](https://voxca.de)'s network games and is meant
for any peer-to-peer game. Status: 0.1, the codes and the relay. Lobbies
to browse, quick join and a door for every engine are planned
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
- **The codes** are an iroh endpoint with a key of its own. Games know the
  key's public half and reach it through the relay, so the program needs
  no port beyond the relay's.
- **The ticket** a host leaves under its code is opaque to the matchmaker
  (up to 1,024 bytes): an iroh address, an IP and port, whatever the game
  joins by.

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
sides share. Over iroh, on the ALPN `matchmaker/codes/1`, one JSON value
is asked on a stream and one answered:

| Who | Asks | Answer |
|---|---|---|
| a host | `{"host": "TICKET"}` | `{"code": "DX67A3"}`, kept while this connection lasts |
| a guest | `{"join": "dx6-7a3"}` | `{"ticket": "TICKET"}` or `{"no": "no game with this code"}` |

Codes are six of 32 signs (no 0, O, 1 or I); a guest may type them in
small letters, with a space or a dash. Limits: 10,000 open games, 20
lookups a connection.

## Building

```
cargo build --release
```

Releases for Linux and Windows are built by the `Release` workflow when a
version tag (`v0.1.0`) is pushed.
