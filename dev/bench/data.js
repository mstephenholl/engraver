window.BENCHMARK_DATA = {
  "lastUpdate": 1779829461908,
  "repoUrl": "https://github.com/mstephenholl/engraver",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "name": "Michael Holland",
            "username": "mstephenholl",
            "email": "m.stephen.holland@gmail.com"
          },
          "committer": {
            "name": "Michael Holland",
            "username": "mstephenholl",
            "email": "m.stephen.holland@gmail.com"
          },
          "id": "9b27acb8124f27259e2f20512d41625c3a76b8be",
          "message": "test(cli): add end-to-end device workflow tests for write, verify, and erase\n\nAdd device_tests.rs with 10 tests validating full CLI workdlows against a real removvable drive, gated by ENGRAVER_TEST_DEVICE env var and #[ignore].  Document device test usage in CONTRIBUTING.md.",
          "timestamp": "2026-03-10T21:49:44Z",
          "url": "https://github.com/mstephenholl/engraver/commit/9b27acb8124f27259e2f20512d41625c3a76b8be"
        },
        "date": 1773234394372,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/1MB",
            "value": 89.1,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.31,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 91.21,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 92.23,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 90.76,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 481.98,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 47249.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.52,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6816.95,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1399.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 53105.44,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 89.1,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 16066.55,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 6761.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 52227.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 483.14,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 37511.57,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 489.4,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 46612.03,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.66,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6757.47,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 5607.45,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 408.27,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 13729,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "m.stephen.holland@gmail.com",
            "name": "Michael Holland",
            "username": "mstephenholl"
          },
          "committer": {
            "email": "m.stephen.holland@gmail.com",
            "name": "Michael Holland",
            "username": "mstephenholl"
          },
          "distinct": true,
          "id": "e952123e88505da453da130da2106a22815a9d66",
          "message": "chore: final clippy sweep + TODO audit\n\nVerified the standard feature configurations are gate-clean under\n'-D warnings':\n- cargo clippy --workspace --all-features --all-targets\n- cargo clippy --workspace --all-targets (default features)\n- cargo clippy --workspace --no-default-features --all-targets\n- cargo clippy --target x86_64-unknown-linux-gnu -p engraver-platform\n  --all-features --all-targets\n\nOne pre-existing dead-code warning remained under exotic feature\ncombinations (--features=checksum alone, or --features=remote alone):\nopen_file_buffered and open_file_buffered_with_size are only called\ninside #[cfg(feature = 'compression')] branches of Source::open. Gated\nthe helper definitions with the same attribute. Exotic combinations\nlike compression-only-without-checksum still fail because test files\nreference checksum-gated APIs (compute_local_header_hash,\nwrite_and_verify); supporting them would mean cfg-gating large parts\nof the test corpus, which is out of scope for cleanup.\n\nAudited the three remaining 'integration tests' TODO entries against\nthe actual test files:\n\n- tests/verify_integration.rs has 16 tests\n- tests/http_integration.rs has 15 tests (with mock server)\n- tests/compression_integration.rs has 19 tests (gzip, xz, zstd, bzip2)\n\nAll three items are factually done; TODO.md updated to match reality.\n\nNet workspace: 830 tests passing, 0 failing, 0 warnings under -D warnings\nacross the three primary feature configurations, cargo deny clean,\ncargo fmt clean.",
          "timestamp": "2026-05-26T14:44:29-04:00",
          "tree_id": "b1a1d7ed7da1caf506bde4c3357de3793a3afcc5",
          "url": "https://github.com/mstephenholl/engraver/commit/e952123e88505da453da130da2106a22815a9d66"
        },
        "date": 1779821639062,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/1MB",
            "value": 7057.52,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 68821.87,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 44053.53,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 463.11,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1263.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 64359.97,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 15742.76,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 79.45,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 441.17,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 57621.89,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6379.19,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 27.61,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 432.89,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 58929.75,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6308,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 27.1,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 81.53,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 79.65,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 79.56,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 79.89,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 79.93,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1547.23,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 15031.25,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 94.24,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "00e07bea84bd67560c1a65231df43a07b5c41623",
          "message": "deps: bump 10 rust-dependencies (4 deferred) (#27)\n\nThis is the rebased & trimmed form of dependabot's batch update on PR 27.\nFour bumps were dropped because they break the current MSRV / API or\nintroduce duplicate-version policy failures; deferred to dedicated PRs\nonce the upstream chains catch up.\n\nIncluded (10):\n- tracing-subscriber 0.3.22 -> 0.3.23\n- clap 4.5.60 -> 4.6.1\n- console 0.16.2 -> 0.16.3\n- serde_json 1.0.149 -> 1.0.150\n- toml 1.0.6+spec-1.1.0 -> 1.1.2+spec-1.1.0\n- tempfile 3.26.0 -> 3.27.0\n- assert_cmd 2.1.2 -> 2.2.2\n- clap_complete 4.5.66 -> 4.6.5\n- clap_mangen 0.2.31 -> 0.3.0\n- libc 0.2.183 -> 0.2.186\n\nDeferred (4):\n- sha2 0.10.9 -> 0.11.0 — pulled in dual majors of block-buffer /\n  crypto-common / digest via transitives; cargo-deny duplicate fail\n- md-5 0.10.6 -> 0.11.0 — same chain as sha2; defer until transitives\n  catch up\n- rand 0.9.4 -> 0.10.1 — same chain (getrandom dup) plus removed the\n  ThreadRng::random() API used by write_integration.rs:431\n- object_store 0.13.1 -> 0.13.2 — uses unstable if-let chains, breaks\n  MSRV 1.87 (would need bumping the project's rust-version to 1.88)\n\ndeny.toml adjustment: tempfile 3.27 brought in getrandom 0.4 while\nolder transitives (ring, etc.) still hold getrandom 0.2 / 0.3. Both\nolder majors are now in the [bans] skip list with explanatory notes;\nremove them as the upstream chain consolidates.\n\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2026-05-26T16:57:57-04:00",
          "tree_id": "b56f9fa8079b7a86347895578d077358cce44b89",
          "url": "https://github.com/mstephenholl/engraver/commit/00e07bea84bd67560c1a65231df43a07b5c41623"
        },
        "date": 1779829461546,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/1MB",
            "value": 7319.68,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 61302.44,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 42571.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 488.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1396.28,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 49176.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 16854.63,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 87.93,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 489.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 45801.75,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 7008.9,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.79,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 482.27,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 46677.5,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6925.74,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.32,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 88.9,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 88.58,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.1,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 88.43,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 89.01,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1573.43,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 10151.35,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 97.26,
            "unit": "iter/sec"
          }
        ]
      }
    ]
  }
}