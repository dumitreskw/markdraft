use std::sync::Mutex;

use crate::events::OpenRequest;

/// Shared app state. Workspace roots and the file index join it in M1.
pub struct AppState {
    /// Path given at launch, held until the UI is ready to receive it.
    pending_open: Mutex<Option<OpenRequest>>,
}

impl AppState {
    pub fn new(pending_open: Option<OpenRequest>) -> Self {
        Self {
            pending_open: Mutex::new(pending_open),
        }
    }

    pub fn take_pending_open(&self) -> Option<OpenRequest> {
        self.pending_open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}
