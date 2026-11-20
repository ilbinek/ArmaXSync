# Changelog

All notable changes to ArmaXSync. The section for a version becomes its GitHub
release notes, which the desktop app and the server show before updating.
Run `task release:bump -- X.Y.Z` to turn "Unreleased" into a version.

## [Unreleased]

## [0.2.0] - 2026-11-20

### Added
- Automatic updates: the desktop app installs signed updates from GitHub releases; the server checks for new releases, shows them in the admin UI and can update itself with one click when run under systemd.
- Signed release artifacts, `SHA256SUMS`, and `server-latest.json` / `latest.json` update manifests.
- Redesigned desktop app and web admin: a dark "tactical" theme (light theme available), icons, and clear loading, empty and error states.
- Desktop: first-run setup, a launch readiness checklist with "Sync & launch", a downloads panel, sync previews grouped by addon, drag-and-drop load order, a live RPT log with error highlighting, and build/push progress.
- Admin: a dashboard, staged file changes shown in the file tree with per-item undo, an upload queue, publish review with conflict detection, a job log drawer, and readable audit log and statistics.
- Public repository pages share the admin look, with join instructions and a download link.
- In-depth statistics in the admin UI, also available per repository:
  - traffic by type and by repository (hourly or daily)
  - unique players: daily, weekly, monthly, and new vs returning
  - client programs and ArmaXSync versions
  - top addons
  - peak-hours heatmap
  - download speed, aborted transfers and errors
  - repository size and publish history, job success rates and durations
  - a live view of current activity
  - CSV export
- Players are counted privately: a monthly-salted IP hash folded into HyperLogLog sketches. IP addresses are never stored. Configured under `[stats]`, with retention settings.
- `trusted_proxies` accepts CIDR ranges.
- Topographic map backdrop, page transitions and a command palette (Ctrl+K) in the admin UI; live speed sparklines in the desktop app.

### Fixed
- Saving repository settings no longer overwrites events or mirror settings changed elsewhere.
- One unreachable repository no longer stops the desktop app from checking the others.
- Sign-out is available on phones; uppercase repository names open correctly after creation.

### Changed
- Release assets are named per target (`armaxsync-server-x86_64-unknown-linux-musl`, …).
- Docker images are published as `ghcr.io/ilbinek/armaxsync-server` with `X.Y.Z`, `X.Y` and `latest` tags.

## [0.1.0]

- First release: CLI, desktop client and repository server, wire-compatible with ArmA3Sync.
