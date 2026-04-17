# octopeek

[![CI](https://github.com/leboiko/octopeek/actions/workflows/ci.yml/badge.svg)](https://github.com/leboiko/octopeek/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/leboiko/octopeek#license)
<!-- Crates.io badge will be enabled once the first release is published. -->
<!-- [![crates.io](https://img.shields.io/crates/v/octopeek.svg)](https://crates.io/crates/octopeek) -->

> **Status:** Phase 5 complete. All core interactive features are implemented. Phase 6 (release binaries) is next.

**A fast, keyboard-driven TUI for your GitHub PR and issue inbox.**

octopeek is a terminal application that gives you a unified, keyboard-navigable
view of every pull request and issue that needs your attention across multiple
repositories — whether you're the author, a requested reviewer, or an assignee.
It stays out of your way when you're in flow and puts all the context you need
at your fingertips when you're doing triage.

## Screenshot

> Screenshot coming in Phase 2 once the data layer populates the dashboard.

## Features

### Phase 1 — Scaffold
- [x] Keyboard-driven event loop with panic-safe terminal teardown
- [x] 8 built-in color themes (Default, Dracula, Solarized Dark/Light, Nord, Gruvbox Dark/Light, GitHub Light)
- [x] Multi-repository tab bar with overflow indicators
- [x] XDG-compliant config and session persistence
- [x] Auto-refresh timer infrastructure (configurable, default disabled)
- [x] Full keybinding help overlay (`?`)
- [x] Dual MIT/Apache-2.0 license, CI, issue templates

### Phase 2 — GitHub data layer
- [x] GitHub GraphQL client (`GITHUB_TOKEN` / `gh auth login`)
- [x] PR/issue inbox: author, reviewer, assignee roles with `A`, `R`, `@` glyphs

### Phase 3 — Dashboard UI
- [x] Scrollable PR/issue list with role-aware sorting and CI status column
- [x] Per-repo toggle between PR and Issue view (`i`)

### Phase 4 — Detail view & actions
- [x] Full PR/issue detail rendered from markdown (pulldown-cmark + syntect)
- [x] Open in browser (`o`), copy URL (`y`)

### Phase 5 — Checkout & repo management
- [x] Repo picker overlay (`p`): add/remove watched repos, persisted to config
- [x] Branch checkout with confirmation overlay (`c`)

### Phase 6 — Distribution (coming next)
- [ ] Pre-built release binaries via GitHub Actions
- [ ] Crates.io publication

## Install

No crates.io release yet. Install from git:

```sh
cargo install --git https://github.com/leboiko/octopeek
```

## Quick Start

1. Authenticate with GitHub:
   ```sh
   gh auth login
   # or:
   export GITHUB_TOKEN=ghp_...
   ```

2. Create a config file at the platform-specific path:

   | OS      | Config path                                                  |
   | ------- | ------------------------------------------------------------ |
   | Linux   | `~/.config/octopeek/config.toml` (or `$XDG_CONFIG_HOME/octopeek/`) |
   | macOS   | `~/Library/Application Support/octopeek/config.toml`         |
   | Windows | `%APPDATA%\octopeek\config.toml`                             |

   Example (macOS):
   ```sh
   mkdir -p ~/Library/Application\ Support/octopeek
   cat > ~/Library/Application\ Support/octopeek/config.toml << 'EOF'
   theme = "default"
   repos = [
     "rust-lang/rust",
     "tokio-rs/tokio",
   ]
   # auto_refresh_seconds = 60  # uncomment to enable background refresh
   EOF
   ```

   Don't remember the path? Add repos interactively with the `p` keybinding — the picker writes to the correct location for you.

3. Launch:
   ```sh
   octopeek
   ```

## Configuration

Location varies by platform (see Quick Start). On Linux:

`~/.config/octopeek/config.toml`:

```toml
# Color theme. Options:
#   default, dracula, solarized_dark, solarized_light,
#   nord, gruvbox_dark, gruvbox_light, github_light
theme = "default"

# Repositories to watch, in owner/name format.
repos = [
  "rust-lang/rust",
  "tokio-rs/tokio",
  "ratatui-org/ratatui",
]

# Background refresh interval in seconds.
# Remove or set to null to disable (manual refresh with r/R).
auto_refresh_seconds = 120

# Use plain ASCII box-drawing characters instead of Unicode.
# Useful for terminals with limited glyph support.
show_ascii_glyphs = false
```

## Requirements

- **`gh` CLI** (recommended) or a `GITHUB_TOKEN` environment variable — required for GitHub API access.
- **`git`** on `$PATH` — required for the branch checkout feature (`c`).
- A terminal with **256-color support** (most modern terminals qualify).

## Quick demo

No GitHub token?  Use the debug dump command to inspect raw data without launching the TUI:

```sh
# Print full inbox as JSON (useful for CI/scripts):
octopeek debug dump

# Print full PR detail:
octopeek debug dump pr rust-lang/rust 12345

# Print full issue detail:
octopeek debug dump issue rust-lang/rust 12345
```

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` | Quit |
| `?` | Toggle help overlay |
| `Tab` / `Shift+Tab` | Next / previous tab |
| `1`–`9` | Jump to tab N |

### Dashboard

| Key | Action |
|-----|--------|
| `j` / `Down` | Move cursor down |
| `k` / `Up` | Move cursor up |
| `g` `g` | Jump to top of list |
| `G` | Jump to bottom of list |
| `Enter` | Open PR / issue detail |
| `i` | Toggle PR / Issue view |
| `r` | Refresh current tab |
| `R` | Refresh all tabs |
| `o` | Open selected item in browser |
| `y` | Copy URL to clipboard |
| `c` | Checkout PR head branch (with confirmation) |
| `p` | Open repo picker overlay |

### Detail view

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `d` / `u` | Page down / up (10 lines) |
| `g` `g` | Scroll to top |
| `G` | Scroll to bottom |
| `Tab` / `Shift+Tab` | Jump to next / previous section |
| `n` / `N` | Next / previous unresolved review thread |
| `f` | Toggle files section expanded/collapsed |
| `m` | Toggle comments section expanded/collapsed |
| `o` | Open in browser |
| `y` | Copy URL to clipboard |
| `c` | Checkout PR head branch (with confirmation) |
| `r` | Refresh detail |
| `Esc` / `b` | Return to dashboard |

### Repo picker overlay (`p`)

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate list |
| `Enter` | Focus selected repo's tab and close picker |
| `d` / `Backspace` | Delete selected repo |
| `a` / `i` | Enter input mode to add a repo |
| `Esc` | Close picker (List mode) / return to list (Input mode) |

### Confirmation overlay (`c`)

| Key | Action |
|-----|--------|
| `y` | Confirm action |
| `n` / `N` / `Esc` | Cancel |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project shall be dual-licensed as above, without any
additional terms or conditions.
