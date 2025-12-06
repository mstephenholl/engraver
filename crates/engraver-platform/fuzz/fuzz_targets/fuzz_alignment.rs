//! Fuzz test for alignment functions
//!
//! Tests that alignment functions handle all inputs correctly.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct AlignmentInput {
    value: usize,
    alignment: usize,
}

fuzz_target!(|input: AlignmentInput| {
    let value = input.value;
    let alignment = input.alignment;

    // Skip zero alignment (would cause division by zero in some implementations)
    if alignment == 0 {
        // Our implementation handles this gracefully
        let up = align_up(value, alignment);
        let down = align_down(value, alignment);
        assert_eq!(up, value);
        assert_eq!(down, value);
        return;
    }

    // Test align_up
    let aligned_up = align_up(value, alignment);

    // Property: result >= value
    assert!(
        aligned_up >= value,
        "align_up({}, {}) = {} should be >= value",
        value,
        alignment,
        aligned_up
    );

    // Property: result is aligned
    assert!(
        aligned_up % alignment == 0,
        "align_up({}, {}) = {} should be divisible by {}",
        value,
        alignment,
        aligned_up,
        alignment
    );

    // Property: result is the smallest aligned value >= value
    if aligned_up > 0 {
        let prev = aligned_up - alignment;
        if prev < value {
            // prev is smaller than value, so aligned_up is correct
        } else {
            // This shouldn't happen
            panic!(
                "align_up({}, {}) = {} but {} would also work",
                value, alignment, aligned_up, prev
            );
        }
    }

    // Test align_down
    let aligned_down = align_down(value, alignment);

    // Property: result <= value
    assert!(
        aligned_down <= value,
        "align_down({}, {}) = {} should be <= value",
        value,
        alignment,
        aligned_down
    );

    // Property: result is aligned
    assert!(
        aligned_down % alignment == 0,
        "align_down({}, {}) = {} should be divisible by {}",
        value,
        alignment,
        aligned_down,
        alignment
    );

    // Test is_aligned
    let is_val_aligned = is_aligned(value, alignment);
    assert_eq!(
        is_val_aligned,
        value % alignment == 0,
        "is_aligned({}, {}) mismatch",
        value,
        alignment
    );

    // Property: aligned values are aligned
    assert!(
        is_aligned(aligned_up, alignment),
        "align_up result should be aligned"
    );
    assert!(
        is_aligned(aligned_down, alignment),
        "align_down result should be aligned"
    );
});

/// Align up implementation (copy for fuzzing)
fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

/// Align down implementation (copy for fuzzing)
fn align_down(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    value & !(alignment - 1)
}

/// Is aligned implementation (copy for fuzzing)
fn is_aligned(value: usize, alignment: usize) -> bool {
    if alignment == 0 {
        return true;
    }
    value % alignment == 0
}
