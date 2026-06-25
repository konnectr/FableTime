# CLAUDE.md

Guidance for Claude Code (and other agents) working in this repository.

## What this is

A cross-platform desktop **time tracker** in Rust on **GPUI** (Zed's GPU UI framework),
with local SQLite storage. Single codebase → Windows / macOS / Linux. Its UI matches an
approved design (see `src/palette.rs` and the design-source memory). Four tabs:

1. **Трекер (Tracker)** — a work bar (description + project picker + live clock + Start/Stop)
   and today's entries with a running highlight and replay.
2. **Календарь (Calendar)** — a Mon–Sun week timeline of project-colored blocks, with an
   inline add/edit/delete form.
3. **Проекты (Projects)** — project cards (client, weekly total, entry count, Mon–Sun
   sparkline) + create.
4. **Экспорт (Export)** — period + project filter, CSV / JSON / Markdown, folder export.

## Build & run

> [!IMPORTANT]
> **macOS:** GPUI's `gpui_macos` backend compiles Metal shaders at build time. macOS 26 /
> Xcode 26 ship `metal` as a stub — install the real component once with
> `xcodebuild -downloadComponent MetalToolchain` (~688 MB, no sudo), then build with
> `DEVELOPER_DIR` pointed at Xcode (system `xcode-select` may stay on CommandLineTools):
> ```
> DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo build
> ```

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo run            # debug
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo build --release
```

Linux/Windows need no `DEVELOPER_DIR`. Linux needs graphics/font dev packages — see
`.github/workflows/build.yml`.

**Test overrides (env):** `TIMETRACKER_DB=<path>` points at a throwaway DB (keeps the real
one untouched); `TIMETRACKER_TAB=tracker|calendar|projects|export` sets the initial tab —
used to smoke each render path headlessly.

## ⚠️ The #1 footgun: gpui ↔ gpui-component rev unification

`gpui` / `gpui_platform` **must** resolve to the *exact* git rev that the chosen
`gpui-component` rev depends on, or you get type-mismatch compile errors. Current pins
(`Cargo.toml`): gpui/gpui_platform → `1d217ee3…`, gpui-component(-assets) → `a0ae3a37…`.
**To bump:** read the new gpui-component rev's `Cargo.toml` for the gpui rev it pins, and
match. Never bump them independently. The checked-out component source (authoritative for
APIs — pre-1.0, churns) is at `~/.cargo/git/checkouts/gpui-component-*/<rev>/crates/ui/src/`.

## Architecture

```
src/
  main.rs        bootstrap: application().with_assets → gpui_component::init → window → Root
  palette.rs     design colors as u32 0xRRGGBB + per-project palette (gpui-free)
  models.rs      row structs + pure time helpers (UTC storage, local day/week grouping)
  db.rs          SQLite: user_version migrations (v1→v2), CRUD, day/week totals, project
                 stats, range export
  app.rs         AppState entity: owns Connection + running-entry snapshot + 1s timer Task
  exporter.rs    pure (no-gpui) CSV/JSON/Markdown serialization + per-project/per-day totals
  ui/
    root.rs      38px top bar + 236px sidebar (4 tabs + Today card) + scrolling panel
    common.rs    small shared helpers (dot)
    tracker.rs   work bar + today's entries + replay
    calendar.rs  Mon–Sun week timeline (absolute-positioned blocks) + add/edit/delete form
    projects.rs  project cards + sparkline + create
    export.rs    period/project chips + format cards + rfd folder export
```

### Conventions

- **Data model:** an entry belongs directly to a **project** and carries a free-text
  **description** — there is no task layer. (Migration v2 in `db.rs` switched from the
  original project→task→entry schema.)
- **Timestamps** stored as **UTC RFC3339** (`…Z`) so lexicographic == chronological → range
  queries use plain `<`/`>=`. Convert to local only for display / grouping (`models.rs`).
- **One running entry**: `Db::start_entry` stops any open entry first. A manual entry and a
  live timer are the same row (manual just sets `end_ts` too).
- **DB threading**: the `rusqlite::Connection` lives on the main thread inside `AppState`;
  never move it across threads. Export file-writing is offloaded to `cx.background_executor()`.
- **Colors:** views use `gpui::rgb(palette::SOMETHING)` and `palette::pal_for_hex(...)`; the
  pure `palette.rs` is the single source of truth. Pure layers (`palette`, `models`, `db`,
  `exporter`) are gpui-free so they unit-test without building gpui.

### GPUI idioms (this rev)

- View: `impl Render { fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement }`.
- `use gpui::AppContext;` to call `cx.new(...)`; `cx.observe(&e, …).detach()` to re-render on
  shared-state change; `cx.subscribe(&state, |this, _, ev, cx| …)` for component events.
- Handlers: `el.on_click(cx.listener(|this, _ev, window, cx| …))` (element needs `.id(...)`).
- A scrollable div needs `.id(...)` before `.overflow_y_scroll()`.
- Async sleep: `cx.background_executor().timer(d).await` (not `gpui::Timer`).
- Icons: `gpui_component::{Icon, IconName, Sizable}` — `Icon::new(IconName::Calendar).small()`
  (names are generated from the bundled SVG set). `Button::disabled` needs `Disableable`.

## Verifying changes

Pure layers have throwaway standalone harnesses (`#[path]`-include the real source, assert) —
the fastest way to check DB/export logic without building gpui. For UI, build then smoke-run
the binary (optionally `TIMETRACKER_DB=<seeded>` + `TIMETRACKER_TAB=<tab>`); a process that
stays alive with no panic clears that render path. Incremental rebuilds are seconds (the
~508-crate dep tree incl. gpui is cached after the first build).
