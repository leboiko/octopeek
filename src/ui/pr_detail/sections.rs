//! Per-section content builders and the `build_section` dispatcher.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ratatui::text::Line;

use crate::github::detail::PrDetail;
use crate::theme::Palette;
use crate::ui::markdown::render_markdown;

use super::DetailSection;
use super::ThreadIndex;
use super::checks::checks_lines;
use super::comments::build_comments;
use super::commits::build_commits;
use super::files::build_files;
use super::reviews::reviews_lines;

/// Build lines for the Description section.
///
/// Returns `(lines, alt_bg_ranges)` — ranges are always empty here (no tinting).
pub(super) fn build_description(
    detail: &PrDetail,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    let mut lines = Vec::new();
    if !detail.body_markdown.is_empty() {
        lines.extend(render_markdown(&detail.body_markdown, p));
        lines.push(Line::from(""));
    }
    (lines, Vec::new())
}

/// Build lines for the Checks section.
///
/// Returns `(lines, alt_bg_ranges)` — ranges are always empty here.
pub(super) fn build_checks(
    detail: &PrDetail,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    if detail.check_runs.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut lines = checks_lines(detail, p);
    lines.push(Line::from(""));
    (lines, Vec::new())
}

/// Build lines for the Reviews section.
///
/// Returns `(lines, alt_bg_ranges)` — ranges are always empty here.
pub(super) fn build_reviews(
    detail: &PrDetail,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    if detail.reviews.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut lines = reviews_lines(detail, p);
    lines.push(Line::from(""));
    (lines, Vec::new())
}

/// Dispatch to the per-section builder for the given [`DetailSection`].
///
/// The second tuple element is the alt-bg range list; non-empty only for
/// [`DetailSection::Comments`].
///
/// # Arguments
///
/// * `section` - Which section to render.
/// * `detail` - The loaded PR detail.
/// * `files_cursor` - Index of the highlighted file in the Files section.
/// * `files_show_diff` - When `true` render the diff; `false` renders the overview.
/// * `comments_expanded` - Whether comments are expanded in the Comments section.
/// * `comments_show_outdated` - Whether outdated review threads are shown
///   (visible-but-muted under the `OUTDATED` divider) or collapsed behind
///   a disclosure line. Bound to `App::detail_show_outdated`.
/// * `thread_index` - Optional index for per-line thread lookups in the Files diff.
/// * `expanded_threads` - Set of `(path, lineno)` anchors expanded by the user.
/// * `diff_cursor` - Written by the Files renderer to track the last thread anchor.
/// * `scoped_patches` - When `Some`, restricts the Files section to this per-commit
///   patch map (commit-scope mode). `None` = cumulative HEAD view.
/// * `commits_cursor` - Index of the highlighted row in the Commits list (for
///   the `▶` indicator). Only used when `section == Commits`.
/// * `comments_scope_sha` - When `Some`, restricts the Comments section to threads
///   that originated on the given commit SHA. Issue comments are always shown.
///   Derived from `App::selected_commit` at the call site.
/// * `p` - Current colour palette.
/// * `ascii` - Use ASCII glyphs instead of Unicode box-drawing.
//
// build_section orchestrates every section renderer, so it naturally has
// many orthogonal inputs. A dedicated options struct is cleaner but would
// ripple through the call chain for minor ergonomic gain; the allows make
// the tradeoff explicit.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn build_section(
    section: DetailSection,
    detail: &PrDetail,
    files_cursor: usize,
    files_show_diff: bool,
    comments_expanded: bool,
    comments_show_outdated: bool,
    thread_index: Option<&ThreadIndex>,
    expanded_threads: &HashSet<(String, u32)>,
    diff_cursor: &RefCell<Option<(String, u32)>>,
    scoped_patches: Option<&HashMap<String, Option<String>>>,
    commits_cursor: usize,
    comments_scope_sha: Option<&str>,
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    match section {
        DetailSection::Description => build_description(detail, p),
        DetailSection::Checks => build_checks(detail, p),
        DetailSection::Reviews => build_reviews(detail, p),
        DetailSection::Files => {
            // When scoped: thread expansion is disabled (scoped diffs show
            // per-commit patches without HEAD-view thread anchors). Use a
            // local empty set so its lifetime covers the build_files call.
            let empty_expanded = HashSet::new();
            let effective_expanded =
                if scoped_patches.is_some() { &empty_expanded } else { expanded_threads };
            build_files(
                detail,
                files_cursor,
                files_show_diff,
                thread_index,
                effective_expanded,
                // Don't update the diff cursor while scoped — the `t`
                // key handler is a no-op in scoped mode (keymap guard).
                diff_cursor,
                scoped_patches,
                p,
                ascii,
            )
        }
        DetailSection::Comments => build_comments(
            detail,
            comments_expanded,
            comments_show_outdated,
            comments_scope_sha,
            p,
            ascii,
        ),
        DetailSection::Commits => build_commits(detail, p, Some(commits_cursor)),
    }
}

/// Count rows for sections that have a cheap non-rendering count path.
///
/// Returns `None` for prose sections where the wrapped row count depends on
/// ratatui's paragraph layout. The Files section is non-wrapping, so its row
/// count can be computed directly from patch text without allocating styled
/// lines.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn cheap_section_row_count(
    section: DetailSection,
    detail: &PrDetail,
    files_cursor: usize,
    files_show_diff: bool,
    thread_index: Option<&ThreadIndex>,
    expanded_threads: &HashSet<(String, u32)>,
    scoped_patches: Option<&HashMap<String, Option<String>>>,
) -> Option<usize> {
    match section {
        DetailSection::Files => Some(super::files::files_row_count(
            detail,
            files_cursor,
            files_show_diff,
            thread_index,
            expanded_threads,
            scoped_patches,
        )),
        DetailSection::Commits => {
            if detail.commits.is_empty() {
                Some(0)
            } else {
                Some(detail.commits.len() + 2 + usize::from(detail.commits.len() >= 100) * 2)
            }
        }
        DetailSection::Description
        | DetailSection::Checks
        | DetailSection::Reviews
        | DetailSection::Comments => None,
    }
}
