# SITEWRAP PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-02
**Commit:** 15dfd04
**Branch:** main

## OVERVIEW

Flatpak-delivered GTK4/libadwaita web app manager for Linux. Wraps arbitrary URLs as native desktop apps using CEF (Chromium Embedded Framework). Sandboxed via Flatpak with portal-based desktop integration.

## STRUCTURE

```
Sitewrap/
├── crates/
│   ├── sitewrap-cli/      # Binary entry point (clap CLI)
│   ├── sitewrap-app/      # GTK4 UI (manager + shell windows)
│   │   └── data/blueprints/  # .blp UI definitions
│   ├── sitewrap-model/    # Data models, persistence, XDG paths
│   ├── sitewrap-portal/   # ashpd portal integration (launchers, notifications)
│   ├── sitewrap-engine/   # CEF abstraction (currently stub)
│   └── sitewrap-icons/    # Favicon fetch + fallback generation
├── flatpak/               # Flatpak manifest + assets
├── SPEC.md                # Authoritative requirements (472 lines)
└── Cargo.toml             # Workspace root
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add CLI flags | `crates/sitewrap-cli/src/main.rs` | clap derive macros |
| Manager UI | `crates/sitewrap-app/src/manager.rs` | Uses Blueprint templates |
| Shell window | `crates/sitewrap-app/src/shell.rs` | Per-app browser window |
| Data models | `crates/sitewrap-model/src/lib.rs` | WebAppDefinition, permissions |
| Portal calls | `crates/sitewrap-portal/src/lib.rs` | DynamicLauncher, Notification |
| Icon fetching | `crates/sitewrap-icons/src/lib.rs` | Favicon + fallback initials |
| CEF integration | `crates/sitewrap-engine/` | Stub; see README.md for TODOs |
| UI templates | `crates/sitewrap-app/data/blueprints/` | .blp → compiled at build |
| Flatpak packaging | `flatpak/xyz.andriishafar.Sitewrap.yml` | GNOME 47, rust-stable |
| Requirements | `SPEC.md` | Full spec, acceptance criteria |

## CRATE DEPENDENCY GRAPH

```
sitewrap-cli
    └── sitewrap-app
            ├── sitewrap-model    (data types, XDG paths)
            ├── sitewrap-engine   (web view abstraction)
            ├── sitewrap-portal   (desktop integration)
            └── sitewrap-icons    (favicon handling)
```

No circular dependencies. Leaf crates have no internal deps.

## CONVENTIONS

### Rust
- Error handling: `anyhow::Result` + `thiserror` for custom errors
- **AVOID** `unwrap()` in production; use `?` or `.context()`
- Workspace deps in root `Cargo.toml`, crates use `workspace = true`
- Edition 2021, resolver 2

### GTK4/libadwaita
- UI defined in Blueprint (`.blp`), compiled via `build.rs`
- Resources bundled as GResource, registered at startup
- Adwaita widgets: `AdwApplicationWindow`, `AdwHeaderBar`, `AdwPreferencesWindow`
- Follow GNOME HIG

### Portal Integration
- **MUST** use portals for host integration (DynamicLauncher, Notification, OpenURI)
- **NEVER** write directly to `~/.local/share` from sandboxed app
- Check `is_supported()` before portal calls; graceful fallback

### Data Storage (XDG paths inside sandbox)
- App definitions: `$XDG_CONFIG_HOME/sitewrap/apps/{uuid}.toml`
- Permissions: `$XDG_CONFIG_HOME/sitewrap/permissions/{uuid}.toml`
- Icons cache: `$XDG_CACHE_HOME/sitewrap/icons/`
- CEF profiles: `$XDG_DATA_HOME/sitewrap/profiles/{uuid}/`

## ANTI-PATTERNS (THIS PROJECT)

- **NO** `unwrap()` outside tests
- **NO** direct filesystem writes to host (use portals)
- **NO** `--filesystem=home` in Flatpak manifest
- **NO** suppressing type errors (`as any`, `@ts-ignore` - N/A for Rust)
- **NO** broad permissions in Flatpak beyond what's needed

## CEF STATUS (NOT YET IMPLEMENTED)

Engine crate has stub backends. When CEF is wired:
- Detect `libcef.so` via `SITEWRAP_CEF_ROOT` or `CEF_ROOT`
- Initialize with `windowless_rendering_enabled=true`
- Hook `engine::tick()` to pump CEF message loop
- Create OSR browser, render to GTK texture

See: `crates/sitewrap-engine/README.md` for detailed TODO list.

## COMMANDS

```bash
# Development
cargo build                    # Build all crates
cargo test                     # Run tests (sitewrap-model, sitewrap-portal)
cargo clippy                   # Lint check

# Run locally (requires GTK4 installed)
cargo run -- --manager         # Manager mode
cargo run -- --shell <uuid>    # Shell mode for specific app

# To regenerate deps after Cargo.lock changes:
uv run flatpak/flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json

# To build:
flatpak-builder --user --install build flatpak/xyz.andriishafar.Sitewrap.yml
```

## TESTS

- `sitewrap-model`: Unit tests in `lib.rs` (registry, permissions, URL normalization)
- `sitewrap-portal`: Unit + integration tests in `tests/` (launcher descriptor, open_uri)
- Other crates: No tests yet (early stage)

Run: `cargo test -p sitewrap-model -p sitewrap-portal`

## NOTES

- Project is early stage
- SPEC.md is the source of truth for requirements
- CEF integration is the main unfinished work
- Blueprint files require `blueprint-compiler` at build time
- Flatpak manifest has CEF placeholders; replace with real CEF bundle when ready
