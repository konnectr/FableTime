# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.4.0](https://github.com/konnectr/FableTime/compare/v0.3.1...v0.4.0) (2026-08-09)


### Features

* **billing:** hourly payment tracking and PDF invoices ([359b1c9](https://github.com/konnectr/FableTime/commit/359b1c969de0786a5c25d69064858303cf4f657f))

## [0.3.1] — 2026-07-19

### Fixed
- The running timer no longer keeps counting across a closed app or a powered-off
  computer. On a clean quit it stops at quit time; if the app is killed or the machine
  loses power, the entry is trimmed on next launch back to its last heartbeat (written
  every ~15s) instead of counting the whole dead gap. Adds a small `app_meta` table
  (migration v4) for the heartbeat.

## [0.3.0] — 2026-07-16

### Added
- **Projects: clickable cards, all-time totals, and a history detail.** Each project
  card now leads with its all-time tracked total (next to this-week and entry count),
  and the Projects tab header shows a grand all-time total ("всего отслежено").
  Clicking a card opens a project detail: a totals strip plus the full entry history
  grouped by day (most recent first). Each entry's note (comment) is shown there and
  editable inline, exactly as in the Tracker.

### Changed
- Extracted the Tracker's entry row into a shared `entry_row` component, now reused by
  the project detail so time entries render identically across the app.

## [0.2.1] — 2026-07-07

### Fixed
- Calendar: the week timeline now spans the full **00:00–24:00** day (matching the
  design) instead of a fixed 08:00–18:00 window, so time tracked outside working
  hours (evenings, early mornings) is no longer clipped out of view. The timeline
  body scrolls under a pinned day-of-week header, opens scrolled to ~07:00, and
  shades the night hours (00:00–07:00 and 21:00–24:00) grey.

## [0.2.0] — 2026-07-06

### Added
- **Notes.** Every time entry now carries a free-text detail note (what you actually
  did — details, links, results) separate from its short description, edited inline via
  a note button on the running row and on each of today's entries. Every project has a
  note too (repo, access, rate…), edited in its card, with clickable URLs shown with a
  github / external-link icon.
- **Lucide icons.** The app vendors its own Lucide SVGs and serves them through a
  composite asset source, so the UI is no longer limited to gpui-component's bundled
  subset. Refreshed nav and note glyphs (clock, layers, download, file-text, pencil).

### Changed
- Database migrated to **v3**: nullable `note` columns on `time_entries` and `projects`
  (existing data is preserved).

## [0.1.2] — 2026-06-27

### Added
- Tracker: edit a finished entry inline — description, start/end time, and project —
  with save / delete / cancel right in today's list.
- A reusable project picker dropdown (a chip that opens a list directly beneath it),
  used in both the Tracker editor and the Calendar form.
- An application icon. On macOS the app now ships as a `FableTime.app` bundle so it
  has a proper Dock/Finder icon; Windows embeds the icon in the executable.

### Infrastructure
- Windows code-signing step (SignPath, free for OSS) wired into CI, gated until the
  signing credentials are configured.

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

[Unreleased]: https://github.com/konnectr/FableTime/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/konnectr/FableTime/releases/tag/v0.3.1
[0.3.0]: https://github.com/konnectr/FableTime/releases/tag/v0.3.0
[0.2.1]: https://github.com/konnectr/FableTime/releases/tag/v0.2.1
[0.2.0]: https://github.com/konnectr/FableTime/releases/tag/v0.2.0
[0.1.2]: https://github.com/konnectr/FableTime/releases/tag/v0.1.2
[0.1.1]: https://github.com/konnectr/FableTime/releases/tag/v0.1.1
[0.1.0]: https://github.com/konnectr/FableTime/releases/tag/v0.1.0
