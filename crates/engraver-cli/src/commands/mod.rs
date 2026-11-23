//! CLI command implementations

pub mod benchmark;
pub mod checksum;
pub mod config;
pub mod erase;
pub mod list;
pub mod verify;
pub mod write;

/// Assert that `err` carries `engraver_core::Error::Cancelled` anywhere in its
/// chain, which is what makes `main` exit 130.
#[cfg(test)]
pub(crate) fn assert_cancelled(err: &anyhow::Error) {
    assert!(
        err.chain().any(|cause| matches!(
            cause.downcast_ref::<engraver_core::Error>(),
            Some(engraver_core::Error::Cancelled)
        )),
        "expected a Cancelled error in the chain, got: {err:?}"
    );
}
