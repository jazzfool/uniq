//! Simple atomic unique ID generator.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Generates a unique ID and returns it.
#[inline]
pub fn next() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
