//! The role engine's shared serving primitives: single-flight coalescing and the stale-on-error
//! bound.
//!
//! Every cached (proxy) role does the same two things around an upstream fetch, whatever it caches.
//! It coalesces concurrent misses for one key so a cold page is fetched once, not once per waiter —
//! the difference between a warm cache and a thundering herd on a popular project. And it decides how
//! long a page past its freshness window may still answer while the upstream is unreachable. Both live
//! here so a `PyPI` page and an `OCI` manifest share one implementation rather than drifting apart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Per-key single-flight locks. Concurrent misses for one key take the same lock, so exactly one
/// fetches from upstream and stores the result while the rest wait and then serve it from cache.
pub type Inflight = Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>;

/// The lock concurrent misses for `key` share.
///
/// # Panics
/// Panics if the inflight map's mutex was poisoned by a thread that panicked while holding it.
#[must_use]
pub fn flight_gate(inflight: &Inflight, key: &str) -> Arc<tokio::sync::Mutex<()>> {
    inflight
        .lock()
        .expect("inflight lock")
        .entry(key.to_owned())
        .or_default()
        .clone()
}

/// Release a single-flight hold: unlock first so a waiter parked on the gate proceeds, then drop the
/// map entry so later requests start fresh.
///
/// # Panics
/// Panics if the inflight map's mutex was poisoned.
pub fn release_flight(inflight: &Inflight, key: &str, guard: tokio::sync::OwnedMutexGuard<()>) {
    drop(guard);
    inflight.lock().expect("inflight lock").remove(key);
}

/// Whether a page past its freshness window may still answer while the upstream cannot be reached.
///
/// Serving something old beats serving nothing while an upstream reboots, but only for a while: a
/// cache that answers with whatever it last saw, forever, has stopped being a cache and become a
/// fork. `max_stale_secs` bounds the outage a stale page papers over; `0` removes the bound, which is
/// what an operator deliberately mirroring an unreliable upstream asks for. `freshness_secs` is the
/// lifetime the page was fresh for — an ecosystem passes the upstream-granted lifetime, or its own
/// fallback.
#[must_use]
pub const fn within_stale_bound(now: i64, max_stale_secs: i64, fetched_at: i64, freshness_secs: i64) -> bool {
    max_stale_secs == 0 || now.saturating_sub(fetched_at) < freshness_secs + max_stale_secs
}

#[cfg(test)]
mod tests {
    use super::within_stale_bound;

    #[test]
    fn test_zero_max_stale_serves_any_age() {
        assert!(within_stale_bound(1_000_000, 0, 0, 60));
    }

    #[test]
    fn test_stale_within_the_bound_serves_and_past_it_does_not() {
        // fresh for 60s, tolerate 300s past that: servable up to 360s after fetch.
        assert!(within_stale_bound(1_359, 300, 1_000, 60));
        assert!(!within_stale_bound(1_360, 300, 1_000, 60));
    }

    #[test]
    fn test_a_future_fetch_time_does_not_underflow() {
        assert!(within_stale_bound(1_000, 300, 5_000, 60));
    }
}
