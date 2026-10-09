# NoammProxy (Rust)

Rust rewrite of the Kotlin/Ktor NoammProxy Minecraft proxy. Listens on `25565`,
answers server-list pings with a custom MOTD

Reads `/apps/files/proxy.json` and `/apps/files/server.png`

## Build

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

## Config

Both files are read once at startup, so restart the proxy after editing them.
A missing file or broken JSON crashes it on boot with the file name in the error.

### `proxy.json`

```json
{
  "server": "mc.hypixel.net",
  "version": "26.1.2",
  "protocol": 775,
  "desc": "§bNoamm's Proxy. 26.1.2",
  "maxPlayers": 67,
  "onlinePlayers": 69,
  "whitelist": ["Noamm", "NoammALT"]
}
```

All keys are required.

- `server` - where logins are forwarded. Also written into the handshake as the
  hostname, and used for the ping shown in the MOTD.
- `version` / `protocol` - server list only. The connection itself passes
  through whatever the client sends.
- `desc` - first MOTD line, `§` codes work. The second line is always
  `Server: <server> | Ping: <ms>ms`.
- `maxPlayers` / `onlinePlayers` - shown in the server list, can be anything.
- `whitelist` - usernames allowed to log in, case-sensitive. Only the name the
  client sends is checked; the target server still does the real auth.

### `server.png`

Server list icon, sent as-is. Use a 64x64 PNG.