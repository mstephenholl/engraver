//! Resume support for interrupted write operations
//!
//! This module provides checkpoint-based resume functionality for write operations.
//! When a write is interrupted (Ctrl+C, power failure, etc.), the checkpoint file
//! allows resuming from the last successfully written block.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use engraver_core::resume::{CheckpointManager, TargetIdentity, WriteCheckpoint};
//! use engraver_core::{WriteConfig, SourceInfo};
//!
//! // Start a new write with checkpointing
//! let manager = CheckpointManager::new("/path/to/checkpoints")?;
//! let source_info = SourceInfo::local("/path/to/image.iso", 1024 * 1024);
//! let config = WriteConfig::new();
//! let target = TargetIdentity {
//!     size: 32_000_000_000,
//!     serial: Some("0123456789".to_string()),
//!     model: Some("Flash Drive".to_string()),
//! };
//! let mut checkpoint = WriteCheckpoint::new(&source_info, "/dev/sdb", &target, &config);
//!
//! // ... write blocks, periodically calling:
//! manager.save(&checkpoint)?;
//!
//! // On successful completion:
//! manager.remove(&checkpoint)?;
//! # Ok(())
//! # }
//! ```

use crate::{Error, Result, SourceInfo, SourceType, WriteConfig};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Current version of the checkpoint format
pub const CHECKPOINT_VERSION: u32 = 1;

/// Default checkpoint directory name
pub const CHECKPOINT_DIR_NAME: &str = "engraver";

/// Checkpoint file extension
pub const CHECKPOINT_EXTENSION: &str = "checkpoint";

/// What identifies the drive a write was started on.
///
/// A device path is not an identity: a different stick can show up at
/// `/dev/sdb` after a replug. A checkpoint stores this so a resume can tell
/// the drive it was made for from another one at the same path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentity {
    /// Size of the device in bytes
    pub size: u64,
    /// Serial number, when the platform reports one
    pub serial: Option<String>,
    /// Model name, when the platform reports one
    pub model: Option<String>,
}

/// A checkpoint representing the state of an interrupted write operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteCheckpoint {
    /// Checkpoint format version (for future compatibility)
    pub version: u32,

    /// Unique identifier for this write session
    pub session_id: String,

    // ── Source Information ──────────────────────────────────────────────────
    /// Path to the source file or URL
    pub source_path: String,

    /// Type of source (local, remote, compressed, etc.)
    pub source_type: SourceType,

    /// Size of the source in bytes (if known)
    pub source_size: Option<u64>,

    /// SHA-256 hex digest of the first chunk of source content, computed
    /// at checkpoint creation. `validate_checkpoint` compares this against
    /// the current source's hash to detect when a user has replaced the
    /// source file with a different image of the same size at the same
    /// path. `None` when the caller did not provide a hash.
    #[serde(default)]
    pub source_header_hash: Option<String>,

    /// ETag captured from the source at checkpoint creation. Used to
    /// detect upstream changes for remote sources (HTTP / cloud storage).
    /// `None` when the source has no etag (local files, or the server
    /// did not return one).
    #[serde(default)]
    pub etag: Option<String>,

    /// Whether the source is seekable
    pub source_seekable: bool,

    /// Whether the source supports resume (HTTP Range requests)
    pub source_resumable: bool,

    // ── Target Information ──────────────────────────────────────────────────
    /// Path to the target device
    pub target_path: String,

    /// Size of the target device in bytes
    pub target_size: u64,

    /// Serial number of the target device at checkpoint creation, when it
    /// had one. `None` for checkpoints written by engraver 0.1.x.
    #[serde(default)]
    pub target_serial: Option<String>,

    /// Model name of the target device at checkpoint creation, when it had
    /// one. `None` for checkpoints written by engraver 0.1.x.
    #[serde(default)]
    pub target_model: Option<String>,

    // ── Write Configuration ─────────────────────────────────────────────────
    /// Block size used for writing
    pub block_size: usize,

    /// Full write configuration
    pub config: WriteConfigCheckpoint,

    // ── Progress State ──────────────────────────────────────────────────────
    /// Number of bytes successfully written
    pub bytes_written: u64,

    /// Number of blocks successfully written
    pub blocks_written: u64,

    /// Total number of blocks to write (if known)
    pub total_blocks: Option<u64>,

    // ── Timing Information ──────────────────────────────────────────────────
    /// When the write operation started (Unix timestamp)
    pub start_time: u64,

    /// When this checkpoint was last updated (Unix timestamp)
    pub last_update: u64,

    /// Total elapsed time before interruption (in seconds)
    pub elapsed_seconds: f64,

    // ── Retry Information ───────────────────────────────────────────────────
    /// Number of times this write has been resumed
    pub resume_count: u32,

    /// Total number of block retries across all attempts
    pub total_retries: u32,
}

/// Serializable subset of WriteConfig for checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConfigCheckpoint {
    /// Block size in bytes
    pub block_size: usize,
    /// Sync after each block
    pub sync_each_block: bool,
    /// Sync on completion
    pub sync_on_complete: bool,
    /// Max retry attempts per block
    pub retry_attempts: u32,
    /// Whether verification was requested
    pub verify: bool,
}

impl From<&WriteConfig> for WriteConfigCheckpoint {
    fn from(config: &WriteConfig) -> Self {
        Self {
            block_size: config.block_size,
            sync_each_block: config.sync_each_block,
            sync_on_complete: config.sync_on_complete,
            retry_attempts: config.retry_attempts,
            verify: config.verify,
        }
    }
}

impl WriteCheckpoint {
    /// Create a new checkpoint for a write operation
    pub fn new(
        source_info: &SourceInfo,
        target_path: &str,
        target: &TargetIdentity,
        config: &WriteConfig,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Generate a unique session ID
        let session_id = format!("{:x}-{:x}", now, std::process::id());

        // Determine source properties
        let source_seekable = matches!(source_info.source_type, SourceType::LocalFile);
        let source_resumable =
            matches!(source_info.source_type, SourceType::Remote) && source_info.resumable;

        let total_blocks = source_info
            .size
            .map(|size| size.div_ceil(config.block_size as u64));

        Self {
            version: CHECKPOINT_VERSION,
            session_id,
            source_path: source_info.path.clone(),
            source_type: source_info.source_type,
            source_size: source_info.size,
            source_header_hash: source_info.source_header_hash.clone(),
            etag: source_info.etag.clone(),
            source_seekable,
            source_resumable,
            target_path: target_path.to_string(),
            target_size: target.size,
            target_serial: target.serial.clone(),
            target_model: target.model.clone(),
            block_size: config.block_size,
            config: WriteConfigCheckpoint::from(config),
            bytes_written: 0,
            blocks_written: 0,
            total_blocks,
            start_time: now,
            last_update: now,
            elapsed_seconds: 0.0,
            resume_count: 0,
            total_retries: 0,
        }
    }

    /// Update progress in the checkpoint
    pub fn update_progress(&mut self, bytes_written: u64, blocks_written: u64, elapsed: Duration) {
        self.bytes_written = bytes_written;
        self.blocks_written = blocks_written;
        self.elapsed_seconds = elapsed.as_secs_f64();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Mark this checkpoint as resumed
    pub fn mark_resumed(&mut self) {
        self.resume_count += 1;
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Add retry count
    pub fn add_retries(&mut self, count: u32) {
        self.total_retries += count;
    }

    /// Check if the source can be resumed
    pub fn can_resume(&self) -> bool {
        // Can resume if source is seekable OR if source is resumable (HTTP Range)
        self.source_seekable || self.source_resumable
    }

    /// Get percentage complete
    pub fn percentage(&self) -> f64 {
        match self.source_size {
            Some(total) if total > 0 => (self.bytes_written as f64 / total as f64) * 100.0,
            _ => 0.0,
        }
    }

    /// Get the checkpoint filename for this session
    ///
    /// A drive with a serial is named by the serial, and one without by the path
    /// it was at, as `checkpoint_filename` explains.
    pub fn filename(&self) -> String {
        checkpoint_filename(
            &self.source_path,
            &self.target_path,
            self.target_serial.as_deref(),
        )
    }
}

/// Manages checkpoint files for resume support
pub struct CheckpointManager {
    /// Directory where checkpoints are stored
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    /// Create a new checkpoint manager with the given directory
    pub fn new<P: AsRef<Path>>(checkpoint_dir: P) -> Result<Self> {
        let checkpoint_dir = checkpoint_dir.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if !checkpoint_dir.exists() {
            fs::create_dir_all(&checkpoint_dir).map_err(|e| {
                Error::Io(std::io::Error::other(format!(
                    "Failed to create checkpoint directory: {}",
                    e
                )))
            })?;
        }

        Ok(Self { checkpoint_dir })
    }

    /// Create a checkpoint manager using the default system directory
    pub fn default_location() -> Result<Self> {
        let dir = default_checkpoint_dir()?;
        Self::new(dir)
    }

    /// Get the path to a checkpoint file
    pub fn checkpoint_path(&self, checkpoint: &WriteCheckpoint) -> PathBuf {
        self.checkpoint_dir.join(checkpoint.filename())
    }

    /// Save a checkpoint to disk
    pub fn save(&self, checkpoint: &WriteCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(checkpoint);
        let temp_path = path.with_extension("tmp");

        // Write to temp file first, then rename (atomic on most systems).
        //
        // The rename comes last and only after the bytes are on disk. Dropping a
        // BufWriter ignores a failed flush, and a rename does not make the data
        // durable, so the buffer is taken back with into_inner (which reports a
        // failed flush) and the file is synced before it replaces the old
        // checkpoint.
        let file = fs::File::create(&temp_path).map_err(Error::Io)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, checkpoint)
            .map_err(|e| Error::Io(std::io::Error::from(e)))?;
        let file = writer.into_inner().map_err(|e| Error::Io(e.into_error()))?;
        file.sync_all().map_err(Error::Io)?;
        drop(file);

        fs::rename(&temp_path, &path).map_err(Error::Io)?;

        tracing::debug!("Saved checkpoint to {:?}", path);
        Ok(())
    }

    /// Load a checkpoint from disk
    pub fn load(&self, checkpoint: &WriteCheckpoint) -> Result<WriteCheckpoint> {
        let path = self.checkpoint_path(checkpoint);
        self.load_from_path(&path)
    }

    /// Load a checkpoint from a specific path
    pub fn load_from_path(&self, path: &Path) -> Result<WriteCheckpoint> {
        let file = fs::File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let checkpoint: WriteCheckpoint = serde_json::from_reader(reader).map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse checkpoint: {}", e),
            ))
        })?;

        // Version check
        if checkpoint.version > CHECKPOINT_VERSION {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Checkpoint version {} is newer than supported version {}",
                    checkpoint.version, CHECKPOINT_VERSION
                ),
            )));
        }

        Ok(checkpoint)
    }

    /// Remove a checkpoint file
    pub fn remove(&self, checkpoint: &WriteCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(checkpoint);
        if path.exists() {
            fs::remove_file(&path).map_err(Error::Io)?;
            tracing::debug!("Removed checkpoint {:?}", path);
        }
        Ok(())
    }

    /// Find an existing checkpoint for a source/target combination
    ///
    /// With `target_serial`, the drive's own checkpoint is looked up first and,
    /// when there is none, the checkpoint named after the path alone: one from
    /// 0.1.x, or one for a drive that reports no serial. Without it, only that
    /// path-named checkpoint is looked up, since a checkpoint of a drive with a
    /// serial is never named by its path.
    ///
    /// Returns `Ok(None)` when there is none. A checkpoint file that exists but
    /// cannot be loaded (damaged, or written by a newer engraver) is an error and
    /// is left in place.
    pub fn find_checkpoint(
        &self,
        source_path: &str,
        target_path: &str,
        target_serial: Option<&str>,
    ) -> Result<Option<WriteCheckpoint>> {
        let own = known_serial(target_serial).map(Some);
        for serial in own.into_iter().chain([None]) {
            let path =
                self.checkpoint_dir
                    .join(checkpoint_filename(source_path, target_path, serial));
            if !path.exists() {
                continue;
            }

            // A file that cannot be loaded stays where it is. It may be a
            // checkpoint from a newer engraver, and deleting it would destroy the
            // only record of an interrupted write. The caller decides what to do
            // with the error.
            return self.load_from_path(&path).map(Some).map_err(|e| match e {
                Error::Io(io) => Error::Io(std::io::Error::new(
                    io.kind(),
                    format!("{}: {io}", path.display()),
                )),
                other => other,
            });
        }

        Ok(None)
    }

    /// List all checkpoints in the directory
    pub fn list_checkpoints(&self) -> Result<Vec<WriteCheckpoint>> {
        let mut checkpoints = Vec::new();

        let entries = fs::read_dir(&self.checkpoint_dir).map_err(Error::Io)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == CHECKPOINT_EXTENSION) {
                match self.load_from_path(&path) {
                    Ok(checkpoint) => checkpoints.push(checkpoint),
                    Err(e) => {
                        tracing::warn!("Failed to load checkpoint {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by last update time (most recent first)
        checkpoints.sort_by_key(|c| std::cmp::Reverse(c.last_update));

        Ok(checkpoints)
    }

    /// Clean up old checkpoints (older than the given duration)
    pub fn cleanup_old(&self, max_age: Duration) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let max_age_secs = max_age.as_secs();
        let mut removed = 0;

        let entries = fs::read_dir(&self.checkpoint_dir).map_err(Error::Io)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == CHECKPOINT_EXTENSION) {
                if let Ok(checkpoint) = self.load_from_path(&path) {
                    if now.saturating_sub(checkpoint.last_update) > max_age_secs
                        && fs::remove_file(&path).is_ok()
                    {
                        removed += 1;
                        tracing::debug!("Cleaned up old checkpoint {:?}", path);
                    }
                }
            }
        }

        Ok(removed)
    }
}

/// Get the default checkpoint directory for the current platform
pub fn default_checkpoint_dir() -> Result<PathBuf> {
    // Try XDG_STATE_HOME first (Linux), then fallback to home directory
    let base = if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(state_home)
    } else if let Some(home) = dirs_next::home_dir() {
        #[cfg(unix)]
        {
            home.join(".local").join("state")
        }
        #[cfg(windows)]
        {
            home.join("AppData").join("Local")
        }
        #[cfg(not(any(unix, windows)))]
        {
            home
        }
    } else {
        // Fallback to temp directory
        std::env::temp_dir()
    };

    Ok(base.join(CHECKPOINT_DIR_NAME).join("checkpoints"))
}

/// Simple hash function for generating checkpoint filenames
fn simple_hash(s: &str) -> u64 {
    // FNV-1a hash
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// A serial that tells one drive from another. An empty one tells nothing.
fn known_serial(serial: Option<&str>) -> Option<&str> {
    serial.filter(|s| !s.is_empty())
}

/// The name of the checkpoint file for a source written to a target
///
/// A drive with a serial is named by the source and the serial, and not by the
/// path: two sticks that show up at `/dev/sdb` in turn have two files, and a
/// stick plugged back in at `/dev/sdc` finds the one it had. A drive with no
/// serial is named by the source and the path, as 0.1.x named every checkpoint,
/// so those files are still found.
fn checkpoint_filename(
    source_path: &str,
    target_path: &str,
    target_serial: Option<&str>,
) -> String {
    let key = match known_serial(target_serial) {
        Some(serial) => format!("{source_path}:serial={serial}"),
        None => format!("{source_path}:{target_path}"),
    };
    format!("{:016x}.{}", simple_hash(&key), CHECKPOINT_EXTENSION)
}

/// Validation result for a checkpoint
#[derive(Debug)]
pub struct CheckpointValidation {
    /// Is the checkpoint valid for resuming?
    pub valid: bool,
    /// Detailed validation messages
    pub messages: Vec<String>,
    /// Warnings that don't prevent resuming
    pub warnings: Vec<String>,
    /// Is the checkpoint invalid because the target is not the drive it was made
    /// for (a different size, serial or model)? Such a checkpoint is still good
    /// for that drive, so the caller may keep it. Never set for a problem with
    /// the source.
    pub for_another_drive: bool,
}

impl CheckpointValidation {
    /// Create a new valid result
    pub fn valid() -> Self {
        Self {
            valid: true,
            messages: Vec::new(),
            warnings: Vec::new(),
            for_another_drive: false,
        }
    }

    /// Create a new invalid result with a message
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            valid: false,
            messages: vec![message.into()],
            warnings: Vec::new(),
            for_another_drive: false,
        }
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Add an error and mark as invalid
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.valid = false;
        self.messages.push(error.into());
        self
    }

    /// Add an error that says the checkpoint is for another drive
    fn with_other_drive_error(mut self, error: impl Into<String>) -> Self {
        self.for_another_drive = true;
        self.with_error(error)
    }
}

/// A drive property for a message: quoted when known, `unknown` when not
fn describe(value: &Option<String>) -> String {
    value
        .as_ref()
        .map_or_else(|| "unknown".to_string(), |v| format!("'{v}'"))
}

/// Validate a checkpoint against current source/target state
///
/// The target is invalid when its size, or its serial or model where both are
/// known, differ from the drive the checkpoint was made for, and the result then
/// has `for_another_drive` set. An unknown serial on either side is valid with a
/// warning.
pub fn validate_checkpoint(
    checkpoint: &WriteCheckpoint,
    source_info: &SourceInfo,
    target: &TargetIdentity,
) -> CheckpointValidation {
    let mut result = CheckpointValidation::valid();

    // Check source path matches
    if checkpoint.source_path != source_info.path {
        return CheckpointValidation::invalid(format!(
            "Source path mismatch: checkpoint has '{}', current is '{}'",
            checkpoint.source_path, source_info.path
        ));
    }

    // Check source size (if known)
    if let (Some(cp_size), Some(src_size)) = (checkpoint.source_size, source_info.size) {
        if cp_size != src_size {
            return CheckpointValidation::invalid(format!(
                "Source size changed: checkpoint has {} bytes, current is {} bytes",
                cp_size, src_size
            ));
        }
    }

    // Check the target is the drive the checkpoint was made for. A device path
    // does not say which drive sits behind it, so compare what the drive reports
    // about itself. A different drive would get only the tail of the image, with
    // the first part of it never written, so a mismatch makes the checkpoint
    // invalid.
    if checkpoint.target_size != target.size {
        result = result.with_other_drive_error(format!(
            "Target size changed: checkpoint has {} bytes, current is {} bytes",
            checkpoint.target_size, target.size
        ));
    }

    match (&checkpoint.target_serial, &target.serial) {
        (Some(cp_serial), Some(cur_serial)) if cp_serial != cur_serial => {
            result = result.with_other_drive_error(format!(
                "Target serial changed: checkpoint has '{cp_serial}', current is '{cur_serial}'"
            ));
        }
        (Some(_), Some(_)) => {}
        (cp_serial, cur_serial) => {
            // Checkpoints from 0.1.x have no serial, and some drives report none.
            // Resuming is allowed, since refusing would strand every old
            // checkpoint, but the user is told the drive is unconfirmed.
            result = result.with_warning(format!(
                "Target serial unknown: checkpoint has {}, current is {}; \
                 cannot confirm this is the drive the checkpoint was made for",
                describe(cp_serial),
                describe(cur_serial)
            ));
        }
    }

    if let (Some(cp_model), Some(cur_model)) = (&checkpoint.target_model, &target.model) {
        if cp_model != cur_model {
            result = result.with_other_drive_error(format!(
                "Target model changed: checkpoint has '{cp_model}', current is '{cur_model}'"
            ));
        }
    }

    // Check if source can be resumed
    if !checkpoint.can_resume() {
        return CheckpointValidation::invalid(
            "This source does not support resume: it is compressed, or its server does not accept Range requests",
        );
    }

    // Check bytes_written doesn't exceed source size
    if let Some(source_size) = checkpoint.source_size {
        if checkpoint.bytes_written > source_size {
            return CheckpointValidation::invalid(format!(
                "Checkpoint bytes_written ({}) exceeds source size ({})",
                checkpoint.bytes_written, source_size
            ));
        }
    }

    // Content identity: when both checkpoint and current source have a
    // header-content hash, they must match. A mismatch means the user
    // has pointed the writer at a different image (same path, same size,
    // different bytes) — resuming would mix data from two images.
    //
    // When either side lacks a hash (old checkpoint, or HTTP source where
    // computing it would require an extra range request), we skip this
    // check rather than block resume — the path/size/etag checks remain.
    if let (Some(cp_hash), Some(cur_hash)) = (
        &checkpoint.source_header_hash,
        &source_info.source_header_hash,
    ) {
        if cp_hash != cur_hash {
            return CheckpointValidation::invalid(format!(
                "Source content has changed (header hash mismatch): \
                 checkpoint = {}, current = {}",
                cp_hash, cur_hash
            ));
        }
    }

    // Etag identity: for remote sources, an upstream content change
    // changes the etag. Mismatch means we'd be resuming a different
    // remote object than the one we started from.
    if let (Some(cp_etag), Some(cur_etag)) = (&checkpoint.etag, &source_info.etag) {
        if cp_etag != cur_etag {
            return CheckpointValidation::invalid(format!(
                "Remote source has changed (etag mismatch): \
                 checkpoint = {}, current = {}",
                cp_etag, cur_etag
            ));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_source_info() -> SourceInfo {
        SourceInfo {
            path: "/path/to/image.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(1024 * 1024 * 100), // 100 MB
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        }
    }

    fn create_test_config() -> WriteConfig {
        WriteConfig::new().block_size(4 * 1024 * 1024) // 4 MB blocks
    }

    /// A target that is known only by its size, as a drive with no serial.
    fn target(size: u64) -> TargetIdentity {
        TargetIdentity {
            size,
            serial: None,
            model: None,
        }
    }

    #[test]
    fn test_checkpoint_creation() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        assert_eq!(checkpoint.version, CHECKPOINT_VERSION);
        assert_eq!(checkpoint.source_path, "/path/to/image.iso");
        assert_eq!(checkpoint.target_path, "/dev/sdb");
        assert_eq!(checkpoint.bytes_written, 0);
        assert_eq!(checkpoint.blocks_written, 0);
        assert!(checkpoint.source_seekable);
        assert!(!checkpoint.source_resumable);
    }

    #[test]
    fn test_checkpoint_progress_update() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        checkpoint.update_progress(50 * 1024 * 1024, 12, Duration::from_secs(10));

        assert_eq!(checkpoint.bytes_written, 50 * 1024 * 1024);
        assert_eq!(checkpoint.blocks_written, 12);
        assert!((checkpoint.elapsed_seconds - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_checkpoint_percentage() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // 0% initially
        assert!((checkpoint.percentage() - 0.0).abs() < 0.001);

        // 50% after writing half
        checkpoint.bytes_written = 50 * 1024 * 1024;
        assert!((checkpoint.percentage() - 50.0).abs() < 0.001);

        // 100% after writing all
        checkpoint.bytes_written = 100 * 1024 * 1024;
        assert!((checkpoint.percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_checkpoint_can_resume() {
        let config = create_test_config();

        // Local file - can resume (seekable)
        let local_info = SourceInfo {
            path: "/path/to/file.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(1024),
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint =
            WriteCheckpoint::new(&local_info, "/dev/sdb", &target(1024 * 1024), &config);
        assert!(checkpoint.can_resume());

        // HTTP - can resume (resumable via Range)
        let http_info = SourceInfo {
            path: "https://example.com/file.iso".to_string(),
            source_type: SourceType::Remote,
            size: Some(1024),
            min_size: None,
            compressed_size: None,
            seekable: false,
            resumable: true,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint =
            WriteCheckpoint::new(&http_info, "/dev/sdb", &target(1024 * 1024), &config);
        assert!(checkpoint.can_resume());

        // Gzip - cannot resume (not seekable, not resumable)
        let gzip_info = SourceInfo {
            path: "/path/to/file.iso.gz".to_string(),
            source_type: SourceType::Gzip,
            size: Some(1024),
            min_size: None,
            compressed_size: Some(512),
            seekable: false,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint =
            WriteCheckpoint::new(&gzip_info, "/dev/sdb", &target(1024 * 1024), &config);
        assert!(!checkpoint.can_resume());
    }

    #[test]
    fn test_checkpoint_filename() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);

        let filename = checkpoint.filename();
        assert!(filename.ends_with(".checkpoint"));
        // Should be consistent for same source/target
        let checkpoint2 =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);
        assert_eq!(checkpoint.filename(), checkpoint2.filename());
    }

    #[test]
    fn test_checkpoint_serialization() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        checkpoint.update_progress(1024 * 1024, 1, Duration::from_secs(1));

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        assert!(json.contains("\"source_path\""));
        assert!(json.contains("\"bytes_written\""));

        // Deserialize back
        let loaded: WriteCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.source_path, checkpoint.source_path);
        assert_eq!(loaded.bytes_written, checkpoint.bytes_written);
    }

    #[test]
    fn test_validate_checkpoint_valid() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        let result =
            validate_checkpoint(&checkpoint, &source_info, &target(32 * 1024 * 1024 * 1024));
        assert!(result.valid);
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_validate_checkpoint_source_changed() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Different source size
        let mut changed_info = source_info.clone();
        changed_info.size = Some(200 * 1024 * 1024);

        let result =
            validate_checkpoint(&checkpoint, &changed_info, &target(32 * 1024 * 1024 * 1024));
        assert!(!result.valid);
        assert!(result.messages[0].contains("size changed"));
    }

    #[test]
    fn test_validate_checkpoint_compressed_source() {
        let gzip_info = SourceInfo {
            path: "/path/to/file.iso.gz".to_string(),
            source_type: SourceType::Gzip,
            size: Some(1024),
            min_size: None,
            compressed_size: Some(512),
            seekable: false,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let config = create_test_config();
        let checkpoint =
            WriteCheckpoint::new(&gzip_info, "/dev/sdb", &target(1024 * 1024), &config);

        let result = validate_checkpoint(&checkpoint, &gzip_info, &target(1024 * 1024));
        assert!(!result.valid);
        assert!(result.messages[0].contains("does not support resume"));
    }

    #[test]
    fn test_simple_hash_consistency() {
        let hash1 = simple_hash("test string");
        let hash2 = simple_hash("test string");
        assert_eq!(hash1, hash2);

        let hash3 = simple_hash("different string");
        assert_ne!(hash1, hash3);
    }

    // -------------------------------------------------------------------------
    // CheckpointManager tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_checkpoint_manager_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        // Directory should exist
        assert!(temp_dir.path().exists());

        // Manager should be functional
        let checkpoints = manager.list_checkpoints().unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_checkpoint_manager_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        checkpoint.update_progress(50 * 1024 * 1024, 12, Duration::from_secs(10));

        // Save checkpoint
        manager.save(&checkpoint).unwrap();

        // Load it back
        let loaded = manager.load(&checkpoint).unwrap();
        assert_eq!(loaded.source_path, checkpoint.source_path);
        assert_eq!(loaded.bytes_written, checkpoint.bytes_written);
        assert_eq!(loaded.blocks_written, checkpoint.blocks_written);
    }

    #[test]
    fn test_checkpoint_filename_without_a_serial_is_unchanged_from_0_1() {
        // FNV-1a of "/path/to/image.iso:/dev/sdb", as 0.1.x named it. A changed
        // name would strand the checkpoints those versions left behind.
        let checkpoint = WriteCheckpoint::new(
            &create_test_source_info(),
            "/dev/sdb",
            &target(1024),
            &create_test_config(),
        );

        assert_eq!(checkpoint.filename(), "4fe336349dc37360.checkpoint");
    }

    #[test]
    fn test_checkpoint_filename_with_a_serial_ignores_the_path() {
        let drive = |serial: Option<&str>| TargetIdentity {
            size: 1024,
            serial: serial.map(str::to_string),
            model: None,
        };
        let name = |path: &str, serial: Option<&str>| {
            WriteCheckpoint::new(
                &create_test_source_info(),
                path,
                &drive(serial),
                &create_test_config(),
            )
            .filename()
        };

        assert_eq!(
            name("/dev/sdb", Some("AAAA")),
            name("/dev/sdc", Some("AAAA"))
        );
        assert_ne!(
            name("/dev/sdb", Some("AAAA")),
            name("/dev/sdb", Some("BBBB"))
        );
        assert_ne!(name("/dev/sdb", Some("AAAA")), name("/dev/sdb", None));
        // An empty serial tells nothing, so it is no serial.
        assert_eq!(name("/dev/sdb", Some("")), name("/dev/sdb", None));
        assert_ne!(name("/dev/sdb", Some("")), name("/dev/sdc", Some("")));
    }

    #[test]
    fn test_checkpoint_manager_find_checkpoint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Save checkpoint
        manager.save(&checkpoint).unwrap();

        // Find it
        let found = manager
            .find_checkpoint("/path/to/image.iso", "/dev/sdb", None)
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().source_path, "/path/to/image.iso");

        // Try to find non-existent
        let not_found = manager
            .find_checkpoint("/different/path.iso", "/dev/sdc", None)
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_checkpoint_manager_remove() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Save checkpoint
        manager.save(&checkpoint).unwrap();

        // Verify it exists
        let path = manager.checkpoint_path(&checkpoint);
        assert!(path.exists());

        // Remove it
        manager.remove(&checkpoint).unwrap();

        // Verify it's gone
        assert!(!path.exists());
    }

    #[test]
    fn test_checkpoint_manager_list_checkpoints() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        // Create and save multiple checkpoints
        let config = create_test_config();

        let source_info1 = SourceInfo {
            path: "/path/to/image1.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(100 * 1024 * 1024),
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint1 = WriteCheckpoint::new(
            &source_info1,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        manager.save(&checkpoint1).unwrap();

        let source_info2 = SourceInfo {
            path: "/path/to/image2.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(200 * 1024 * 1024),
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint2 = WriteCheckpoint::new(
            &source_info2,
            "/dev/sdc",
            &target(64 * 1024 * 1024 * 1024),
            &config,
        );
        manager.save(&checkpoint2).unwrap();

        // List should return both
        let checkpoints = manager.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 2);
    }

    #[test]
    fn test_checkpoint_manager_cleanup_old() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        manager.save(&checkpoint).unwrap();

        // Cleanup with 1 hour max age - checkpoint is fresh, shouldn't be removed
        let removed = manager.cleanup_old(Duration::from_secs(3600)).unwrap();
        assert_eq!(removed, 0);

        // Checkpoint should still exist
        let checkpoints = manager.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 1);
    }

    // -------------------------------------------------------------------------
    // WriteCheckpoint additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_checkpoint_mark_resumed() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        assert_eq!(checkpoint.resume_count, 0);

        checkpoint.mark_resumed();
        assert_eq!(checkpoint.resume_count, 1);

        checkpoint.mark_resumed();
        assert_eq!(checkpoint.resume_count, 2);
    }

    #[test]
    fn test_checkpoint_add_retries() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        assert_eq!(checkpoint.total_retries, 0);

        checkpoint.add_retries(3);
        assert_eq!(checkpoint.total_retries, 3);

        checkpoint.add_retries(2);
        assert_eq!(checkpoint.total_retries, 5);
    }

    // -------------------------------------------------------------------------
    // CheckpointValidation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_checkpoint_validation_valid_creation() {
        let result = CheckpointValidation::valid();
        assert!(result.valid);
        assert!(result.messages.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_checkpoint_validation_invalid_creation() {
        let result = CheckpointValidation::invalid("Source not found");
        assert!(!result.valid);
        assert_eq!(result.messages.len(), 1);
        assert!(result.messages[0].contains("Source not found"));
    }

    #[test]
    fn test_checkpoint_validation_with_warning() {
        let result = CheckpointValidation::valid().with_warning("Target size changed");
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_checkpoint_validation_with_error() {
        let result = CheckpointValidation::valid().with_error("Critical failure");
        assert!(!result.valid);
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn test_validate_checkpoint_path_mismatch() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Create source with different path
        let different_source = SourceInfo {
            path: "/different/path.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(100 * 1024 * 1024),
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };

        let result = validate_checkpoint(
            &checkpoint,
            &different_source,
            &target(32 * 1024 * 1024 * 1024),
        );
        assert!(!result.valid);
        assert!(result.messages[0].contains("path mismatch"));
    }

    #[test]
    fn test_validate_checkpoint_bytes_exceed_size() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let mut checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Set bytes_written to exceed source size
        checkpoint.bytes_written = 200 * 1024 * 1024; // More than 100 MB source

        let result =
            validate_checkpoint(&checkpoint, &source_info, &target(32 * 1024 * 1024 * 1024));
        assert!(!result.valid);
        assert!(result.messages[0].contains("exceeds source size"));
    }

    #[test]
    fn test_validate_checkpoint_target_size_mismatch_is_invalid() {
        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        // Validate with different target size
        let result =
            validate_checkpoint(&checkpoint, &source_info, &target(64 * 1024 * 1024 * 1024));
        assert!(!result.valid);
        assert!(result.messages[0].contains("Target size changed"));
    }

    // -------------------------------------------------------------------------
    // WriteConfigCheckpoint tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_config_checkpoint_from() {
        let config = WriteConfig::new()
            .block_size(4 * 1024 * 1024)
            .sync_each_block(true)
            .sync_on_complete(true)
            .retry_attempts(5);

        let cp_config = WriteConfigCheckpoint::from(&config);

        assert_eq!(cp_config.block_size, 4 * 1024 * 1024);
        assert!(cp_config.sync_each_block);
        assert!(cp_config.sync_on_complete);
        assert_eq!(cp_config.retry_attempts, 5);
    }

    // -------------------------------------------------------------------------
    // default_checkpoint_dir tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_default_checkpoint_dir() {
        let dir = default_checkpoint_dir();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        // Should end with engraver/checkpoints
        assert!(path.ends_with("checkpoints"));
    }

    // -------------------------------------------------------------------------
    // WriteCheckpoint edge case tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_checkpoint_percentage_no_source_size() {
        let config = create_test_config();
        let source_info = SourceInfo {
            path: "/path/to/stream".to_string(),
            source_type: SourceType::Remote,
            size: None, // unknown size
            min_size: None,
            compressed_size: None,
            seekable: false,
            resumable: true,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let mut checkpoint =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);
        checkpoint.bytes_written = 500;

        // With no source size, percentage should be 0.0
        assert_eq!(checkpoint.percentage(), 0.0);
    }

    #[test]
    fn test_checkpoint_percentage_zero_source_size() {
        let config = create_test_config();
        let source_info = SourceInfo {
            path: "/path/to/empty".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(0),
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);

        assert_eq!(checkpoint.percentage(), 0.0);
    }

    #[test]
    fn test_checkpoint_total_blocks_calculation() {
        let config = create_test_config(); // 4 MB blocks
        let source_info = SourceInfo {
            path: "/path/to/image.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(10 * 1024 * 1024), // 10 MB
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(1024 * 1024 * 1024),
            &config,
        );

        // 10 MB / 4 MB = 3 blocks (ceiling division)
        assert_eq!(checkpoint.total_blocks, Some(3));
    }

    #[test]
    fn test_checkpoint_total_blocks_exact_division() {
        let config = create_test_config(); // 4 MB blocks
        let source_info = SourceInfo {
            path: "/path/to/image.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: Some(8 * 1024 * 1024), // 8 MB
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint = WriteCheckpoint::new(
            &source_info,
            "/dev/sdb",
            &target(1024 * 1024 * 1024),
            &config,
        );

        assert_eq!(checkpoint.total_blocks, Some(2));
    }

    #[test]
    fn test_checkpoint_filename_different_for_different_targets() {
        let source_info = create_test_source_info();
        let config = create_test_config();

        let cp1 = WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);
        let cp2 = WriteCheckpoint::new(&source_info, "/dev/sdc", &target(1024 * 1024), &config);

        assert_ne!(cp1.filename(), cp2.filename());
    }

    #[test]
    fn test_validate_checkpoint_both_sizes_none() {
        let config = create_test_config();
        let source_info = SourceInfo {
            path: "/path/to/image.iso".to_string(),
            source_type: SourceType::LocalFile,
            size: None,
            min_size: None,
            compressed_size: None,
            seekable: true,
            resumable: false,
            content_type: None,
            etag: None,
            source_header_hash: None,
        };
        let checkpoint =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);

        // Should be valid when both sizes are None (can't compare)
        let result = validate_checkpoint(&checkpoint, &source_info, &target(1024 * 1024));
        assert!(result.valid);
    }

    #[test]
    fn test_checkpoint_manager_creates_missing_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_path = temp_dir.path().join("deeply").join("nested").join("dir");

        // Directory doesn't exist yet
        assert!(!nested_path.exists());

        let manager = CheckpointManager::new(&nested_path).unwrap();

        // Should have been created
        assert!(nested_path.exists());

        // Should work normally
        let checkpoints = manager.list_checkpoints().unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_checkpoint_manager_remove_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let source_info = create_test_source_info();
        let config = create_test_config();
        let checkpoint =
            WriteCheckpoint::new(&source_info, "/dev/sdb", &target(1024 * 1024), &config);

        // Removing a checkpoint that was never saved should succeed silently
        manager.remove(&checkpoint).unwrap();
    }

    #[test]
    fn test_simple_hash_empty_string() {
        let hash1 = simple_hash("");
        let hash2 = simple_hash("");
        assert_eq!(hash1, hash2);
        // Should produce the FNV offset basis since no bytes are processed
        // but after XOR with nothing it stays the same
    }

    #[test]
    fn test_simple_hash_different_lengths() {
        let hash1 = simple_hash("a");
        let hash2 = simple_hash("ab");
        let hash3 = simple_hash("abc");
        // All should be different
        assert_ne!(hash1, hash2);
        assert_ne!(hash2, hash3);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_write_config_checkpoint_verify_field() {
        let config = WriteConfig::new().verify(true);
        let cp_config = WriteConfigCheckpoint::from(&config);
        assert!(cp_config.verify);

        let config = WriteConfig::new().verify(false);
        let cp_config = WriteConfigCheckpoint::from(&config);
        assert!(!cp_config.verify);
    }

    // -------------------------------------------------------------------------
    // Content-aware validation (header hash + etag)
    //
    // These guard against the scenario where a user resumes a write but
    // the source image at `source_path` has been replaced by a different
    // file of the same length. Path + size matching alone cannot detect
    // this; a content hash or etag can.
    // -------------------------------------------------------------------------

    fn source_with_hash(hash: &str) -> SourceInfo {
        let mut info = create_test_source_info();
        info.source_header_hash = Some(hash.to_string());
        info
    }

    fn source_with_etag(etag: &str) -> SourceInfo {
        let mut info = create_test_source_info();
        info.etag = Some(etag.to_string());
        info
    }

    #[test]
    fn test_validate_checkpoint_rejects_header_hash_mismatch() {
        let original = source_with_hash("aaaa");
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &original,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        assert_eq!(checkpoint.source_header_hash.as_deref(), Some("aaaa"));

        let replaced = source_with_hash("bbbb");
        let result = validate_checkpoint(&checkpoint, &replaced, &target(32 * 1024 * 1024 * 1024));

        assert!(!result.valid, "must reject when source content differs");
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.contains("content") || m.contains("hash") || m.contains("header")),
            "error message should explain the mismatch: {:?}",
            result.messages
        );
    }

    #[test]
    fn test_validate_checkpoint_accepts_matching_header_hash() {
        let info = source_with_hash("deadbeef");
        let config = create_test_config();
        let checkpoint =
            WriteCheckpoint::new(&info, "/dev/sdb", &target(32 * 1024 * 1024 * 1024), &config);

        let result = validate_checkpoint(&checkpoint, &info, &target(32 * 1024 * 1024 * 1024));
        assert!(
            result.valid,
            "matching hashes must validate: {:?}",
            result.messages
        );
    }

    #[test]
    fn test_validate_checkpoint_passes_when_checkpoint_lacks_hash() {
        // Backwards-compat: a pre-existing checkpoint written before this
        // feature shipped will have `source_header_hash: None`. Such a
        // checkpoint should still validate (with at most a warning) so
        // in-flight resumes don't break when users upgrade.
        let info = create_test_source_info(); // no hash
        let config = create_test_config();
        let mut checkpoint =
            WriteCheckpoint::new(&info, "/dev/sdb", &target(32 * 1024 * 1024 * 1024), &config);
        checkpoint.source_header_hash = None;

        // The current source happens to have a hash now (caller computed it).
        let current = source_with_hash("anything");
        let result = validate_checkpoint(&checkpoint, &current, &target(32 * 1024 * 1024 * 1024));
        assert!(
            result.valid,
            "missing checkpoint hash must not block resume"
        );
    }

    #[test]
    fn test_validate_checkpoint_passes_when_current_lacks_hash() {
        // Symmetric: caller couldn't compute the current hash (e.g. HTTP
        // source where seeking is impossible). Don't fail loudly — fall
        // back to the etag / size checks.
        let original = source_with_hash("aaaa");
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &original,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );

        let mut current = original.clone();
        current.source_header_hash = None;
        let result = validate_checkpoint(&checkpoint, &current, &target(32 * 1024 * 1024 * 1024));
        assert!(result.valid);
    }

    #[test]
    fn test_validate_checkpoint_rejects_etag_mismatch() {
        let original = source_with_etag("v1");
        let config = create_test_config();
        let checkpoint = WriteCheckpoint::new(
            &original,
            "/dev/sdb",
            &target(32 * 1024 * 1024 * 1024),
            &config,
        );
        assert_eq!(checkpoint.etag.as_deref(), Some("v1"));

        let updated = source_with_etag("v2");
        let result = validate_checkpoint(&checkpoint, &updated, &target(32 * 1024 * 1024 * 1024));

        assert!(!result.valid, "must reject when remote source etag differs");
        assert!(
            result.messages.iter().any(|m| m.contains("etag")),
            "error message should mention etag: {:?}",
            result.messages
        );
    }

    #[test]
    fn test_validate_checkpoint_accepts_matching_etag() {
        let info = source_with_etag("abc123");
        let config = create_test_config();
        let checkpoint =
            WriteCheckpoint::new(&info, "/dev/sdb", &target(32 * 1024 * 1024 * 1024), &config);

        let result = validate_checkpoint(&checkpoint, &info, &target(32 * 1024 * 1024 * 1024));
        assert!(
            result.valid,
            "matching etags must validate: {:?}",
            result.messages
        );
    }

    #[test]
    fn test_write_checkpoint_carries_hash_and_etag_from_source_info() {
        let mut info = create_test_source_info();
        info.source_header_hash = Some("h".to_string());
        info.etag = Some("e".to_string());
        let config = create_test_config();
        let cp = WriteCheckpoint::new(&info, "/dev/sdb", &target(1024), &config);
        assert_eq!(cp.source_header_hash.as_deref(), Some("h"));
        assert_eq!(cp.etag.as_deref(), Some("e"));
    }
}
