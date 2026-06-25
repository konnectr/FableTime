# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.1] — 2026-06-25

### Fixed
- Windows: the app no longer opens a console window alongside it — release builds
  now use the Windows GUI subsystem.

## [0.1.0] — 2026-06-25

First release — a cross-platform desktop time tracker built in Rust on GPUI with
local SQLite storage, styled to match the approved Time Tracker design.

### Features
- **Tracker** — a work bar (description + project picker + live clock + Start/Stop)
  and today's entries with a running highlight and one-click replay.
- **Calendar** — a Mon–Sun week timeline with project-colored time blocks, plus an
  inline form to add / edit / delete entries.
- **Projects** — project cards with client, weekly total, entry count and a Mon–Sun
  sparkline; create projects inline (auto-assigned palette colors).
- **Export** — pick a period and project filter, choose CSV / JSON / Markdown, and
  write a report (with per-project and per-day totals) to a folder.

### Details
- Data model: a time entry belongs to a project and carries a free-text description
  (no separate task layer). One timer runs at a time.
- Storage: SQLite via `rusqlite` (bundled → single binary), in the per-OS data dir.
- Timestamps stored as UTC RFC3339; durations and day/week grouping in local time.
- UI palette and typography extracted from the approved design (`src/palette.rs`).

### Platforms
- Prebuilt binaries for Windows, macOS and Linux are attached to this release (built
  by CI). Building on macOS needs the Xcode Metal toolchain — see the README.

[0.1.1]: https://github.com/konnectr/FableTime/releases/tag/v0.1.1
[0.1.0]: https://github.com/konnectr/FableTime/releases/tag/v0.1.0
