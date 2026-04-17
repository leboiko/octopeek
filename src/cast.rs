//! Saturating integer casts for terminal-UI arithmetic.
// All three helpers are used in at least one later phase; suppress the
// "function is never used" lint for the ones not yet called.
#![allow(dead_code)]
//!
//! Terminal dimensions and document line counts are bounded well below
//! `u16::MAX` / `u32::MAX` in any realistic scenario, but clippy's
//! `cast_possible_truncation` lint correctly flags bare `as` casts as
//! potentially lossy. These helpers make the intent explicit and saturate
//! rather than silently wrap on overflow.

/// Saturating cast from `usize` to `u32`.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn u32_sat(n: usize) -> u32 {
    // n is clamped to u32::MAX before casting, so truncation is intentional.
    n.min(u32::MAX as usize) as u32
}

/// Saturating cast from `usize` to `u16`.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn u16_sat(n: usize) -> u16 {
    // n is clamped to u16::MAX before casting, so truncation is intentional.
    n.min(u16::MAX as usize) as u16
}

/// Saturating cast from `u32` to `u16`.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn u16_from_u32(n: u32) -> u16 {
    // n is clamped to u16::MAX before casting; using `as u16` after `.min()` is
    // clearer than `u16::try_from(...).unwrap()`.
    n.min(u32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_sat_clamps_at_max() {
        assert_eq!(u32_sat(usize::MAX), u32::MAX);
        assert_eq!(u32_sat(42), 42);
    }

    #[test]
    fn u16_sat_clamps_at_max() {
        assert_eq!(u16_sat(usize::MAX), u16::MAX);
        assert_eq!(u16_sat(100), 100);
    }

    #[test]
    fn u16_from_u32_clamps() {
        assert_eq!(u16_from_u32(u32::MAX), u16::MAX);
        assert_eq!(u16_from_u32(255), 255);
    }
}
