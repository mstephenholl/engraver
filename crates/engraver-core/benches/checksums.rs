//! Benchmarks for checksum algorithms.
//!
//! Run with: cargo bench -p engraver-core

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engraver_core::verifier::{ChecksumAlgorithm, Verifier};
use std::hint::black_box;
use std::io::Cursor;

/// Generate test data of the specified size
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Benchmark checksum calculation for different algorithms and data sizes
fn bench_checksum_algorithms(c: &mut Criterion) {
    let mut group = c.benchmark_group("checksum");

    // Test different data sizes: 1KB, 64KB, 1MB, 16MB
    let sizes = [
        (1024, "1KB"),
        (64 * 1024, "64KB"),
        (1024 * 1024, "1MB"),
        (16 * 1024 * 1024, "16MB"),
    ];

    let algorithms = [
        (ChecksumAlgorithm::Sha256, "SHA-256"),
        (ChecksumAlgorithm::Sha512, "SHA-512"),
        (ChecksumAlgorithm::Md5, "MD5"),
        (ChecksumAlgorithm::Crc32, "CRC32"),
    ];

    for (size, size_name) in sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        for (algorithm, algo_name) in algorithms {
            group.bench_with_input(BenchmarkId::new(algo_name, size_name), &data, |b, data| {
                b.iter(|| {
                    let mut cursor = Cursor::new(data);
                    let mut verifier = Verifier::new();
                    verifier
                        .calculate_checksum(
                            black_box(&mut cursor),
                            black_box(algorithm),
                            Some(data.len() as u64),
                        )
                        .unwrap()
                });
            });
        }
    }

    group.finish();
}

/// Benchmark data comparison (verification)
fn bench_data_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare");

    // Test different data sizes
    let sizes = [
        (64 * 1024, "64KB"),
        (1024 * 1024, "1MB"),
        (16 * 1024 * 1024, "16MB"),
    ];

    for (size, size_name) in sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("identical", size_name),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut source = Cursor::new(data);
                    let mut target = Cursor::new(data.clone());
                    let mut verifier = Verifier::new();
                    verifier
                        .compare(
                            black_box(&mut source),
                            black_box(&mut target),
                            black_box(data.len() as u64),
                        )
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark with different block sizes
fn bench_block_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_size");

    let data_size = 16 * 1024 * 1024; // 16MB
    let data = generate_test_data(data_size);

    // Test different block sizes
    let block_sizes = [
        (4 * 1024, "4KB"),
        (64 * 1024, "64KB"),
        (256 * 1024, "256KB"),
        (1024 * 1024, "1MB"),
        (4 * 1024 * 1024, "4MB"),
    ];

    group.throughput(Throughput::Bytes(data_size as u64));

    for (block_size, block_name) in block_sizes {
        group.bench_with_input(
            BenchmarkId::new("SHA-256", block_name),
            &block_size,
            |b, &block_size| {
                b.iter(|| {
                    let mut cursor = Cursor::new(&data);
                    let config =
                        engraver_core::verifier::VerifyConfig::new().block_size(block_size);
                    let mut verifier = Verifier::with_config(config);
                    verifier
                        .calculate_checksum(
                            black_box(&mut cursor),
                            black_box(ChecksumAlgorithm::Sha256),
                            Some(data.len() as u64),
                        )
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50);
    targets = bench_checksum_algorithms, bench_data_comparison, bench_block_sizes
}
criterion_main!(benches);
