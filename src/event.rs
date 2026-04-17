//! Terminal event loop: polls crossterm on a blocking OS thread and forwards
//! actions to the async application via an unbounded channel.

use crate::app::actions::Action;
use crossterm::event::{self, Event};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::warn;

/// Drives the terminal event loop and forwards actions to the application.
///
/// `EventHandler` owns the receiving end of an unbounded channel. A background
/// OS thread polls crossterm for keyboard and resize events and sends them as
/// [`Action`] values into the channel.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Action>,
    /// Shared flag to signal the background thread to stop polling.
    stop: Arc<AtomicBool>,
    /// Join handle for the background polling thread. Taken and joined in
    /// `Drop` so raw-mode and mouse-capture handles are released cleanly.
    thread: Option<JoinHandle<()>>,
}

impl EventHandler {
    /// Create a new handler and spawn the background input-polling thread.
    ///
    /// # Returns
    ///
    /// A tuple of `(EventHandler, UnboundedSender<Action>)`. The sender is
    /// shared with the rest of the application so that non-input sources (e.g.
    /// the auto-refresh timer) can also inject actions into the event loop.
    pub fn new() -> (Self, mpsc::UnboundedSender<Action>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        // Use a real OS thread — crossterm::event::poll is blocking I/O and
        // must not run on the tokio async executor, which would block the
        // entire single-threaded runtime.
        let thread = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                // Poll with a short timeout so the stop flag is checked often.
                if event::poll(Duration::from_millis(50)).unwrap_or(false)
                    && let Ok(evt) = event::read()
                {
                    let action = match evt {
                        // Only handle key-down events. On Windows, crossterm
                        // emits both Press and Release for every keystroke;
                        // forwarding both would duplicate every action.
                        Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                            Some(Action::RawKey(key))
                        }
                        Event::Resize(w, h) => Some(Action::Resize(w, h)),
                        Event::Mouse(m) => Some(Action::Mouse(m)),
                        _ => None,
                    };
                    if let Some(action) = action
                        && event_tx.send(action).is_err()
                    {
                        // Receiver was dropped — the app is shutting down.
                        break;
                    }
                }
            }
        });

        (Self { rx, stop, thread: Some(thread) }, tx)
    }

    /// Wait for the next action from any source.
    ///
    /// Returns `None` when every sender has been dropped and the channel is
    /// empty, which signals that the application should shut down.
    pub async fn next(&mut self) -> Option<Action> {
        self.rx.recv().await
    }
}

impl Drop for EventHandler {
    /// Signal the background thread to stop and join it so OS-level terminal
    /// handles (raw mode, mouse capture) are released before `TerminalGuard`
    /// tears the terminal down.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take()
            && let Err(e) = handle.join()
        {
            warn!("input thread panicked during shutdown: {e:?}");
        }
    }
}
