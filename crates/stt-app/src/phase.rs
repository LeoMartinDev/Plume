use crate::prefs::Prefs;

/// Process-level machine. Live has no download field. Onboarding has no session.
pub enum AppPhase {
    Onboarding {
        prefs: Prefs,
        status: OnboardStatus,
        warning: Option<String>,
    },
    Live {
        prefs: Prefs,
    },
    Refused {
        prefs: Prefs,
        reason: String,
    },
}

impl AppPhase {
    pub fn prefs(&self) -> &Prefs {
        match self {
            Self::Onboarding { prefs, .. } | Self::Live { prefs } | Self::Refused { prefs, .. } => {
                prefs
            }
        }
    }

    pub fn prefs_mut(&mut self) -> &mut Prefs {
        match self {
            Self::Onboarding { prefs, .. } | Self::Live { prefs } | Self::Refused { prefs, .. } => {
                prefs
            }
        }
    }
}

pub enum OnboardStatus {
    Idle,
    Fetching { last: Progress },
    Failed { reason: String },
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub file: String,
    pub bytes: u64,
    pub total: Option<u64>,
}
