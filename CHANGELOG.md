# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial project scaffold.
- Async event loop built on `tokio` + a blocking `crossterm` polling thread.
- RAII `TerminalGuard` that restores the terminal on exit, error, and panic.
- Palette-based theme system with eight built-in themes: Default, Dracula, Solarized Dark, Solarized Light, Nord, Gruvbox Dark, Gruvbox Light, GitHub Light.
- Tab bar with stable `TabId`s, repo deduplication, and a `MAX_TABS` cap.
- XDG-based configuration (`~/.config/octopeek/config.toml`) and session persistence with graceful fallback on parse errors.
- Full `Action` enum and key-binding dispatcher covering every Phase 2–5 interaction (unimplemented variants log at `warn` level).
- Help overlay (`?`) listing every key binding.
- `clap`-powered CLI with `--version` and `--help`.
- CI pipeline (fmt, clippy, test) on Linux and macOS, stable and beta.
- Release workflow skeleton driven by git tags.
- OSS project hygiene: dual MIT/Apache-2.0 licensing, Contributor Covenant, security policy, contributing guide, issue and PR templates, CODEOWNERS, Dependabot.

<!-- Comparison links will be populated once the first version is tagged. -->

