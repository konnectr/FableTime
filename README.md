# Time Tracker

A small, fast **desktop time tracker** written in Rust on [GPUI](https://github.com/zed-industries/zed)
(Zed's GPU-accelerated UI framework). One codebase runs natively on **Windows, macOS, and Linux**,
storing everything in a local SQLite file — no server, no account. The UI follows an approved design.

## Features

- **Tracker** — a work bar (what are you working on? + project picker + a live clock + Start/Stop)
  and today's entries with a running highlight and one-click replay.
- **Calendar** — a Mon–Sun **week timeline** of project-colored time blocks, with an inline form to
  add, edit, or delete entries.
- **Projects** — project cards showing client, weekly total, entry count, and a Mon–Sun sparkline;
  create projects inline (each gets a palette color).
- **Export** — pick a period and an optional project filter, choose **CSV / JSON / Markdown**, and
  write a report (with per-project and per-day totals) into a folder you select.

A time entry belongs to a **project** and carries a free-text **description** — a live timer and a
hand-entered calendar entry are the same kind of record. Only one timer runs at a time.

## Tech

| Concern | Choice |
|---|---|
| UI | `gpui` + `gpui_platform` (git, zed-industries) |
| Widgets | [`gpui-component`](https://github.com/longbridge/gpui-component) — Input, Button, Icon, theming |
| Storage | `rusqlite` with the `bundled` feature → SQLite statically linked, true single binary |
| Dates | `chrono` (timestamps stored as UTC RFC3339) |
| Data dir | `directories` (per-OS app-data path) |
| Save dialog | `rfd` (native folder picker) |
| Export | `csv`, `serde_json`, hand-built Markdown |

## Building

Requires a recent **stable Rust** (edition 2024, i.e. Rust ≥ 1.85). Install via [rustup](https://rustup.rs).
The first build compiles a large dependency tree (gpui is pulled from git); later builds are incremental.

### Platform prerequisites

**macOS** — full **Xcode** is needed to compile GPUI's Metal shaders. On macOS 26 / Xcode 26 the Metal
compiler is a separate download:

```bash
xcodebuild -downloadComponent MetalToolchain        # one-time, ~688 MB
sudo xcode-select -s /Applications/Xcode.app        # or prefix builds with DEVELOPER_DIR=…
```

**Linux** — install the graphics/font dev packages GPUI needs:

```bash
sudo apt-get install -y \
  libwayland-dev libxkbcommon-x11-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libfontconfig-dev libfreetype-dev \
  libvulkan-dev libgl1-mesa-dev \
  libssl-dev libzstd-dev pkg-config cmake clang
```

**Windows** — nothing extra (GPUI uses the DirectX 11 backend).

### Build & run

```bash
cargo run --release
# macOS, if you didn't switch xcode-select:
# DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo run --release
```

Two env overrides help testing: `TIMETRACKER_DB=<path>` uses a throwaway database, and
`TIMETRACKER_TAB=tracker|calendar|projects|export` chooses the initial tab.

## Data

A single SQLite file `timetracker.db` is created in your OS application-data directory (via the
`directories` crate), e.g. `~/Library/Application Support/dev.timetracker.TimeTracker/` on macOS,
`~/.local/share/timetracker/` on Linux, `%APPDATA%\timetracker\TimeTracker\data\` on Windows.
Your data persists across restarts.

## Privacy

FableTime stores everything — projects, entries, settings — in a local SQLite file on your own
device. There is no account, no server, no telemetry, and no network access: nothing you track
ever leaves your machine.

## Export formats

Choosing a period + formats and clicking **Export** writes the selected formats into a folder:

- `report.csv` — one row per entry (date, project, description, start, end, duration) + total rows.
- `report.json` — `{ rows, totals_by_project, totals_by_day, total_seconds, total_hms }`.
- `report.md` — a Markdown table plus "Totals by project / by day" sections and a grand total.

## Releases & CI

`.github/workflows/build.yml` builds release binaries for Windows, macOS, and Linux on every push.
Pushing a `v*` tag also **publishes a GitHub Release** with all three binaries attached. See
[CHANGELOG.md](CHANGELOG.md).

### Windows SmartScreen

The Windows binary isn't code-signed yet, so on first run SmartScreen shows an "unrecognized app"
prompt — click **More info → Run anyway**. Code signing (via SignPath, free for OSS) is being set
up to remove this; the CI workflow already has the signing step, gated until it's configured.

## Project layout

```
src/
  main.rs        app bootstrap + window
  palette.rs     design colors
  models.rs      data types + time helpers
  db.rs          SQLite: migrations, CRUD, totals, project stats, range export
  app.rs         shared app state (DB, running entry, timer)
  exporter.rs    CSV / JSON / Markdown serialization
  ui/            root shell + tracker / calendar / projects / export views
```

See [CLAUDE.md](CLAUDE.md) for contributor/agent build notes and the gpui rev-pinning gotcha.
