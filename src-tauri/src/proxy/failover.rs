//! Module Overview
//! Runtime-only provider failover state for group routing.
//! Tracks provider failures separately from persisted config and offers helper operations.

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailoverRouteDecision {
    pub provider_id: String,
    pub failover_active: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct FailoverConfigSnapshot {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub cooldown_seconds: u32,
}

impl FailoverConfigSnapshot {
    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            failure_threshold: 0,
            cooldown_seconds: 0,
        }
    }

    fn threshold(&self) -> u32 {
        self.failure_threshold.max(1)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FailoverProviderState {
    consecutive_failures: u32,
}

impl FailoverProviderState {
    pub(crate) fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveFailoverState {
    provider_id: String,
    cooldown_until: DateTime<Utc>,
}

impl ActiveFailoverState {
    pub(crate) fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub(crate) fn is_cooldown_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.cooldown_until
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GroupFailoverRuntime {
    provider_states: HashMap<String, FailoverProviderState>,
    active_failover: Option<ActiveFailoverState>,
}

impl GroupFailoverRuntime {
    pub(crate) fn provider_state(&self, provider_id: &str) -> Option<&FailoverProviderState> {
        self.provider_states.get(provider_id)
    }

    pub(crate) fn active_failover(&self) -> Option<&ActiveFailoverState> {
        self.active_failover.as_ref()
    }
}

pub(crate) type FailoverStateMap = HashMap<String, GroupFailoverRuntime>;

pub(crate) fn select_provider(
    state_map: &mut FailoverStateMap,
    group_id: &str,
    preferred_provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
) -> FailoverRouteDecision {
    select_provider_at(
        state_map,
        group_id,
        preferred_provider_id,
        provider_ids,
        config,
        Utc::now(),
    )
}

pub(crate) fn select_provider_at(
    state_map: &mut FailoverStateMap,
    group_id: &str,
    preferred_provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
    now: DateTime<Utc>,
) -> FailoverRouteDecision {
    if provider_ids.is_empty() || !config.enabled {
        clear_group_runtime(state_map, group_id);
        return FailoverRouteDecision {
            provider_id: preferred_provider_id.to_string(),
            failover_active: false,
        };
    }

    let runtime = state_map.entry(group_id.to_string()).or_default();

    if let Some(active) = runtime.active_failover.as_ref() {
        if active.provider_id == preferred_provider_id {
            runtime.active_failover = None;
        } else if active.is_cooldown_expired_at(now) {
            return FailoverRouteDecision {
                provider_id: preferred_provider_id.to_string(),
                failover_active: false,
            };
        } else if provider_ids
            .iter()
            .any(|provider_id| provider_id == &active.provider_id)
        {
            return FailoverRouteDecision {
                provider_id: active.provider_id.clone(),
                failover_active: true,
            };
        } else {
            runtime.active_failover = None;
        }
    }

    FailoverRouteDecision {
        provider_id: preferred_provider_id.to_string(),
        failover_active: false,
    }
}

pub(crate) fn record_provider_success(
    state_map: &mut FailoverStateMap,
    group_id: &str,
    provider_id: &str,
) {
    record_provider_success_at(state_map, group_id, provider_id, Utc::now())
}

pub(crate) fn record_provider_success_at(
    state_map: &mut FailoverStateMap,
    group_id: &str,
    provider_id: &str,
    now: DateTime<Utc>,
) {
    let runtime = state_map.entry(group_id.to_string()).or_default();
    runtime
        .provider_states
        .entry(provider_id.to_string())
        .or_default()
        .consecutive_failures = 0;

    if runtime
        .active_failover
        .as_ref()
        .map(|active| active.provider_id != provider_id && active.is_cooldown_expired_at(now))
        .unwrap_or(false)
    {
        runtime.active_failover = None;
    }
}

pub(crate) fn record_provider_failure(
    state_map: &mut FailoverStateMap,
    group_id: &str,
    provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
    now: DateTime<Utc>,
) {
    if !config.enabled {
        clear_group_runtime(state_map, group_id);
        return;
    }

    let runtime = state_map.entry(group_id.to_string()).or_default();
    let provider_state = runtime
        .provider_states
        .entry(provider_id.to_string())
        .or_default();
    provider_state.consecutive_failures = provider_state.consecutive_failures.saturating_add(1);

    if !config.enabled {
        runtime.active_failover = None;
        return;
    }

    if provider_state.consecutive_failures < config.threshold() {
        return;
    }

    if let Some(next_provider_id) = next_provider_id(provider_ids, provider_id) {
        runtime.active_failover = Some(ActiveFailoverState {
            provider_id: next_provider_id,
            cooldown_until: now + Duration::seconds(i64::from(config.cooldown_seconds)),
        });
    }
}

pub(crate) fn is_failover_cooldown_expired(
    state_map: &FailoverStateMap,
    group_id: &str,
    now: DateTime<Utc>,
) -> bool {
    state_map
        .get(group_id)
        .and_then(|runtime| runtime.active_failover())
        .map(|active| active.is_cooldown_expired_at(now))
        .unwrap_or(false)
}

pub(crate) fn active_failover_provider_id(
    state_map: &FailoverStateMap,
    group_id: &str,
) -> Option<String> {
    state_map
        .get(group_id)
        .and_then(|runtime| runtime.active_failover())
        .map(|active| active.provider_id().to_string())
}

pub(crate) fn runtime_current_provider_id(
    state_map: &FailoverStateMap,
    group_id: &str,
    preferred_provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
    now: DateTime<Utc>,
) -> Option<String> {
    if provider_ids.is_empty() {
        return None;
    }

    let decision = state_map
        .get(group_id)
        .and_then(|runtime| runtime.active_failover())
        .map(|active| {
            if !config.enabled || active.provider_id() == preferred_provider_id {
                preferred_provider_id.to_string()
            } else if active.is_cooldown_expired_at(now) {
                preferred_provider_id.to_string()
            } else if provider_ids
                .iter()
                .any(|provider_id| provider_id == active.provider_id())
            {
                active.provider_id().to_string()
            } else {
                preferred_provider_id.to_string()
            }
        })
        .unwrap_or_else(|| preferred_provider_id.to_string());

    Some(decision)
}

pub(crate) fn provider_failure_count(
    state_map: &FailoverStateMap,
    group_id: &str,
    provider_id: &str,
) -> u32 {
    state_map
        .get(group_id)
        .and_then(|runtime| runtime.provider_state(provider_id))
        .map(FailoverProviderState::consecutive_failures)
        .unwrap_or(0)
}

fn clear_group_runtime(state_map: &mut FailoverStateMap, group_id: &str) {
    state_map.remove(group_id);
}

fn next_provider_id(provider_ids: &[String], current_provider_id: &str) -> Option<String> {
    if provider_ids.len() < 2 {
        return None;
    }

    let current_index = provider_ids
        .iter()
        .position(|provider_id| provider_id == current_provider_id)?;
    let next_index = (current_index + 1) % provider_ids.len();
    provider_ids.get(next_index).cloned()
}

#[cfg(test)]
mod tests {
    use super::{
        active_failover_provider_id, is_failover_cooldown_expired, provider_failure_count,
        record_provider_failure, record_provider_success, record_provider_success_at,
        select_provider, select_provider_at, FailoverConfigSnapshot, FailoverStateMap,
    };
    use chrono::{Duration, TimeZone, Utc};

    fn provider_ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    fn failover_config() -> FailoverConfigSnapshot {
        FailoverConfigSnapshot {
            enabled: true,
            failure_threshold: 3,
            cooldown_seconds: 60,
        }
    }

    #[test]
    fn failover_default_route_uses_preferred_provider() {
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        let selected = select_provider(&mut state, "dev", "p1", &providers, &failover_config());

        assert_eq!(selected.provider_id, "p1");
        assert!(!selected.failover_active);
    }

    #[test]
    fn failover_switches_after_threshold_for_group_provider_pair() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);

        let selected =
            select_provider_at(&mut state, "dev", "p1", &providers, &failover_config(), now);

        assert_eq!(provider_failure_count(&state, "dev", "p1"), 3);
        assert_eq!(
            active_failover_provider_id(&state, "dev"),
            Some("p2".to_string())
        );
        assert_eq!(selected.provider_id, "p2");
        assert!(selected.failover_active);
    }

    #[test]
    fn failover_state_is_tracked_per_group_and_provider() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(
            &mut state,
            "prod",
            "p1",
            &providers,
            &failover_config(),
            now,
        );
        record_provider_failure(
            &mut state,
            "prod",
            "p1",
            &providers,
            &failover_config(),
            now,
        );
        record_provider_failure(
            &mut state,
            "prod",
            "p1",
            &providers,
            &failover_config(),
            now,
        );
        record_provider_failure(&mut state, "dev", "p2", &providers, &failover_config(), now);

        assert_eq!(provider_failure_count(&state, "dev", "p1"), 1);
        assert_eq!(provider_failure_count(&state, "dev", "p2"), 1);
        assert_eq!(provider_failure_count(&state, "prod", "p1"), 3);
        assert_eq!(active_failover_provider_id(&state, "dev"), None);
        assert_eq!(
            active_failover_provider_id(&state, "prod"),
            Some("p2".to_string())
        );
    }

    #[test]
    fn successful_preferred_provider_retry_after_cooldown_clears_failover_state() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let after_cooldown = now + Duration::seconds(61);
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);

        let selected = select_provider_at(
            &mut state,
            "dev",
            "p1",
            &providers,
            &failover_config(),
            after_cooldown,
        );
        assert_eq!(selected.provider_id, "p1");
        assert_eq!(
            active_failover_provider_id(&state, "dev"),
            Some("p2".to_string())
        );

        record_provider_success_at(&mut state, "dev", "p1", after_cooldown);

        assert_eq!(active_failover_provider_id(&state, "dev"), None);
        assert_eq!(provider_failure_count(&state, "dev", "p1"), 0);
    }

    #[test]
    fn failover_retries_preferred_provider_after_cooldown() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let after_cooldown = now + Duration::seconds(61);
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);

        assert!(is_failover_cooldown_expired(&state, "dev", after_cooldown));

        let selected = select_provider_at(
            &mut state,
            "dev",
            "p1",
            &providers,
            &failover_config(),
            after_cooldown,
        );

        assert_eq!(selected.provider_id, "p1");
        assert!(!selected.failover_active);
        assert_eq!(
            active_failover_provider_id(&state, "dev"),
            Some("p2".to_string())
        );
    }

    #[test]
    fn failover_keeps_using_failover_provider_before_cooldown_expires() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let before_cooldown = now + Duration::seconds(30);
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);

        assert!(!is_failover_cooldown_expired(
            &state,
            "dev",
            before_cooldown
        ));

        let selected = select_provider_at(
            &mut state,
            "dev",
            "p1",
            &providers,
            &failover_config(),
            before_cooldown,
        );

        assert_eq!(selected.provider_id, "p2");
        assert!(selected.failover_active);
        assert_eq!(
            active_failover_provider_id(&state, "dev"),
            Some("p2".to_string())
        );
    }

    #[test]
    fn disabled_failover_does_not_accumulate_failure_debt() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();
        let disabled = FailoverConfigSnapshot::disabled();

        record_provider_failure(&mut state, "dev", "p1", &providers, &disabled, now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &disabled, now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &disabled, now);

        assert_eq!(provider_failure_count(&state, "dev", "p1"), 0);
        assert_eq!(active_failover_provider_id(&state, "dev"), None);

        let selected = select_provider(&mut state, "dev", "p1", &providers, &disabled);
        assert_eq!(selected.provider_id, "p1");
        assert!(!selected.failover_active);

        let enabled = failover_config();
        let selected_enabled = select_provider(&mut state, "dev", "p1", &providers, &enabled);
        assert_eq!(selected_enabled.provider_id, "p1");
        assert!(!selected_enabled.failover_active);
    }

    #[test]
    fn failover_success_resets_provider_failure_count() {
        let now = Utc.with_ymd_and_hms(2026, 3, 26, 12, 0, 0).unwrap();
        let providers = provider_ids(&["p1", "p2"]);
        let mut state = FailoverStateMap::default();

        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_failure(&mut state, "dev", "p1", &providers, &failover_config(), now);
        record_provider_success(&mut state, "dev", "p1");

        assert_eq!(provider_failure_count(&state, "dev", "p1"), 0);
    }
}
