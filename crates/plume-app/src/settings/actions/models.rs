use super::super::{SettingsSection, SettingsView};
use crate::catalog::{self, ModelId};
use crate::download::DownloadRequest;
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::Prefs;
use gpui::Context;
use plume_engine::LanguageTarget;
use plume_session::{EngineTarget, HoldTarget, InsertionTarget};

impl SettingsView {
    pub(in crate::settings) fn register_download(&mut self, model: ModelId) -> DownloadRequest {
        self.downloads.register(model)
    }

    pub(in crate::settings) fn accepts_download(&self, request: DownloadRequest) -> bool {
        self.downloads.accepts(self.phase.prefs().model, request)
    }

    pub(in crate::settings) fn begin_download(
        &mut self,
        prefs: Prefs,
        cx: &mut Context<Self>,
    ) -> DownloadRequest {
        let request = self.register_download(prefs.model);
        let installed = catalog::is_complete(prefs.model, &prefs.model.data_dir());
        let status = if installed {
            OnboardStatus::Activating
        } else {
            OnboardStatus::Fetching {
                last: Progress {
                    file: "starting".into(),
                    bytes: 0,
                    total: None,
                    bytes_per_second: None,
                },
            }
        };
        self.set_phase(
            AppPhase::Onboarding {
                prefs,
                status,
                warning: None,
            },
            cx,
        );
        request
    }

    pub fn show_progress(&mut self, last: Progress, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Fetching { last };
        }
        cx.notify();
    }

    pub fn show_fetch_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        self.downloads.clear();
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Failed { reason };
        }
        cx.notify();
    }

    pub fn show_live(
        &mut self,
        hold_target: HoldTarget,
        engine_target: EngineTarget,
        language_target: LanguageTarget,
        insertion_target: InsertionTarget,
        cx: &mut Context<Self>,
    ) {
        self.reset_for_phase();
        self.downloads.clear();
        self.hold_target = Some(hold_target);
        self.engine_target = Some(engine_target);
        self.language_target = Some(language_target);
        self.insertion_target = Some(insertion_target);
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    pub(in crate::settings) fn engine_target(&self) -> Option<EngineTarget> {
        self.engine_target.clone()
    }

    pub(in crate::settings) fn show_swapped(
        &mut self,
        language_target: LanguageTarget,
        cx: &mut Context<Self>,
    ) {
        self.language_target = Some(language_target);
        self.downloads.clear();
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    pub fn show_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        self.downloads.clear();
        self.phase = AppPhase::Refused {
            prefs: self.phase.prefs().clone(),
            reason,
        };
        cx.notify();
    }

    pub(in crate::settings) fn delete_model(&mut self, id: ModelId, cx: &mut Context<Self>) {
        if self.settings_preview {
            return;
        }
        let selected = self.phase.prefs().model == id;
        if self.downloads.is_active_for(id) {
            self.save_error =
                Some("Wait for the current download to finish before deleting it.".into());
            cx.notify();
            return;
        }

        match crate::download::remove(id.offer(), &id.data_dir()) {
            Ok(()) => {
                self.save_error = None;
                if selected {
                    self.downloads.clear();
                    self.phase = AppPhase::Onboarding {
                        prefs: self.phase.prefs().clone(),
                        status: OnboardStatus::Idle,
                        warning: None,
                    };
                }
            }
            Err(err) => self.save_error = Some(format!("Could not delete model: {err}")),
        }
        cx.notify();
    }
}
