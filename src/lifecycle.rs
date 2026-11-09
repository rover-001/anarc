//! Runtime policy for keeping inactive CEF browsers within a bounded footprint.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::model::TabState;

pub const DISCARD_AFTER: Duration = Duration::from_secs(30 * 60);

#[derive(Default)]
pub struct TabLifecycle {
    last_active: HashMap<String, Instant>,
    states: HashMap<String, TabState>,
}

impl TabLifecycle {
    pub fn opened(&mut self, tab_id: &str, now: Instant) {
        self.last_active.insert(tab_id.to_owned(), now);
        self.states.insert(tab_id.to_owned(), TabState::Live);
    }

    pub fn activated(&mut self, tab_id: &str, now: Instant) {
        self.last_active.insert(tab_id.to_owned(), now);
        self.states.insert(tab_id.to_owned(), TabState::Live);
    }

    pub fn deactivated(&mut self, tab_id: &str, now: Instant) {
        if self.states.contains_key(tab_id) {
            self.last_active.insert(tab_id.to_owned(), now);
        }
    }

    pub fn closed(&mut self, tab_id: &str) {
        self.last_active.remove(tab_id);
        self.states.remove(tab_id);
    }

    pub fn state(&self, tab_id: &str) -> Option<TabState> {
        self.states.get(tab_id).copied()
    }

    #[cfg(test)]
    pub fn discard_expired(&mut self, now: Instant, active: Option<&str>) -> Vec<String> {
        self.discard_expired_many(
            now,
            &active.into_iter().map(str::to_owned).collect::<Vec<_>>(),
        )
    }

    pub fn discard_expired_many(&mut self, now: Instant, active: &[String]) -> Vec<String> {
        let expired: Vec<String> = self
            .last_active
            .iter()
            .filter(|(id, last)| {
                !active.iter().any(|active_id| active_id == *id)
                    && now.saturating_duration_since(**last) >= DISCARD_AFTER
                    && self.states.get(*id) == Some(&TabState::Live)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            self.states.insert(id.clone(), TabState::Discarded);
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discards_only_inactive_live_tabs_after_thirty_minutes() {
        let start = Instant::now();
        let mut lifecycle = TabLifecycle::default();
        lifecycle.opened("old", start);
        lifecycle.opened("active", start);
        lifecycle.opened("recent", start + DISCARD_AFTER - Duration::from_secs(1));

        assert!(
            lifecycle
                .discard_expired(start + DISCARD_AFTER, Some("active"))
                .contains(&"old".to_owned())
        );
        assert_eq!(lifecycle.state("old"), Some(TabState::Discarded));
        assert_eq!(lifecycle.state("active"), Some(TabState::Live));
        assert_eq!(lifecycle.state("recent"), Some(TabState::Live));
        lifecycle.activated("old", start + DISCARD_AFTER);
        assert_eq!(lifecycle.state("old"), Some(TabState::Live));
    }
}
