//! Per-section content builders and the `build_section` dispatcher.

use ratatui::text::Line;

use crate::github::detail::PrDetail;
use crate::theme::Palette;
use crate::ui::markdown::render_markdown;

use super::DetailSection;
use super::checks::checks_lines;
use super::comments::build_comments;
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
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    match section {
        DetailSection::Description => build_description(detail, p),
        DetailSection::Checks => build_checks(detail, p),
        DetailSection::Reviews => build_reviews(detail, p),
        DetailSection::Files => build_files(detail, files_cursor, files_show_diff, p),
        DetailSection::Comments => {
            build_comments(detail, comments_expanded, comments_show_outdated, p, ascii)
        }
    }
}
