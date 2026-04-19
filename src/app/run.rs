//! [`App::run`] — the async event loop entry point and shutdown path.

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::actions::Action;
use super::state::App;

impl App {
    /// Run the application: spawn the event thread, start the auto-refresh
    /// timer if configured, then loop drawing frames and processing actions
    /// until `running` is false.
    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        let (mut events, tx) = crate::event::EventHandler::new();
        self.action_tx = Some(tx.clone());

        // Kick off an initial inbox fetch if a client is available and we have
        // no cached data yet.
        if self.client.is_some() && self.inbox.is_none() {
            self.spawn_fetch(tx.clone());
        }

        // If auto-refresh is configured, spawn a background task that emits
        // `Action::RefreshAll` on the given cadence. The handle is stored on
        // `App` so it is cancelled automatically when `App` is dropped.
        if let Some(secs) = self.config.auto_refresh_seconds {
            let refresh_tx = tx.clone();
            let interval = tokio::time::Duration::from_secs(u64::from(secs));
            self.refresh_handle = Some(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                // Skip the first (immediate) tick so we don't refresh before
                // any data has been loaded.
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if refresh_tx.send(Action::AutoRefresh).is_err() {
                        // Receiver dropped — app is shutting down.
                        break;
                    }
                }
            }));
        }

        while self.running {
            // Draw the current frame.
            terminal.draw(|f| crate::ui::draw(f, &self))?;

            // Wait for the next action from either the input thread or a
            // background task (e.g. the auto-refresh timer).
            let Some(action) = events.next().await else {
                // All senders dropped — shut down.
                break;
            };

            self.handle_action(action);
        }

        // Abort the auto-refresh task explicitly: `JoinHandle::drop` only
        // detaches in tokio, it does not cancel.
        if let Some(handle) = self.refresh_handle.take() {
            handle.abort();
        }

        // Persist the session so the active tab index, sidebar width, and
        // sidebar visibility all survive a relaunch.
        self.session.active_tab_index = self.tabs.active_index().unwrap_or(0);
        self.session.sidebar_width = self.sidebar_width;
        self.session.sidebar_hidden = self.sidebar_hidden;
        self.session.save();

        Ok(())
    }
}
