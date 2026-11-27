//! CLI command implementations

pub mod benchmark;
pub mod checksum;
pub mod config;
pub mod erase;
pub mod list;
pub mod verify;
pub mod write;

use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use engraver_core::{Checksum, ChecksumAlgorithm, Source, SourceOptions, Verifier, VerifyConfig};
use indicatif::{ProgressBar, ProgressStyle};

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

/// Whether to hide the lines that lead up to a confirmation prompt.
///
/// `silent` hides status output, but a run that will ask a question keeps the
/// lines that say what it is about, so a prompt never appears without the drive
/// and image it refers to. `skip_confirm` is true when nothing will be asked,
/// and then there is no prompt to lead up to.
pub(crate) fn hides_lead_up(silent: bool, skip_confirm: bool) -> bool {
    silent && skip_confirm
}

/// Ask a yes/no question. Ctrl+C at the prompt cancels the run.
pub(crate) fn confirm(prompt: &str, default: bool) -> Result<bool> {
    into_answer(crate::prompt::ask(prompt, default))
}

/// Turn what [`crate::prompt::ask`] returned into the command's answer: Ctrl+C
/// cancels the run and any other failure is an error.
pub(crate) fn into_answer(asked: std::io::Result<Option<bool>>) -> Result<bool> {
    match asked {
        Ok(Some(answer)) => Ok(answer),
        Ok(None) => Err(cancelled("Cancelled.")),
        Err(err) => Err(anyhow::Error::new(err)),
    }
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

/// How to show a source's length: the image length, or the compressed length
/// labelled as such. `None` when neither is known.
pub(crate) fn describe_source_size(
    info: &engraver_core::SourceInfo,
    format_size: fn(u64) -> String,
) -> Option<String> {
    match (info.size, info.compressed_size) {
        (Some(size), _) => Some(format_size(size)),
        (None, Some(compressed)) => Some(format!("{} compressed", format_size(compressed))),
        (None, None) => None,
    }
}

/// Create a progress bar for operations
pub(crate) fn create_progress_bar(
    total: Option<u64>,
    operation: &str,
    silent: bool,
) -> ProgressBar {
    if silent {
        return ProgressBar::hidden();
    }

    let (pb, template) = match total {
        Some(t) => (
            ProgressBar::new(t),
            format!(
                "  {{spinner:.green}} {operation} [{{bar:40.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{eta}})"
            ),
        ),
        None => (
            ProgressBar::new_spinner(),
            format!("  {{spinner:.green}} {operation} {{bytes}} ({{bytes_per_sec}})"),
        ),
    };

    pb.set_style(
        ProgressStyle::default_bar()
            .template(&template)
            .unwrap()
            .progress_chars("█▓░"),
    );

    pb
}

struct CountingReader<R> {
    inner: R,
    count: u64,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count += n as u64;
        Ok(n)
    }
}

fn ensure_image_length(actual: u64, expected: Option<u64>) -> Result<()> {
    match expected {
        Some(expected) if expected != actual => bail!(
            "Verification failed: the image is {actual} bytes once decompressed, \
             but {expected} bytes were expected"
        ),
        _ => Ok(()),
    }
}

fn target_checksum_error(err: engraver_core::Error) -> anyhow::Error {
    match err {
        engraver_core::Error::SizeMismatch {
            source_size,
            target_size,
        } => anyhow::anyhow!(
            "Verification failed: the target is shorter than the image \
             ({target_size} of {source_size} bytes)"
        ),
        err => context_or_cancelled(err, "Failed to checksum target", "Verification cancelled."),
    }
}

/// Checksum the image at `source`, read with `options`, then the first
/// image-length bytes of `target`.
///
/// Returns `(source, target)` checksums. `expected_len` is the image length
/// when already known, such as the bytes just written; an image of any other
/// length fails verification. `target` must be positioned at its start.
pub(crate) fn checksum_image_and_target<T: Read + ?Sized>(
    source: &str,
    options: &SourceOptions,
    target: &mut T,
    expected_len: Option<u64>,
    block_size: usize,
    cancel_flag: &Arc<AtomicBool>,
    silent: bool,
) -> Result<(Checksum, Checksum)> {
    let verifier = |pb: &ProgressBar| {
        let pb = pb.clone();
        Verifier::with_config(VerifyConfig::new().block_size(block_size))
            .with_cancel_flag(Arc::clone(cancel_flag))
            .on_progress(move |p| pb.set_position(p.bytes_processed))
    };

    println_if!(silent, "  Calculating source checksum...");
    let source_reader = Source::open_with_options(source, 0, options)
        .with_context(|| format!("Failed to open source: {source}"))?;
    let mut source_reader = CountingReader {
        inner: source_reader,
        count: 0,
    };

    let pb = create_progress_bar(expected_len, "Checksumming source", silent);
    let source_checksum = verifier(&pb)
        .calculate_checksum(&mut source_reader, ChecksumAlgorithm::Sha256, expected_len)
        .map_err(|e| {
            context_or_cancelled(e, "Failed to checksum source", "Verification cancelled.")
        })?;
    pb.finish_and_clear();

    let image_len = source_reader.count;
    ensure_image_length(image_len, expected_len)?;

    println_if!(silent, "  Calculating target checksum...");
    let pb = create_progress_bar(Some(image_len), "Checksumming target", silent);
    let target_checksum = verifier(&pb)
        .calculate_checksum_of_prefix(target, ChecksumAlgorithm::Sha256, image_len)
        .map_err(target_checksum_error)?;
    pb.finish_and_clear();

    Ok((source_checksum, target_checksum))
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

    // -------------------------------------------------------------------------
    // describe_source_size tests
    // -------------------------------------------------------------------------

    fn show(bytes: u64) -> String {
        format!("{bytes} B")
    }

    #[test]
    fn test_describe_source_size_shows_the_image_length_of_a_plain_source() {
        let info = engraver_core::SourceInfo::local("image.iso", 4096);

        assert_eq!(describe_source_size(&info, show).as_deref(), Some("4096 B"));
    }

    #[test]
    fn test_describe_source_size_labels_the_size_of_a_compressed_source_as_compressed() {
        let info = engraver_core::SourceInfo::compressed(
            "image.img.gz",
            1000,
            engraver_core::SourceType::Gzip,
        );

        assert_eq!(
            describe_source_size(&info, show).as_deref(),
            Some("1000 B compressed")
        );
    }

    #[test]
    fn test_describe_source_size_is_none_when_no_length_is_known() {
        let mut info = engraver_core::SourceInfo::local("https://example.com/a.iso", 0);
        info.size = None;
        info.compressed_size = None;

        assert_eq!(describe_source_size(&info, show), None);
    }

    // -------------------------------------------------------------------------
    // checksum_image_and_target tests
    // -------------------------------------------------------------------------

    /// An image whose length is not a multiple of the block size
    fn image() -> Vec<u8> {
        (0..3 * 4096 + 17)
            .map(|i: usize| (i * 7 % 251) as u8)
            .collect()
    }

    fn image_then_other_data(image: &[u8], extra: usize) -> Vec<u8> {
        let mut drive = image.to_vec();
        drive.extend(std::iter::repeat_n(0xEE, extra));
        drive
    }

    fn gzip_file(dir: &tempfile::TempDir, image: &[u8]) -> String {
        use std::io::Write;

        let path = dir.path().join("image.img.gz");
        let mut encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        encoder.write_all(image).unwrap();
        encoder.finish().unwrap();
        path.to_str().unwrap().to_string()
    }

    fn checksum_of(data: &[u8]) -> String {
        Verifier::new()
            .calculate_checksum(
                &mut std::io::Cursor::new(data),
                ChecksumAlgorithm::Sha256,
                None,
            )
            .unwrap()
            .to_hex()
    }

    fn not_cancelled() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn test_checksum_image_and_target_matches_a_compressed_image_on_a_larger_drive() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = std::io::Cursor::new(image_then_other_data(&image, 1024 * 1024));

        let (source_checksum, target_checksum) = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut drive,
            None,
            4096,
            &not_cancelled(),
            true,
        )
        .unwrap();

        assert_eq!(source_checksum.to_hex(), checksum_of(&image));
        assert!(target_checksum.matches(&source_checksum));
    }

    #[test]
    fn test_checksum_image_and_target_differs_when_a_byte_of_the_image_is_wrong() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = image_then_other_data(&image, 4096);
        drive[5000] ^= 0xFF;

        let (source_checksum, target_checksum) = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut std::io::Cursor::new(drive),
            None,
            4096,
            &not_cancelled(),
            true,
        )
        .unwrap();

        assert!(!target_checksum.matches(&source_checksum));
    }

    #[test]
    fn test_checksum_image_and_target_accepts_the_length_it_expects() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = std::io::Cursor::new(image_then_other_data(&image, 4096));

        let (source_checksum, target_checksum) = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut drive,
            Some(image.len() as u64),
            4096,
            &not_cancelled(),
            true,
        )
        .unwrap();

        assert!(target_checksum.matches(&source_checksum));
    }

    #[test]
    fn test_checksum_image_and_target_fails_when_the_image_is_not_the_expected_length() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = std::io::Cursor::new(image_then_other_data(&image, 4096));

        let err = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut drive,
            Some(image.len() as u64 + 1),
            4096,
            &not_cancelled(),
            true,
        )
        .expect_err("an image of another length than expected must fail");

        let message = err.to_string();
        assert!(message.contains("once decompressed"), "{message}");
        assert!(message.contains(&image.len().to_string()), "{message}");
    }

    #[test]
    fn test_checksum_image_and_target_fails_when_the_target_is_shorter_than_the_image() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = std::io::Cursor::new(image[..image.len() - 1].to_vec());

        let err = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut drive,
            None,
            4096,
            &not_cancelled(),
            true,
        )
        .expect_err("a target shorter than the image must fail");

        assert!(err.to_string().contains("shorter than the image"), "{err}");
    }

    #[test]
    fn test_checksum_image_and_target_with_a_pending_cancel_returns_a_cancelled_error() {
        let dir = tempfile::tempdir().unwrap();
        let image = image();
        let source = gzip_file(&dir, &image);
        let mut drive = std::io::Cursor::new(image_then_other_data(&image, 4096));
        let cancel = Arc::new(AtomicBool::new(true));

        let err = checksum_image_and_target(
            &source,
            &SourceOptions::default(),
            &mut drive,
            None,
            4096,
            &cancel,
            true,
        )
        .expect_err("a pending cancel must stop the checksum");

        assert_cancelled(&err);
        assert_eq!(err.to_string(), "Verification cancelled.");
    }

    // -------------------------------------------------------------------------
    // Lead-up to a confirmation prompt
    // -------------------------------------------------------------------------

    #[test]
    fn test_hides_lead_up_only_when_nothing_will_be_asked() {
        // -q: status is hidden but a question is coming, so its context shows
        assert!(!hides_lead_up(true, false));
        // --silent, or -q with --yes: no question, nothing to lead up to
        assert!(hides_lead_up(true, true));
        // No -q: nothing is hidden, asking or not
        assert!(!hides_lead_up(false, false));
        assert!(!hides_lead_up(false, true));
    }

    // -------------------------------------------------------------------------
    // Progress bar creation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_create_progress_bar_silent() {
        let pb = create_progress_bar(Some(1024), "Test", true);
        // Silent progress bar should be hidden
        assert!(pb.is_hidden());
    }

    #[test]
    fn test_create_progress_bar_with_size() {
        let pb = create_progress_bar(Some(1024 * 1024), "Writing", false);
        // Progress bar may be hidden in test environment without a terminal
        // Just verify it was created with the right length
        assert_eq!(pb.length(), Some(1024 * 1024));
    }

    #[test]
    fn test_create_progress_bar_unknown_size() {
        let pb = create_progress_bar(None, "Downloading", false);
        // Spinner doesn't have a length - just verify creation succeeds
        assert!(pb.length().is_none() || pb.length() == Some(0));
    }
}
