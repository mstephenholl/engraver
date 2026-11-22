window.BENCHMARK_DATA = {
  "lastUpdate": 1795360825086,
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
          "id": "34bf55d8e9daccc5aedcf55441cf1b7badebad82",
          "message": "deps: bump rust-dependencies group, raise MSRV to 1.89 (#36)\n\n* deps: bump rust-dependencies group, raise MSRV to 1.89\n\nThis is Dependabot's batch update from PR 36, rebuilt on current main.\nIt also fixes the Clippy, Cargo Deny and Security Audit failures that\nmain has today.\n\nBumped (15):\n- thiserror 2.0.18 -> 2.0.21\n- anyhow 1.0.102 -> 1.0.104\n- clap 4.6.1 -> 4.6.8\n- indicatif 0.18.4 -> 0.18.6\n- serde 1.0.228 -> 1.0.229\n- serde_json 1.0.150 -> 1.0.154\n- toml 1.1.2+spec-1.1.0 -> 1.2.1+spec-1.1.0\n- sha2 0.10.9 -> 0.11.1\n- md-5 0.10.6 -> 0.11.0\n- mockall 0.14.0 -> 0.15.0\n- object_store 0.13.1 -> 0.14.2\n- rand 0.9.4 -> 0.10.4\n- clap_complete 4.6.5 -> 4.6.12\n- clap_mangen 0.3.0 -> 0.3.4\n- libc 0.2.186 -> 0.2.191\n\nOther changes:\n- MSRV 1.87 -> 1.89. object_store 0.14 needs crc-fast 1.10, which\n  requires rustc 1.89. Staying on 1.87 would leave two quick-xml\n  advisories (RUSTSEC-2026-0194, -0195) that only object_store 0.14\n  fixes.\n- Cargo.lock regenerated with cargo +1.89 update and\n  CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback. This clears the\n  22 advisories cargo audit reports on main's lockfile.\n- rand 0.10 renamed the Rng extension trait to RngExt. Two integration\n  tests now import it.\n- Rust 1.100 added clippy::assert_is_empty. Two engraver-detect tests\n  now use assert_eq!.\n- deny.toml: allow BSD-2-Clause (zstd-sys 2.1), skip syn@2 and\n  base64@0.22, and drop the getrandom@0.3 and windows-sys@0.60 skips\n  that no longer match anything.\n\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\n\n* deps(deny): skip digest, crypto-common and reqwest duplicates\n\ncargo-deny-action runs with --all-features, so its graph includes the\noptional cloud feature. There, object_store 0.14 pulls in crc-fast 1.10\n(digest 0.10, crypto-common 0.1) and reqwest 0.13, next to sha2/md-5\n0.11 and engraver-core's own reqwest 0.12. No newer crc-fast exists.\nMoving engraver-core to reqwest 0.13 is a separate change.\n\n---------\n\nCo-authored-by: Michael Holland <m.stephen.holland@gmail.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2026-11-17T15:32:21-05:00",
          "tree_id": "2b54aa9d59c9d394ddaa9bb7ba54ee8597a17ac1",
          "url": "https://github.com/mstephenholl/engraver/commit/34bf55d8e9daccc5aedcf55441cf1b7badebad82"
        },
        "date": 1794947905552,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/256KB",
            "value": 104.98,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 103.82,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 97.52,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 108.27,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 105.28,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 36.28,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 578.57,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8111.63,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53497.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 1150.38,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 16198.06,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52733.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59051.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 104.14,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1642.35,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18838.1,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 56043.64,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 42.35,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 678,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9418.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 54086.16,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 124.36,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1745.92,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14873.39,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "m.stephen.holland@gmail.com",
            "name": "mholl",
            "username": "mstephenholl"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ac7660cfd9bb857e9fdef8010cca3814023e8608",
          "message": "chore(workspace): inherit rust-version and use resolver 3 (#38)",
          "timestamp": "2026-11-17T17:08:58-05:00",
          "tree_id": "ca1ba73a1f899d2dbe1b13a905bc96b378979c4f",
          "url": "https://github.com/mstephenholl/engraver/commit/ac7660cfd9bb857e9fdef8010cca3814023e8608"
        },
        "date": 1794953705469,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/256KB",
            "value": 96.18,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 96.27,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 93.31,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 101.07,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 97.1,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 35.25,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 578.13,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8162.51,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53106.56,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 1179.95,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 17350.34,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52931.57,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 58891.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 94.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1649.89,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18913.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 56239.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 41.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 682.06,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9438.97,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 53689.12,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 121.63,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1727.87,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14864.23,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "m.stephen.holland@gmail.com",
            "name": "mholl",
            "username": "mstephenholl"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ac2537063b117329a6b452d646b724f0d4cb046",
          "message": "deps(engraver-core): bump reqwest 0.12 -> 0.13 (#40)\n\nGives the workspace a single reqwest major. object_store 0.14 already\nuses 0.13, so the reqwest@0.12 and base64@0.22 skips in deny.toml go\naway.\n\nTLS behavior is unchanged. reqwest 0.13 drops its bundled webpki roots\nand verifies through the OS trust store. Its `rustls` feature also\nselects aws-lc-rs, which would add aws-lc-sys (a C/cmake build) to the\ndefault and static musl release builds. The crate instead enables\n`rustls-no-provider` and passes reqwest a rustls ClientConfig through\n`tls_backend_preconfigured`. The config uses the ring provider, the\nsafe default protocol versions (TLS 1.2 and 1.3), the Mozilla roots\nfrom webpki-roots and ALPN http/1.1, as reqwest 0.12 did. HTTPS keeps\nworking on hosts with no system CA certificates, such as minimal live\nor recovery images.\n\nNothing installs a process-wide default crypto provider, so object_store\nwith the cloud features keeps choosing its own.\n\nwebpki-roots is used rather than webpki-root-certs. The latter is not\ncompiled on Linux targets here and its README tells rustls users to\nprefer webpki-roots. CDLA-Permissive-2.0 stays in the license allowlist\nfor it.\n\nget_source_size now builds its client with a builder and maps failure to\nError::Network instead of panicking in Client::new().",
          "timestamp": "2026-11-17T17:09:35-05:00",
          "tree_id": "f9ba913b15cdc3c964c35f8bfe73482b4e12c2ea",
          "url": "https://github.com/mstephenholl/engraver/commit/2ac2537063b117329a6b452d646b724f0d4cb046"
        },
        "date": 1794953730745,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/256KB",
            "value": 104.38,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 103.48,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 97.2,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 106.62,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 104.42,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 35.57,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 573.95,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8130.21,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53627.82,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 1194.65,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 16863.34,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52614.37,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59108.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 103.25,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1639.64,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18848.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 57379.48,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 40.73,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 670.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9433.94,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 54029.04,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 124.46,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1736.42,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 15443.97,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "m.stephen.holland@gmail.com",
            "name": "mholl",
            "username": "mstephenholl"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6428a5a8d9d398d186d523d5807dded8a95ccc7d",
          "message": "feat(platform): add io_uring raw device writer for Linux (#41)\n\nPort d33a3f6 from feature/io-uring onto main. That branch shares no\nhistory with main, so the work is recreated here instead of merged.\n\nengraver-platform gains an `io_uring` feature that exports\nIoUringDevice, BatchWriteResult, is_io_uring_available and\nis_kernel_supported, plus a PlatformError::IoUringError variant. It\nuses io-uring 0.7.16, which declares rust-version 1.63.\n\nAdapted to main:\n- The BLKGETSIZE64 request has type libc::Ioctl, as in linux.rs. The\n  original c_ulong does not build on musl, where ioctl takes a c_int.\n- O_DIRECT comes from libc. The hard-coded 0o40000 is O_DIRECTORY on\n  aarch64.\n- write_at returns its count through bytes_consumed_from_aligned_write.\n  The original always returned data.len(), even after a short write.\n- sync calls File::sync_all, and the size fallback seeks with std. Two\n  unsafe blocks go away.\n\nDropped:\n- The engraver-core and engraver-cli `io_uring` features and the core\n  dependency on engraver-platform. Neither crate calls the io_uring code,\n  and CI already builds it with --all-features.\n- The \"Parallel verification\" TODO item. Main already lists it as done.\n\nSoundness findings. A submission entry holds a raw pointer into the\nbuffer it writes. The original could return while the kernel still had\nsuch an entry:\n- write_batch returned AlignmentError after it had pushed earlier\n  entries. They stayed in the submission queue, and the next submit sent\n  them to the kernel with pointers into buffers the caller had freed. A\n  new test fails on the original because the next write carries the\n  rejected entry to disk.\n- Each `?` after a push or submit_and_wait returned with entries queued\n  or in flight.\n- submit_and_wait(1) returns Ok with no completion when a signal\n  interrupts the wait. It also returns at once if an earlier call left a\n  completion behind. write_at could then take the wrong completion, or\n  none, and return while its own write was still in flight.\n- write_batch reaped by spinning on the completion queue without\n  entering the kernel. Once the 128-entry completion queue is full, the\n  kernel parks further completions in an overflow list that only a\n  kernel entry flushes, so the loop can spin forever. On Linux 7.0,\n  batches of 160 to 320 writes hung the original in every run. Batches\n  of 128 and 129 passed.\n\nThe fix routes every write through one function, submit_all. It keeps\nat most 64 entries pending, so the completion queue cannot overflow. It\nretries EINTR, EAGAIN and EBUSY, and it returns only after it reaps a\ncompletion for every entry it pushed. Any other io_uring_enter error\nmeans the ring is broken while the kernel may still read the buffers, so\nit aborts instead of returning. write_batch checks every offset before\nit queues anything.\nA guard aborts if a panic unwinds out of submit_all, so the no-unwind\nrequirement does not rest on review.\n\nAlignedBuffer was sound, because its slicing is bounds checked. The\nioctl was sound, because it writes one u64 into a u64.\n\nOther fixes:\n- Write::write never advanced the file position, so each write\n  overwrote the one before it.\n- write_at under direct I/O panicked on data longer than two blocks. The\n  aligned buffer now grows to fit.\n- write_batch counted short writes as complete. They now fail the batch.\n- open rejects a block size that is not a power of two under direct I/O.\n\nThe new tests run the real io_uring path against temp files. They skip\nwith a message when the host has no io_uring.",
          "timestamp": "2026-11-17T20:03:55-05:00",
          "tree_id": "813d29ce78bd81c8bb478303f4861dd1abc3334e",
          "url": "https://github.com/mstephenholl/engraver/commit/6428a5a8d9d398d186d523d5807dded8a95ccc7d"
        },
        "date": 1794964151705,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/SHA-256/16MB",
            "value": 83.8,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 45806.26,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1323.53,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 15065.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 1076.38,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 47150.69,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 14229.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 42094.59,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 29.02,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 42924.96,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 462.84,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6483.17,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.97,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 42817.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 494.71,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6903.07,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 106.77,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1524.2,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 12758.13,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 79.7,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 84.3,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 86.4,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 83.55,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 84.54,
            "unit": "iter/sec"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "m.stephen.holland@gmail.com",
            "name": "mholl",
            "username": "mstephenholl"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "593cda87ce54046bb17d297b2e5b999b4e160b20",
          "message": "fix(cli): reject block sizes that aren't a power of two (#47)\n\n* test(cli): cover non-power-of-two block sizes in write, erase and verify\n\nengraver benchmark rejects --block-size 3M with \"Size must be a power of\n2\". write, erase and verify accept it, because each has its own\nparse_block_size that checks only the range. These three integration\ntests pass --block-size 3M to each command and expect that same\nrejection.\n\nThey fail on this commit. Each command gets past argument parsing and\nstops at the root-privileges check instead. The next commit makes them\npass.\n\n* fix(cli): reject block sizes that aren't a power of two\n\nengraver write, erase and verify accepted any --block-size from 4K to\n64M, and verify did not check the range at all. Each command had its own\ncopy of parse_block_size. The block size is also the O_DIRECT alignment\nfor the device's bounce buffer, and align_up rounds with a mask, which\nonly works for a power of two. With --block-size 3M, align_up(3 MiB,\n3 MiB) returned 5 MiB, so the writer placed chunks at 0, 5, 10 and 15\nMiB. A 12 MiB image became a 20 MiB file with the data in the wrong\nplaces. Verification read the same wrong offsets and reported success\nwith no mismatches, so the corrupt image looked good.\n\nengraver benchmark already rejects such sizes through the shared parser\nin engraver-core. This adds parse_block_size there. It uses parse_size,\nwhich requires a power of two, and then checks the 4K to 64M range.\nparse_block_sizes now maps each part through it. write, erase and verify\ncall it, and the three private copies and their unit tests are removed.\nThe new core tests check which error variant each input produces.\n\nThe --block-size help text for the three commands now says the size must\nbe a power of 2 from 4K to 64M. Error messages for out-of-range and\nmalformed sizes change to the shared wording, for example \"Block size\n128M exceeds maximum of 64 MB\".",
          "timestamp": "2026-11-22T10:13:57-05:00",
          "tree_id": "e3e6e9e747c463c1b8bbacc5493dc59d75cf10a7",
          "url": "https://github.com/mstephenholl/engraver/commit/593cda87ce54046bb17d297b2e5b999b4e160b20"
        },
        "date": 1795360824405,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 90.02,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 89.07,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.02,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 89.73,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 88.67,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 17108.81,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 50976.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1402.92,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 88.43,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 7009.43,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 46822.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 486.8,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6954.5,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 47661.93,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 481.2,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.36,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 51220.83,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 57153.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 10670.4,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 789.69,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 15069.68,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1958.48,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 123.48,
            "unit": "iter/sec"
          }
        ]
      }
    ]
  }
}