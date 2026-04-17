# octopeek

[![CI](https://github.com/leboiko/octopeek/actions/workflows/ci.yml/badge.svg)](https://github.com/leboiko/octopeek/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/leboiko/octopeek#license)
[![crates.io](https://img.shields.io/crates/v/octopeek.svg)](https://crates.io/crates/octopeek)

**A fast, keyboard-driven TUI for your GitHub PR and issue inbox.**

octopeek is a terminal application that gives you a unified, keyboard-navigable
view of every pull request and issue that needs your attention across multiple
repositories — whether you're the author, a requested reviewer, or an assignee.
It stays out of your way when you're in flow and puts all the context you need
at your fingertips when you're doing triage.

## Screenshot

![octopeek screenshot](docs/screenshot.png)

> Screenshot coming soon — Phase 2 data layer will populate the dashboard.

## Features

### Phase 1 (current — scaffold)
- [x] Keyboard-driven event loop with panic-safe terminal teardown
- [x] 8 built-in color themes (Default, Dracula, Solarized Dark/Light, Nord, Gruvbox Dark/Light, GitHub Light)
- [x] Multi-repository tab bar with overflow indicators
- [x] XDG-compliant config and session persistence
- [x] Auto-refresh timer infrastructure (configurable, default disabled)
- [x] Full keybinding help overlay (`?`)
- [x] Dual MIT/Apache-2.0 license, CI, issue templates

### Phase 2 (GitHub data layer — coming next)
- [ ] GitHub GraphQL client (`GITHUB_TOKEN` / `gh auth login`)
- [ ] PR/issue inbox: author, reviewer, assignee roles with `A`, `R`, `@` glyphs
- [ ] Rate-limit tracking and exponential backoff

### Phase 3 (dashboard UI)
- [ ] Scrollable PR/issue list with role-aware sorting
- [ ] Per-repo toggle between PR and Issue view (`i`)
- [ ] Repo picker overlay (`p`)

### Phase 4 (detail view & actions)
- [ ] Full PR/issue detail rendered from markdown (pulldown-cmark + syntect)
- [ ] Open in browser (`o`), copy URL (`y`)

### Phase 5 (checkout & advanced)
- [ ] Branch checkout with confirmation overlay (`c`)
- [ ] Mouse support

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

2. Create a config file:
   ```sh
   mkdir -p ~/.config/octopeek
   cat > ~/.config/octopeek/config.toml << 'EOF'
   theme = "default"
   repos = [
     "rust-lang/rust",
     "tokio-rs/tokio",
   ]
   # auto_refresh_seconds = 60  # uncomment to enable background refresh
   EOF
   ```

3. Launch:
   ```sh
   octopeek
   ```

## Configuration

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

## Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `?` | Toggle help overlay |
| `Tab` / `Shift+Tab` | Next / previous tab |
| `1`–`9` | Jump to tab N |
| `j` / `Down` | Move cursor down (Phase 3) |
| `k` / `Up` | Move cursor up (Phase 3) |
| `g` | Jump to top of list (Phase 3) |
| `G` | Jump to bottom of list (Phase 3) |
| `Enter` | Open detail view (Phase 3) |
| `Esc` | Return to dashboard (Phase 3) |
| `i` | Toggle PR / Issue view (Phase 3) |
| `r` | Refresh current tab (Phase 2) |
| `R` | Refresh all tabs (Phase 2) |
| `n` / `N` | Next / previous match (Phase 3) |
| `f` | Filter / find (Phase 3) |
| `o` | Open in browser (Phase 4) |
| `y` | Copy URL to clipboard (Phase 4) |
| `c` | Checkout PR branch (Phase 5) |
| `p` | Open repo picker (Phase 3) |

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
