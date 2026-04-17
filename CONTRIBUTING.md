# Contributing to octopeek

Thank you for your interest in contributing! This guide covers the development
workflow, coding standards, and the pull request process.

## Building

```sh
# Prerequisites: Rust 1.85+ (see rust-toolchain.toml)
git clone https://github.com/leboiko/octopeek
cd octopeek
cargo build
```

## Running

```sh
# Debug build (faster compile, slower runtime)
cargo run

# Release build
cargo run --release
```

## Testing

```sh
cargo test --all-features
```

## Linting and Formatting

All CI checks must pass before a PR is merged. Run them locally first:

```sh
# Check formatting (does not modify files)
cargo fmt --all -- --check

# Apply formatting
cargo fmt --all

# Run clippy with all warnings as errors
cargo clippy --all-targets --all-features -- -D warnings
```

The project enforces `clippy::pedantic` and additional targeted lints via
`Cargo.toml [lints]`. New `#[allow(...)]` suppressions require a comment
explaining why the lint is inappropriate at that specific site.

## PR Checklist

Before opening a pull request:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --all-features` passes
- [ ] New public items have doc comments (`///`)
- [ ] `CHANGELOG.md` has an entry under `## [Unreleased]`
- [ ] No hardcoded credentials or secrets
- [ ] No debug `println!` or `dbg!` macros in committed code

## Commit Message Convention

This project uses [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <short description>

[optional body]

[optional footer(s)]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`, `ci`.

Examples:
```
feat(dashboard): add PR inbox list with role glyphs
fix(tab_bar): correct overflow indicator width on narrow terminals
docs: update keybindings table in README
```

## Issue Triage

- **Bug reports**: must include environment details (OS, terminal, version). Use
  the bug report template.
- **Feature requests**: describe the problem you're trying to solve, not just
  the solution. Use the feature request template.
- Duplicate issues are closed with a link to the original.
- Issues idle for 90 days without activity are closed with a `stale` label.

## Developer Certificate of Origin (DCO)

By submitting a contribution, you certify that:

1. The contribution was created in whole or in part by you, and you have the
   right to submit it under the open source license indicated in the file; or
2. The contribution is based upon previous work that, to the best of your
   knowledge, is covered under an appropriate open source license; or
3. The contribution was provided directly to you by some other person who
   certified (1), (2) or (3) and you have not modified it.

You understand and agree that this project and the contribution are public, and
that a record of the contribution (including all personal information you submit
with it) is maintained indefinitely and may be redistributed consistent with
this project or the open source license(s) involved.

Include `Signed-off-by: Your Name <email@example.com>` in each commit, or use
`git commit -s` to add it automatically.

## Code of Conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md).
Please read it before participating in the community.
