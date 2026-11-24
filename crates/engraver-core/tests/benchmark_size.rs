use engraver_core::{BenchmarkConfig, BenchmarkProgress, BenchmarkRunner, DataPattern};
use std::io::{self, Seek, SeekFrom, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

#[derive(Default)]
struct ExtentSink {
    position: u64,
    extent: u64,
}

impl Write for ExtentSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.position += buf.len() as u64;
        self.extent = self.extent.max(self.position);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for ExtentSink {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Start(offset) => self.position = offset,
            _ => return Err(io::Error::from(io::ErrorKind::Unsupported)),
        }
        Ok(self.position)
    }
}

fn config(test_size: u64, block_size: u64, passes: u32) -> BenchmarkConfig {
    BenchmarkConfig {
        test_size,
        block_size,
        pattern: DataPattern::Zeros,
        passes,
    }
}

#[test]
fn a_given_size_is_what_the_run_writes_and_reports() {
    for (size, block) in [
        (64 * MIB, 32 * MIB),
        (16 * KIB, 4 * KIB),
        (8 * MIB, 4 * MIB),
    ] {
        let mut sink = ExtentSink::default();

        let result = BenchmarkRunner::new(config(size, block, 1))
            .run(&mut sink, "/dev/test", None::<fn(&BenchmarkProgress)>)
            .unwrap();

        assert_eq!(
            sink.extent, size,
            "bytes written, size {size} block {block}"
        );
        assert_eq!(
            result.test_size, size,
            "test_size, size {size} block {block}"
        );
        assert_eq!(result.passes[0].bytes_written, size);
        assert_eq!(result.summary.total_bytes_written, size);
    }
}

#[test]
fn passes_each_write_the_given_size_and_the_summary_is_their_total() {
    let mut sink = ExtentSink::default();

    let result = BenchmarkRunner::new(config(32 * KIB, 4 * KIB, 3))
        .run(&mut sink, "/dev/test", None::<fn(&BenchmarkProgress)>)
        .unwrap();

    assert_eq!(result.test_size, 32 * KIB);
    assert_eq!(sink.extent, 32 * KIB, "every pass rewrites the same region");
    assert_eq!(result.passes.len(), 3);
    for pass in &result.passes {
        assert_eq!(pass.bytes_written, 32 * KIB);
    }
    assert_eq!(result.summary.total_bytes_written, 3 * 32 * KIB);
}

#[test]
fn a_file_target_ends_up_exactly_as_long_as_the_reported_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drive.img");
    let file = std::fs::File::create(&path).unwrap();

    let result = BenchmarkRunner::new(config(16 * KIB, 4 * KIB, 1))
        .run(file, "/dev/test", None::<fn(&BenchmarkProgress)>)
        .unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), result.test_size);
    assert_eq!(result.test_size, 16 * KIB);
}

#[test]
fn a_size_smaller_than_one_block_is_refused_before_anything_is_written() {
    let mut sink = ExtentSink::default();

    let err = BenchmarkRunner::new(config(4 * MIB, 64 * MIB, 1))
        .run(&mut sink, "/dev/test", None::<fn(&BenchmarkProgress)>)
        .expect_err("4 MB cannot hold a 64 MB block");

    let message = err.to_string();
    assert!(
        message.contains("4 MB") && message.contains("64 MB") && message.contains("smaller"),
        "unclear message: {message}"
    );
    assert_eq!(sink.extent, 0, "nothing may be written");
}

#[test]
fn a_size_of_exactly_one_block_is_accepted() {
    let mut sink = ExtentSink::default();

    let result = BenchmarkRunner::new(config(4 * MIB, 4 * MIB, 1))
        .run(&mut sink, "/dev/test", None::<fn(&BenchmarkProgress)>)
        .unwrap();

    assert_eq!(sink.extent, 4 * MIB);
    assert_eq!(result.test_size, 4 * MIB);
}

#[test]
fn several_block_sizes_each_write_the_given_size() {
    let mut sink = ExtentSink::default();
    let size = 320 * MIB;

    let results = BenchmarkRunner::run_multi_block_sizes(
        config(size, 4 * KIB, 1),
        &[4 * KIB, 32 * MIB],
        &mut sink,
        "/dev/test",
        None::<fn(&BenchmarkProgress, usize, usize)>,
        Arc::new(AtomicBool::new(false)),
    )
    .expect("every block size must run");

    assert_eq!(results.len(), 2);
    for row in &results {
        assert_eq!(row.bytes_written, size, "block size {}", row.block_size);
    }
    assert_eq!(sink.extent, size);
}
