//! Mouse event routing: scroll, click, and drag handlers.

use crate::ui::pr_detail::DetailSection;

use super::state::App;
use super::types::Focus;

impl App {
    /// Route a raw mouse event to the focused panel.
    ///
    /// In Detail focus:
    /// - Wheel over sidebar → scroll the sidebar files list.
    /// - Wheel over right pane → scroll the active section.
    /// - Left click on a sidebar section row → select that section.
    /// - Left click on a sidebar file row → select the Files section and set
    ///   `pr_detail_files_cursor` (Phase 2 will use this to open the diff view).
    /// - Left click on the right pane → enter copy mode.
    /// - Drag on the right pane → extend the copy selection.
    pub(super) fn handle_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};

        match self.focus {
            Focus::Detail => {
                let (sections_rect, files_rect) = self.pr_detail_sidebar_rects.get();
                let right_rect = self.pr_detail_right_viewport.get();

                // Determine where the event landed.
                let in_sidebar = m.column < right_rect.x;

                match m.kind {
                    MouseEventKind::ScrollUp => {
                        if in_sidebar {
                            self.pr_detail_sidebar_scroll =
                                self.pr_detail_sidebar_scroll.saturating_sub(3);
                        } else {
                            let next = self.right_pane_scroll().saturating_sub(3);
                            *self.right_pane_scroll_mut() = next;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if in_sidebar {
                            self.pr_detail_sidebar_scroll =
                                self.pr_detail_sidebar_scroll.saturating_add(3);
                        } else {
                            let next = self.right_pane_scroll().saturating_add(3);
                            *self.right_pane_scroll_mut() = next;
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if in_sidebar {
                            self.handle_sidebar_click(m.column, m.row, sections_rect, files_rect);
                        } else if let Some((row, col)) = self.mouse_to_content_pos(m.column, m.row)
                        {
                            // A fresh left-click on the right pane enters copy mode.
                            self.copy_mode.enter(row, col);
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if !self.copy_mode.active || in_sidebar {
                            return;
                        }
                        if self.copy_mode.anchor.is_none() {
                            self.copy_mode.anchor = Some(self.copy_mode.cursor);
                        }
                        if let Some((row, col)) = self.mouse_to_content_pos(m.column, m.row) {
                            let lines = self.current_detail_lines();
                            let last_row = lines.len().saturating_sub(1);
                            self.copy_mode.cursor =
                                crate::ui::copy_mode::Pos { row: row.min(last_row), col };
                            self.ensure_cursor_visible(&lines);
                        }
                    }
                    _ => {}
                }
            }
            Focus::Dashboard => match m.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    self.move_dashboard_selection(-1);
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    self.move_dashboard_selection(1);
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Handle a left-click in the sidebar area.
    ///
    /// Clicks in the sections panel select that section row; clicks in the
    /// files panel jump to the Files section and set `pr_detail_files_cursor`.
    pub fn handle_sidebar_click(
        &mut self,
        _col: u16,
        row: u16,
        sections_rect: ratatui::layout::Rect,
        files_rect: ratatui::layout::Rect,
    ) {
        // Sections panel: row 0 is the "SECTIONS" header; rows 1–5 are sections.
        if row >= sections_rect.y && row < sections_rect.y.saturating_add(sections_rect.height) {
            let relative = (row - sections_rect.y) as usize;
            // relative == 0 is the header row — ignore it.
            if relative >= 1 {
                let sec_idx = relative - 1;
                if let Some(&sec) = DetailSection::ALL.get(sec_idx) {
                    self.pr_detail_selected_section = sec;
                    self.copy_mode.h_scroll = 0;
                }
            }
            return;
        }

        // Files panel: row 0 is the "FILES CHANGED" header; rows 1+ are files.
        if row >= files_rect.y && row < files_rect.y.saturating_add(files_rect.height) {
            let relative = (row - files_rect.y) as usize;
            // Subtract sidebar scroll offset and 1 for the header row.
            let header_offset = 1;
            let scroll_offset = usize::from(self.pr_detail_sidebar_scroll);
            if relative >= header_offset {
                let file_idx = relative - header_offset + scroll_offset;
                self.pr_detail_selected_section = DetailSection::Files;
                self.pr_detail_files_cursor = file_idx;
                // A sidebar file click is a drill-in gesture — open diff mode.
                self.pr_detail_files_show_diff = true;
                self.copy_mode.h_scroll = 0;
            }
        }
    }

    /// Map a (column, row) mouse position to a logical (row, col) position
    /// within the currently rendered detail lines. Returns `None` when the
    /// event is outside the right-pane viewport (including sidebar, status bar,
    /// or tab bar).
    ///
    /// The column mapping uses display cells, not characters: wide characters
    /// (CJK / emoji) will round to the nearest cell boundary.
    pub(super) fn mouse_to_content_pos(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let area = self.pr_detail_right_viewport.get();
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if row < area.y || row >= area.y.saturating_add(area.height) {
            return None;
        }
        if col < area.x || col >= area.x.saturating_add(area.width) {
            return None;
        }
        let scroll = self.scroll_for(self.pr_detail_selected_section);
        let content_row = scroll.saturating_add(row.saturating_sub(area.y));
        let content_col = self.copy_mode.h_scroll.saturating_add(col.saturating_sub(area.x));
        Some((usize::from(content_row), usize::from(content_col)))
    }

    /// Move the dashboard selection by `delta` rows, clamped to the current
    /// list length. Used by the mouse wheel handler.
    pub(super) fn move_dashboard_selection(&mut self, delta: i32) {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return;
        };
        let len = self.active_list_len();
        if len == 0 {
            return;
        }
        let max_idx = len.saturating_sub(1);
        let sel = self.selection.entry(repo).or_insert(0);
        let new = i64::try_from(*sel).unwrap_or(0).saturating_add(i64::from(delta));
        let clamped = new.clamp(0, i64::try_from(max_idx).unwrap_or(i64::MAX));
        *sel = usize::try_from(clamped).unwrap_or(0);
    }
}
