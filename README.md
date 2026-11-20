# ArmaXSync

A Rust rewrite of [ArmA3Sync](https://github.com/DanAlbert/arma3-sync): sync Arma mod repositories, launch the game with modsets, and host repositories with a web administration UI.

ArmaXSync is **wire-compatible with ArmA3Sync**:
- Legacy clients sync from repositories built or hosted by ArmaXSync.
- ArmaXSync syncs from repositories built by the legacy app, over HTTP(S) or FTP.

| Component | Binary | What it does |
|---|---|---|
| CLI | `armaxsync` | Sync, check, verify, build and push repositories; manage profiles; launch the game. Usable in scripts: `--json` output and real exit codes. |
| Desktop | `armaxsync-gui` | Tauri app for Windows and Linux: Play, Modsets, Repositories, Servers, Tools and Settings. |
| Server | `armaxsync-server` | Hosts repositories over HTTP with Range support. Web admin UI, uploads and file manager, delta push, mirroring, cron schedules, users and API tokens, audit log, statistics and Prometheus metrics. |

## Quick start: hosting a repository

```sh
# Docker
docker compose -f packaging/docker/compose.yaml up -d   # ghcr.io/ilbinek/armaxsync-server; set ARMAXSYNC_PUBLIC_URL first
docker compose logs | grep "setup token"                # open http://host:8080/admin/

# or the static binary
armaxsync-server example-config > config.toml          # edit public_url, data_dir
armaxsync-server --config config.toml init --username admin
armaxsync-server --config config.toml
```

The server speaks plain HTTP. Put a reverse proxy (Caddy, nginx, Traefik) in front of it for TLS, and set `public_url` to the proxy's `https://` URL.

Repositories are served at `https://host/r/<slug>/`. Players have three ways to add one:
- `armaxsync://` link (one click)
- ArmA3Sync auto-config URL: `https://host/r/<slug>/.a3s/autoconfig`
- the public page: `https://host/repo/<slug>`

Ways to get content into a repository:
- **Web UI**: drag and drop files, folders or zip archives. Changes are staged and published atomically.
- **Desktop or CLI push**: delta upload with an API token:
  `armaxsync push --server https://host --server-repo main --token axs_… --path D:\mods --delete`
- **Mirror**: pull another ArmA3Sync or ArmaXSync repository, on demand or on a cron schedule.
- **External folder**: point a repository at an existing folder (edited over SFTP or SMB) and rebuild.

## Quick start: players (CLI)

```sh
armaxsync repo add https://host/r/main/.a3s/autoconfig --dest D:\ArmaMods
armaxsync check
armaxsync sync "Main Repo"            # --event "Sunday Op", --dry-run, --exact
armaxsync import-legacy C:\ArmA3Sync   # repositories, profiles, servers from an old install
armaxsync modset "Main Repo" --event "Sunday Op"
armaxsync launch --server "Main Server" --dry-run
```

Legacy command lines are still accepted: `-sync <repo> <dest> <true|false>`, `-build <repo>`, `-check <repo>` and `-extract <src> <dst>`.

**Exit codes:**
| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Error |
| 2 | Usage |
| 3 | Not found |
| 4 | Network |
| 5 | Verification failed |
| 10 | Updates available (with `check --fail-on-update`) |

## Statistics

The admin UI has a **Statistics** page and a Stats tab on each repository:
- traffic over time
- unique players
- client programs and versions
- the most downloaded addons
- a weekday × hour peak-time heatmap
- publish history and job health
- a live view of current downloads

Every chart can be exported as CSV, and Prometheus metrics are at `/metrics`.

Privacy:
- **Players are counted by a salted hash of their IP address. The address itself is never stored.**
- The hash salt changes every month and the old one is deleted, so players can't be linked from one month to the next.
- The hashes are folded into HyperLogLog sketches, which store only counts.
- Set `players = false` under `[stats]` to turn player counting off completely.

Retention is set under `[stats]`: daily data is kept 365 days and hourly data 90 days by default.

**Behind a reverse proxy**, add the proxy's address or network to `trusted_proxies` (CIDR ranges work). Otherwise every player appears to come from the proxy's address. In Docker, that means the network your proxy connects from, for example `"172.16.0.0/12"`; set it with `ARMAXSYNC_TRUSTED_PROXIES=127.0.0.1,::1,172.16.0.0/12` if you have no config file.

## Updates

ArmaXSync updates itself from [GitHub releases](https://github.com/ilbinek/ArmaXSync/releases). Every release artifact is signed (minisign), and updates are verified against the public key compiled into the apps.

- **Desktop:** checks at startup and every 6 hours, and shows an update button in the sidebar. Windows installs and AppImages update in place and restart. deb/rpm and portable installs only get the notice. To install updates automatically, enable it in *Settings → Updates*.
- **Server:** checks every 6 hours (`[updates]` in `config.toml`), and the admin UI shows new versions with their release notes.
  - **deb/rpm (systemd):** the owner can click **Update and restart**. The server downloads and verifies the binary, then a root-owned helper (`armaxsync-server-update.path`) re-verifies it with the installed binary, swaps it in, restarts the service, and rolls back if the new version does not come up. Set `auto_install = true` to install automatically in the `auto_install_cron` window.
  - **Docker:** run `docker compose pull && docker compose up -d`, or enable the Watchtower example in `packaging/docker/compose.yaml`.

## Releasing

```sh
task release:bump -- 0.2.0       # sets the version everywhere and opens its CHANGELOG section
# edit CHANGELOG.md (it becomes the release notes shown in the apps), commit
git tag v0.2.0 && git push origin v0.2.0
```

The release workflow builds everything into a draft release and publishes it when all jobs succeed. It produces:
- server and CLI binaries per target
- deb/rpm packages
- desktop installers
- `latest.json` (desktop updater) and `server-latest.json` (server updater)
- `SHA256SUMS`
- Docker images

Tags with a suffix (`v0.2.0-rc.1`) become pre-releases, which updaters ignore.

Repository secrets: `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The matching public key is in `apps/desktop/tauri.conf.json` and `apps/server/src/updates.rs`. To rotate the key, run `pnpm -C ui/apps/desktop tauri signer generate` and update the secrets and both public-key locations.

## UI development

`task ui:build` builds both web UIs. To work on the desktop UI in a normal browser with fake data, no Tauri needed:

```sh
VITE_MOCK=1 pnpm -C ui dev:desktop     # http://localhost:1420  (?empty = first run, ?theme=light)
```

## Building

With [Task](https://taskfile.dev) installed: `task setup` once, then `task desktop`, `task server` or `task cli -- …`. Run `task` to list everything. Dev runs keep their data in `./.data`.

Manually:

```sh
cargo build --release -p armaxsync -p armaxsync-server
pnpm -C ui install && pnpm -C ui build          # admin + desktop UIs (Svelte 5)
cargo build --release -p armaxsync-server       # embeds ui/apps/admin/build
cd apps/desktop && npx --prefix ../../ui/apps/desktop tauri build
```

## Repository layout

```
crates/javaser    Java serialization stream reader/writer (legacy .a3s files)
crates/legacy     typed ArmA3Sync formats (+ encrypted client files, for import)
crates/zsync      jazsync-compatible .zsync maker, matcher, range planning
crates/model      modern manifest (.axs/index.json + manifest.<rev>.json.zst)
crates/build      repository builder (hashes, sidecars, legacy + modern metadata)
crates/transport  HTTP(S)/FTP access, rate limiting
crates/sync       planner + executor (delta, resume, segmented, zip, verify)
crates/client     client services (registry, status, push, legacy import)
crates/launcher   addon discovery, profiles, -mod line, Steam/Proton launch, tools
apps/cli | apps/server | apps/desktop | ui/ (Svelte)
tools/java-compat Java harness: golden fixtures + end-to-end tests with the original client
```

## Compatibility testing

`tools/java-compat` compiles the original ArmA3Sync classes:

```sh
tools/java-compat/build.sh && AXS_JAVA_COMPAT=1 cargo test -p axs-legacy -p axs-zsync
tools/java-compat/build-legacy-app.sh
tools/java-compat/e2e-legacy-client.sh       # legacy client ← Rust-built repository
tools/java-compat/e2e-rust-client.sh         # Rust client ← legacy-built repository
tools/java-compat/e2e-legacy-vs-server.sh    # legacy client ← armaxsync-server
```

Known legacy quirks that ArmaXSync works around or documents:
- The legacy zsync maker crashes on files of exactly 8191 bytes.
- The legacy `.pbo.zip` builder fails on Java ≥ 9.
- The legacy client can corrupt its own encrypted repository settings file.
- The legacy client sends raw UTF-8 bytes in its request paths. The server accepts them.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
