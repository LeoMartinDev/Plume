use crate::history_policy::HistoryPolicy;
use gpui::Context;
use std::time::Duration;
const COPY_FEEDBACK_DURATION: Duration = Duration::from_secs(2);
use super::super::SettingsView;

impl SettingsView {
    fn invalidate_history_list(&self) {
        super::super::history_view::invalidate_list(
            &self.history_list,
            self.history.entries().len(),
        );
    }

    pub(in crate::settings) fn record_result(
        &mut self,
        result: plume_session::DictationResult,
        cx: &mut Context<Self>,
    ) {
        self.history_error = self.history.push(result).err();
        self.invalidate_history_list();
        cx.notify();
    }

    pub(in crate::settings) fn copy_history(
        &mut self,
        id: u64,
        text: String,
        cx: &mut Context<Self>,
    ) {
        match crate::history::copy_text(&text) {
            Ok(()) => {
                self.history_error = None;
                self.copied_history_id = Some(id);
                self.copy_feedback_serial = self.copy_feedback_serial.wrapping_add(1);
                let serial = self.copy_feedback_serial;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(COPY_FEEDBACK_DURATION).await;
                    let _ = this.update(cx, |view, cx| {
                        if view.copy_feedback_serial == serial {
                            view.copied_history_id = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            Err(error) => {
                self.history_error = Some(error);
                self.copied_history_id = None;
            }
        }
        cx.notify();
    }

    pub(in crate::settings) fn delete_history(&mut self, id: u64, cx: &mut Context<Self>) {
        self.history_error = self.history.delete(id).err();
        self.invalidate_history_list();
        cx.notify();
    }

    pub(in crate::settings) fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.history_error = self.history.clear().err();
        self.invalidate_history_list();
        cx.notify();
    }

    pub(in crate::settings) fn commit_history_policy(
        &mut self,
        policy: HistoryPolicy,
        cx: &mut Context<Self>,
    ) {
        let outcome = update_history_policy(
            self.phase.prefs_mut(),
            &mut self.history,
            policy,
            &self.prefs_path,
        );
        match outcome {
            Ok(()) => {
                self.save_error = None;
                self.history_error = None;
            }
            Err(HistorySettingsError::Preferences(error)) => self.save_error = Some(error),
            Err(HistorySettingsError::History(error)) => {
                self.save_error = None;
                self.history_error = Some(error);
            }
        }
        self.invalidate_history_list();
        cx.notify();
    }
}

#[derive(Debug)]
enum HistorySettingsError {
    Preferences(String),
    History(String),
}

fn update_history_policy(
    prefs: &mut crate::prefs::Prefs,
    history: &mut crate::history::HistoryStore,
    policy: HistoryPolicy,
    prefs_path: &std::path::Path,
) -> Result<(), HistorySettingsError> {
    let mut next = prefs.clone();
    next.set_history(policy);
    crate::prefs::save_at(prefs_path, &next)
        .map_err(|error| HistorySettingsError::Preferences(error.to_string()))?;
    *prefs = next;
    history
        .apply_policy(policy)
        .map_err(HistorySettingsError::History)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use crate::prefs::{Prefs, PrefsLoad};
    use std::path::PathBuf;

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "plume-history-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn failed_preference_save_does_not_change_either_policy() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        let prefs_path = root.join("prefs.toml");
        std::fs::create_dir(prefs_path.with_extension("toml.tmp")).unwrap();
        let mut prefs = Prefs::default_fresh();
        let original = prefs.clone();
        let mut history = HistoryStore::empty(root.join("history.json"), prefs.history());
        assert!(matches!(
            update_history_policy(
                &mut prefs,
                &mut history,
                HistoryPolicy::new(Some(7), Some(100)).unwrap(),
                &prefs_path
            ),
            Err(HistorySettingsError::Preferences(_))
        ));
        assert_eq!(prefs, original);
        assert!(!root.join("history.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_history_save_keeps_the_persisted_preference_for_retry() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("history.json");
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let mut prefs = Prefs::default_fresh();
        let mut history = HistoryStore::empty(path.clone(), prefs.history());
        let policy = HistoryPolicy::new(Some(7), Some(100)).unwrap();
        let prefs_path = root.join("prefs.toml");
        assert!(matches!(
            update_history_policy(&mut prefs, &mut history, policy, &prefs_path),
            Err(HistorySettingsError::History(_))
        ));
        assert_eq!(prefs.history(), policy);
        match crate::prefs::load_at(&prefs_path) {
            PrefsLoad::Loaded(saved) => assert_eq!(saved.history(), policy),
            _ => panic!("expected persisted history preferences"),
        }
        std::fs::remove_dir(path.with_extension("json.tmp")).unwrap();
        update_history_policy(&mut prefs, &mut history, policy, &prefs_path).unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
