//! CLI command implementations

pub mod benchmark;
pub mod checksum;
pub mod config;
pub mod erase;
pub mod list;
pub mod verify;
pub mod write;

/// The error a cancelled run returns.
///
/// It carries [`engraver_core::Error::Cancelled`], which is what makes `main`
/// exit 130, and `message` is the one line `main` prints for it.
pub(crate) fn cancelled(message: &str) -> anyhow::Error {
    anyhow::Error::new(engraver_core::Error::Cancelled).context(message.to_string())
}

/// Whether any error in the chain is [`engraver_core::Error::Cancelled`].
pub(crate) fn is_cancelled(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<engraver_core::Error>(),
            Some(engraver_core::Error::Cancelled)
        )
    })
}

/// Turn an error from `engraver-core` into the command's error.
///
/// Like `.context(context)`, except that a cancel becomes [`cancelled`] with
/// `cancel_message`, so the user never reads "Failed to ..." for a Ctrl+C.
pub(crate) fn context_or_cancelled(
    err: engraver_core::Error,
    context: &str,
    cancel_message: &str,
) -> anyhow::Error {
    match err {
        engraver_core::Error::Cancelled => cancelled(cancel_message),
        err => anyhow::Error::new(err).context(context.to_string()),
    }
}

/// Assert that `err` carries `engraver_core::Error::Cancelled` anywhere in its
/// chain.
#[cfg(test)]
pub(crate) fn assert_cancelled(err: &anyhow::Error) {
    assert!(
        is_cancelled(err),
        "expected a Cancelled error in the chain, got: {err:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancelled_carries_the_cancelled_error_and_its_message() {
        let err = cancelled("Write cancelled by user.");

        assert_cancelled(&err);
        assert_eq!(err.to_string(), "Write cancelled by user.");
    }

    #[test]
    fn test_context_or_cancelled_turns_a_cancel_into_a_cancelled_error() {
        let err = context_or_cancelled(
            engraver_core::Error::Cancelled,
            "Failed to checksum source",
            "Verification cancelled.",
        );

        assert_cancelled(&err);
        assert_eq!(err.to_string(), "Verification cancelled.");
    }

    #[test]
    fn test_context_or_cancelled_keeps_a_real_failure_a_failure() {
        let io_err = std::io::Error::other("disk error");

        let err = context_or_cancelled(
            engraver_core::Error::Io(io_err),
            "Failed to checksum source",
            "Verification cancelled.",
        );

        assert_eq!(err.to_string(), "Failed to checksum source");
        assert!(
            !is_cancelled(&err),
            "a disk error must not read as a cancel: {err:?}"
        );
    }
}
