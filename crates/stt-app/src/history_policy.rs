/// Validated limits for locally stored transcriptions. `None` disables a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryPolicy {
    retention_days: Option<u32>,
    max_entries: Option<usize>,
}

impl HistoryPolicy {
    pub const DEFAULT_RETENTION_DAYS: u32 = 30;
    pub const DEFAULT_MAX_ENTRIES: usize = 500;
    pub const MAX_RETENTION_DAYS: u32 = 3_650;
    pub const MAX_ENTRIES: usize = 100_000;

    pub fn new(
        retention_days: Option<u32>,
        max_entries: Option<usize>,
    ) -> Result<Self, &'static str> {
        if retention_days.is_some_and(|days| !(1..=Self::MAX_RETENTION_DAYS).contains(&days)) {
            return Err("history retention must be between 1 and 3650 days");
        }
        if max_entries.is_some_and(|entries| !(1..=Self::MAX_ENTRIES).contains(&entries)) {
            return Err("history size must be between 1 and 100000 entries");
        }
        Ok(Self {
            retention_days,
            max_entries,
        })
    }

    pub fn retention_days(self) -> Option<u32> {
        self.retention_days
    }
    pub fn max_entries(self) -> Option<usize> {
        self.max_entries
    }
    pub(crate) fn retention_secs(self) -> Option<u64> {
        self.retention_days.map(|days| u64::from(days) * 86_400)
    }
}

impl Default for HistoryPolicy {
    fn default() -> Self {
        Self {
            retention_days: Some(Self::DEFAULT_RETENTION_DAYS),
            max_entries: Some(Self::DEFAULT_MAX_ENTRIES),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_are_validated() {
        assert!(HistoryPolicy::new(Some(1), Some(1)).is_ok());
        assert!(HistoryPolicy::new(Some(3650), Some(100_000)).is_ok());
        for (days, entries) in [(0, 500), (3651, 500), (30, 0), (30, 100_001)] {
            assert!(HistoryPolicy::new(Some(days), Some(entries)).is_err());
        }
    }
    #[test]
    fn unlimited_is_valid_and_zero_is_still_invalid() {
        assert!(HistoryPolicy::new(None, None).is_ok());
        assert!(HistoryPolicy::new(Some(30), None).is_ok());
        assert!(HistoryPolicy::new(None, Some(500)).is_ok());
        assert!(HistoryPolicy::new(Some(0), None).is_err());
        assert!(HistoryPolicy::new(None, Some(0)).is_err());
    }
}
