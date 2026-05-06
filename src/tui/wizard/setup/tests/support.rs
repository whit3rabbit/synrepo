//! Shared fixture helpers for setup wizard tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::ExplainConfig;
use crate::tui::wizard::setup::explain::ExplainWizardSupport;
use crate::tui::wizard::setup::state::SetupWizardState;

pub(super) const RELEVANT_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "OPENROUTER_API_KEY",
    "ZAI_API_KEY",
    "MINIMAX_API_KEY",
];

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct EnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(super) fn new() -> Self {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for var in RELEVANT_ENV {
            std::env::remove_var(var);
        }
        Self { _guard: guard }
    }

    pub(super) fn set(&self, key: &str, value: &str) {
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for var in RELEVANT_ENV {
            std::env::remove_var(var);
        }
    }
}

pub(super) fn press(state: &mut SetupWizardState, code: KeyCode) {
    state.handle_key(code, KeyModifiers::empty());
}

pub(super) fn support_with_saved_anthropic() -> ExplainWizardSupport {
    let config = ExplainConfig {
        anthropic_api_key: Some("saved-anthropic-key".to_string()),
        ..Default::default()
    };
    ExplainWizardSupport::with_global_explain(config)
}

pub(super) fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry.expect("walk");
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)
                .expect("strip")
                .to_path_buf();
            let bytes = std::fs::read(entry.path()).expect("read");
            out.insert(rel, bytes);
        }
    }
    out
}

pub(super) fn drive_cancel_and_assert_no_writes(drive: impl FnOnce(&mut SetupWizardState)) {
    use crate::bootstrap::runtime_probe::AgentTargetKind;
    use crate::config::Mode;

    let tempdir = tempfile::tempdir().expect("tempdir");
    std::fs::write(tempdir.path().join("fixture.txt"), b"original content").expect("seed fixture");
    std::fs::create_dir_all(tempdir.path().join("nested/dir")).expect("mkdir");
    std::fs::write(tempdir.path().join("nested/dir/leaf.md"), b"# leaf").expect("seed leaf");
    let before = snapshot_tree(tempdir.path());

    let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
    drive(&mut s);
    assert!(s.cancelled, "drive closure must cancel the wizard");
    assert!(
        s.finalize().is_none(),
        "cancelled wizard must yield no plan"
    );

    let after = snapshot_tree(tempdir.path());
    assert_eq!(
        before, after,
        "working tree must be byte-identical after cancellation",
    );
}

/// Drive the wizard from Splash to SelectExplain using the defaults
/// (auto mode, skip target, skip embeddings). Passes through the
/// `ExplainExplain` explainer step automatically.
pub(super) fn drive_to_explain(s: &mut SetupWizardState) {
    use crate::tui::wizard::setup::state::{SetupStep, WIZARD_TARGETS};

    press(s, KeyCode::Enter); // splash → mode
    press(s, KeyCode::Enter); // mode → target
    for _ in 0..WIZARD_TARGETS.len() {
        press(s, KeyCode::Down); // land on "Skip"
    }
    press(s, KeyCode::Enter); // target → embeddings
    assert_eq!(s.step, SetupStep::SelectEmbeddings);
    press(s, KeyCode::Enter); // embeddings → explain explainer
    assert_eq!(s.step, SetupStep::ExplainExplain);
    press(s, KeyCode::Enter); // explain → explain
    assert_eq!(s.step, SetupStep::SelectExplain);
}
