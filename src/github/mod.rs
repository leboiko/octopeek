//! GitHub data layer.
//!
//! Phase 2: GitHub GraphQL client goes here. Will include:
//! - Token authentication via `GITHUB_TOKEN` env var or `gh auth token`.
//! - GraphQL queries for PRs where the user is author, reviewer, or assignee.
//! - Issue queries for configured repositories.
//! - Rate-limit tracking and exponential backoff.
