use crate::DecisionWorkflow;
use std::{collections::BTreeMap, sync::Mutex, time::{Duration, Instant}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct State {
    consecutive_failures: u32,
    state: BreakerState,
    opened_at: Option<Instant>,
    probe_in_flight: bool,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    open_for: Duration,
    state: Mutex<State>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, open_for: Duration) -> Self {
        Self {
            failure_threshold: failure_threshold.max(1),
            open_for,
            state: Mutex::new(State {
                consecutive_failures: 0,
                state: BreakerState::Closed,
                opened_at: None,
                probe_in_flight: false,
            }),
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state.lock().expect("circuit breaker mutex poisoned").state
    }

    /// Returns whether a request may start. Once the open interval elapses,
    /// exactly one caller is allowed to perform a half-open probe.
    pub fn allow(&self, now: Instant) -> bool {
        let mut state = self.state.lock().expect("circuit breaker mutex poisoned");
        match state.state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                let elapsed = state
                    .opened_at
                    .map(|opened| now.saturating_duration_since(opened))
                    .unwrap_or_default();
                if elapsed < self.open_for || state.probe_in_flight {
                    false
                } else {
                    state.state = BreakerState::HalfOpen;
                    state.probe_in_flight = true;
                    true
                }
            }
            BreakerState::HalfOpen => {
                if state.probe_in_flight {
                    false
                } else {
                    state.probe_in_flight = true;
                    true
                }
            }
        }
    }

    pub fn record_success(&self) {
        let mut state = self.state.lock().expect("circuit breaker mutex poisoned");
        state.consecutive_failures = 0;
        state.state = BreakerState::Closed;
        state.opened_at = None;
        state.probe_in_flight = false;
    }

    pub fn record_failure(&self, now: Instant) {
        let mut state = self.state.lock().expect("circuit breaker mutex poisoned");
        state.probe_in_flight = false;
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.state == BreakerState::HalfOpen
            || state.consecutive_failures >= self.failure_threshold
        {
            state.state = BreakerState::Open;
            state.opened_at = Some(now);
        }
    }

    pub fn reset(&self) {
        self.record_success();
    }
}

#[derive(Debug)]
pub struct CircuitBreakerSet {
    breakers: BTreeMap<DecisionWorkflow, CircuitBreaker>,
}

impl Default for CircuitBreakerSet {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(30))
    }
}

impl CircuitBreakerSet {
    pub fn new(failure_threshold: u32, open_for: Duration) -> Self {
        let breakers = crate::EXECUTABLE_WORKFLOWS
            .into_iter()
            .map(|workflow| (workflow, CircuitBreaker::new(failure_threshold, open_for)))
            .collect();
        Self { breakers }
    }

    pub fn get(&self, workflow: DecisionWorkflow) -> Option<&CircuitBreaker> {
        self.breakers.get(&workflow)
    }

    pub fn state(&self, workflow: DecisionWorkflow) -> BreakerState {
        self.get(workflow)
            .map(CircuitBreaker::state)
            .unwrap_or(BreakerState::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_breakers_are_isolated() {
        let set = CircuitBreakerSet::new(1, Duration::from_secs(60));
        let now = Instant::now();
        let retention = set.get(DecisionWorkflow::ToolOutputRetention).unwrap();
        retention.allow(now);
        retention.record_failure(now);
        assert_eq!(retention.state(), BreakerState::Open);
        assert_eq!(
            set.state(DecisionWorkflow::ContextRanking),
            BreakerState::Closed
        );
    }

    #[test]
    fn half_open_allows_one_probe() {
        let breaker = CircuitBreaker::new(1, Duration::ZERO);
        let now = Instant::now();
        assert!(breaker.allow(now));
        breaker.record_failure(now);
        assert!(breaker.allow(now));
        assert!(!breaker.allow(now));
        breaker.record_success();
        assert!(breaker.allow(now));
    }
}
