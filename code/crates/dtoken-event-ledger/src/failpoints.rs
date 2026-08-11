//! Deterministic fault injection for durability testing (PR-C6B).
//!
//! A small, compile-time-gated registry of named crash points.  Production
//! code calls [`maybe_crash`] at the exact byte boundaries where a process
//! could die; tests arm the registry, run the operation, and assert the
//! unique safe terminal state (acked records survive, unacked records are
//! absent or complete, never torn).
//!
//! The whole module only exists when the `failpoints` feature is enabled, so
//! release builds carry no test-only injection points.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Error returned at an injected crash point.  Callers treat it exactly like
/// an I/O failure: the operation aborts and the durable state is whatever
/// was already committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailpointError(pub &'static str);

impl std::fmt::Display for FailpointError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "failpoint '{}' triggered", self.0)
    }
}

impl std::error::Error for FailpointError {}

type Registry = Mutex<HashMap<&'static str, usize>>;

fn registry() -> &'static Registry {
    static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Mutex::new(HashMap::new()));
    &REGISTRY
}

/// Arm a named failpoint for `hits` more triggers.  `hits == 0` disarms.
/// `usize::MAX` fires on every check (used by process-crash tests that never
/// return).
pub fn arm(name: &'static str, hits: usize) {
    if hits == 0 {
        disarm(name);
        return;
    }
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(name, hits);
}

pub fn disarm(name: &'static str) {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(name);
}

pub fn clear_all() {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

pub fn armed(name: &'static str) -> bool {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(name)
}

/// Consume one hit.  Returns `true` when the failpoint should fire this
/// time.  A hit count of `usize::MAX` never depletes.
pub fn hit(name: &'static str) -> bool {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(remaining) = registry.get_mut(name) else {
        return false;
    };
    if *remaining == usize::MAX {
        return true;
    }
    *remaining -= 1;
    if *remaining == 0 {
        registry.remove(name);
    }
    true
}

/// Check a crash point: returns `Err(FailpointError)` when armed, otherwise
/// `Ok(())`.  This is the only entry point production code needs.
pub fn maybe_crash(name: &'static str) -> Result<(), FailpointError> {
    if hit(name) {
        Err(FailpointError(name))
    } else {
        Ok(())
    }
}
