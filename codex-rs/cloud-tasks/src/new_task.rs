use codex_tui::ComposerInput;
use codex_tui::i18n::message;

pub struct NewTaskPage {
    pub composer: ComposerInput,
    pub submitting: bool,
    pub env_id: Option<String>,
    pub best_of_n: usize,
}

impl NewTaskPage {
    pub fn new(env_id: Option<String>, best_of_n: usize) -> Self {
        let mut composer = ComposerInput::new();
        composer.set_hint_items(vec![
            ("⏎", message("cloud-composer-send", &[])),
            ("Shift+⏎", message("cloud-composer-newline", &[])),
            ("Ctrl+O", message("cloud-composer-env", &[])),
            ("Ctrl+N", message("cloud-composer-attempts", &[])),
            ("Ctrl+C", message("cloud-composer-quit", &[])),
        ]);
        Self {
            composer,
            submitting: false,
            env_id,
            best_of_n,
        }
    }

    // Additional helpers can be added as usage evolves.
}

impl Default for NewTaskPage {
    fn default() -> Self {
        Self::new(/*env_id*/ None, /*best_of_n*/ 1)
    }
}
