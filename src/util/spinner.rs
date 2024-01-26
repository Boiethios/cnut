use std::time::Duration;

use indicatif::ProgressBar;

pub struct Spinner {
    message: String,
    bar: ProgressBar,
}

impl Spinner {
    /// Creates a new spinner and displays it.
    pub fn create(message: impl Into<String>) -> Self {
        let message = message.into();
        let bar = ProgressBar::new_spinner().with_message(format!("{message}…"));

        bar.enable_steady_tick(Duration::from_millis(300));

        Self { message, bar }
    }

    /// Finishes the spinner with an “OK” message.
    pub fn success(&self) {
        self.bar.finish_with_message(format!("{} OK", self.message));
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if self.bar.is_finished() == false {
            self.bar
                .finish_with_message(format!("{} ERROR", self.message))
        }
    }
}
