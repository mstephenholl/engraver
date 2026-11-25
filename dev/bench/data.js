window.BENCHMARK_DATA = {
  "lastUpdate": 1795645642407,
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
          "id": "4db96e8085ba3bba6b8e05379c7c7fb3a95a4a4d",
          "message": "fix(deps): update url to 2.5.9 for RUSTSEC-2026-0592 (#50)\n\nurl 2.5.8 and earlier recurse without a limit in Url::origin() on nested\nblob: URLs, so a crafted URL can overflow the stack. 2.5.9 fixes it.\ncargo deny and cargo audit fail CI on main until the lockfile moves.",
          "timestamp": "2026-11-23T09:10:57-05:00",
          "tree_id": "650d4d9fc2428ad60f06447bb92d3e8f0206b469",
          "url": "https://github.com/mstephenholl/engraver/commit/4db96e8085ba3bba6b8e05379c7c7fb3a95a4a4d"
        },
        "date": 1795443405958,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 97.25,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 103.02,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 101.02,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 101.79,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 100.5,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 19992.81,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 84009.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1624.26,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 101.33,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 8359.11,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 73170.18,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 577.59,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 36.08,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8103.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 73885.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 555.29,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 34.6,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 72193.94,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 85048.56,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 19072.64,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 1422.7,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 21622.52,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 2419.45,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 142.67,
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
          "id": "02b894536365ef5891c68c1ad8cc21f907c65d11",
          "message": "perf(core): align the Writer buffer and raise the default block size to 32M (#49)\n\n* test(core): require Writer chunks to start on a 4096-byte boundary\n\nDirect-I/O devices skip their bounce-buffer copy only when the buffer\naddress and length are both aligned. The Writer's chunk buffer is a\nplain Vec, which the allocator places 16 bytes past a page boundary, so\nevery chunk takes the copy. Record the address of each write the Writer\nissues and require full chunks to start on a 4096-byte boundary.\n\nThe test is ignored so this commit stays green; the next commit fixes\nthe buffer and removes the attribute. Run it on this commit with\n`cargo test -p engraver-core --lib -- --ignored test_writer_full_chunks`\nto see it fail.\n\n* perf(core): align the Writer buffer so direct I/O skips the bounce copy\n\nLinuxDevice writes a chunk straight from the caller's buffer only when\nthe buffer address and length are both multiples of the device alignment.\nOtherwise it copies the chunk into its own aligned buffer first. Neither\ncondition held for `engraver write`:\n\n- The CLI passed the chunk size (4 MiB) as the device alignment, a value\n  no buffer address satisfies.\n- The Writer's chunk buffer was a plain `Vec`, which glibc places 16\n  bytes past a page boundary, so it is not even 4096-aligned.\n\nAllocate the Writer's chunk buffer on a 4096-byte boundary by\nover-allocating the `Vec` and slicing from the aligned offset, and use it\nin both `write_internal` variants and in the read-back of\n`write_and_verify`. Stop passing the chunk size as the device alignment\nin write, erase and verify so devices open at the 4096 default. Chunk\nsizing is unchanged. Windows and macOS only store the value in\n`DeviceInfo`, and verify opens without direct I/O.\n\nThe test added in the previous commit now passes, so drop its `ignore`.\n\n* perf(core): raise the default block size from 4M to 32M\n\nWith the Writer buffer and device alignment fixed, larger chunks no\nlonger pay for a bounce-buffer copy, so the chunk size sets how many\nsyscalls and USB transfers a write needs. On a 10 Gbps USB SSD a\n4096-aligned buffer measured 875 MiB/s at 4 MiB and 927 MiB/s at 32 MiB.\n\n`DEFAULT_BLOCK_SIZE` in the Writer and `DEFAULT_BLOCK_SIZE_STR` in the\nsettings change together. The settings constant also feeds the `erase`\nand `benchmark` defaults, so those move to 32M as well; the default\nbenchmark test grows from 256 MB to 320 MB to keep ten blocks per pass.\n\nLeft at 4M on purpose: the `verify` subcommand's own default,\n`read_buffer_size` and `cloud_chunk_size`. The read buffer no longer\nmatches the block size, so fix the comments that claimed it did and\nreplace the test that pinned the two together with one that keeps the\nread buffer large.\n\nUpdate the help texts, config example, READMEs and man pages that state\nthe default.",
          "timestamp": "2026-11-23T10:14:04-05:00",
          "tree_id": "427e72a3f390a0d0c2d92056eab013ab092f9226",
          "url": "https://github.com/mstephenholl/engraver/commit/02b894536365ef5891c68c1ad8cc21f907c65d11"
        },
        "date": 1795447164239,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/16MB",
            "value": 1185.38,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59212.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52929.73,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 17223.59,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 103.6,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 57494.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18871.01,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1641.27,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 36.08,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53577.45,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8129.93,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 576.54,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 42.51,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 54092.01,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9464.26,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 677.76,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 122.44,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14930.93,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1719.83,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 96.99,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 104.6,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 104.73,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 106.59,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 103.35,
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
          "id": "762a3955d071dd22ff76f5dcc112a961856c3fa9",
          "message": "deps: bump the rust-dependencies group with 3 updates (#51)\n\nBumps the rust-dependencies group with 3 updates: [clap](https://github.com/clap-rs/clap), [zstd](https://github.com/gyscos/zstd-rs) and [ctrlc](https://github.com/Detegr/rust-ctrlc).\n\n\nUpdates `clap` from 4.6.8 to 4.6.9\n- [Release notes](https://github.com/clap-rs/clap/releases)\n- [Changelog](https://github.com/clap-rs/clap/blob/main/CHANGELOG.md)\n- [Commits](https://github.com/clap-rs/clap/compare/clap_complete-v4.6.8...clap_complete-v4.6.9)\n\nUpdates `zstd` from 0.13.5 to 0.14.4\n- [Release notes](https://github.com/gyscos/zstd-rs/releases)\n- [Commits](https://github.com/gyscos/zstd-rs/compare/v0.13.5...v0.14.4)\n\nUpdates `ctrlc` from 3.5.2 to 3.5.3\n- [Release notes](https://github.com/Detegr/rust-ctrlc/releases)\n- [Commits](https://github.com/Detegr/rust-ctrlc/compare/3.5.2...3.5.3)\n\n---\nupdated-dependencies:\n- dependency-name: clap\n  dependency-version: 4.6.9\n  dependency-type: direct:production\n  update-type: version-update:semver-patch\n  dependency-group: rust-dependencies\n- dependency-name: zstd\n  dependency-version: 0.14.4\n  dependency-type: direct:production\n  update-type: version-update:semver-minor\n  dependency-group: rust-dependencies\n- dependency-name: ctrlc\n  dependency-version: 3.5.3\n  dependency-type: direct:production\n  update-type: version-update:semver-patch\n  dependency-group: rust-dependencies\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: mholl <m.stephen.holland@gmail.com>",
          "timestamp": "2026-11-23T11:07:51-05:00",
          "tree_id": "cbd5e08fd34c050dbd69775474bd22f97d99403c",
          "url": "https://github.com/mstephenholl/engraver/commit/762a3955d071dd22ff76f5dcc112a961856c3fa9"
        },
        "date": 1795450457964,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 88.2,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 86.33,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 87.28,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 88.11,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 87.86,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 16885.66,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 53734.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1390.28,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 87.44,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6789.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 55988.63,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 486.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.63,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6923.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 45737.79,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 481.37,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.28,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 51139.16,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 50627.17,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 10503.36,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 772.75,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14693.64,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1934.43,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 119.13,
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
          "id": "9422bbe5833929e100e6e6be80808ed04489536a",
          "message": "fix: verify compressed and remote sources by the decompressed length (#62)\n\n* fix(platform): keep a macOS file's read position at the start\n\nWhen the disk-size ioctl fails, as it does for a regular file, macOS\nmeasures the file by seeking a clone of the handle to its end. A clone\nshares the handle's offset, and the old code never moved it back, so\nevery sequential read after open_device started at the end and saw\nEOF. On macOS, `engraver verify` of a compressed image against a file\nhashed none of the file.\n\nSeek the clone back to where it was. Raw disks answer the ioctl, so\nthey never took this path. Linux already restores the position.\n\nThe new test reads a freshly opened file with read_to_end. Every\nexisting platform test reads with read_at, which seeks first, so none\nof them saw the bug.\n\n* test: show verification of compressed images hashing the whole drive\n\n`engraver verify img.img.gz <drive>` hashes the target to its end, so a\ndrive that holds the image followed by anything else never matches. The\nsame drive passes for a plain image, which is compared byte for byte over\nthe image length only.\n\nAdd end-to-end tests that run the binary against a regular file holding\nthe image followed by 1 MiB of other bytes. The compressed-image test\nfails here. The corrupted-image, exact-length and plain-image tests pass\nboth before and after the fix and guard its edges. The shorter-target\ntest fails only on its message.\n\n`verify` demanded root before it looked at the target, so these tests\ncould not run unprivileged. The check exists for raw device access, so\nskip it when the target is a regular file. Reading a file needs no\nprivileges, and `verify` never writes the target. Add flate2 as a dev\ndependency of the CLI to compress the test images.\n\n* fix: verify compressed and remote sources by the decompressed length\n\nVerifying a compressed or remote source hashed the target to its end, so\non a drive it hashed the whole drive and never matched the image. Both\nchecksum paths, `engraver verify` and the post-write check in `write`,\nnow hash the source first, take its decompressed length, and hash\nexactly that many bytes of the target.\n\nAdd Verifier::calculate_checksum_of_prefix for the second step. It\nhashes `reader.take(len)`, so it never reads past `len`. A target\nshorter than the image returns SizeMismatch, with the bytes the target\nheld. It honours the cancel flag like calculate_checksum: it only reads\nthe flag, and a flag that is already set cancels before any read. Both\npaths share one function in commands/mod.rs, and an image of a\ndifferent length than the one expected, such as the bytes just written,\nfails verification.\n\nRemove the `size.or(compressed_size)` that mixed two lengths. The image\nlength is `size` and the bytes on disk or on the wire are\n`compressed_size`, and only the first is a length of the image:\n\n- The capacity check uses the image length. A compressed source whose\n  compressed file alone is larger than the drive is refused. Otherwise\n  its capacity is Unknown and the check is skipped with a notice,\n  instead of passing on the compressed size.\n- Progress totals use the image length, or a spinner when it is unknown.\n- The source size shown is the image length, or the compressed size\n  labelled as compressed.\n- The checkpoint already stores the image length through\n  WriteCheckpoint::new, so it needed no change.\n\n`verify` skips its root check when the target is a regular file, because\nreading a file needs no privileges. This lets the CLI tests run against\na file that holds the image and then other data.\n\nThe write path's resume-verify progress bar loses its per-block text; it\nshares the verify bars now. The checksum command's progress bar and size\nline follow the same rule. What it hashes is unchanged.",
          "timestamp": "2026-11-23T20:15:28-05:00",
          "tree_id": "d9d80541a7770fd8c4689d9bb4ebeee3b3770b27",
          "url": "https://github.com/mstephenholl/engraver/commit/9422bbe5833929e100e6e6be80808ed04489536a"
        },
        "date": 1795483294750,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/16MB",
            "value": 1138.18,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 58990.67,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52882.83,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 16872.94,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 103.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 57273.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18854.69,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1637.16,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 36.14,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53556.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8076.12,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 577.98,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 42.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 53860.92,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9519,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 686.3,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 122.98,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14408.74,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1744.86,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 97.28,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 105.17,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 105.08,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 107.8,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 103.9,
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
          "id": "93ef3b2f0adafc1786b771ea73cce6073efa27b6",
          "message": "fix(platform): read through an aligned buffer under Windows direct I/O (#68)\n\n* test(platform): show direct I/O reads and writes losing their place\n\nUnder direct I/O, a device sends a buffer of unaligned address or\nlength through an aligned bounce buffer. On Linux it reads or writes\nthe padded length, which moves the file position past the bytes it\nreports. A read of 1000 bytes moves 4096 bytes on, so the next read\nskips 3096 bytes. A write of 1000 bytes leaves the next write 3096\nbytes further on, past a gap of zeros.\n\nOn Windows the device hands the caller's buffer straight to ReadFile,\nand FILE_FLAG_NO_BUFFERING refuses a buffer whose address or length is\nnot sector-aligned.\n\nThese tests read a file opened with direct I/O in 1000-byte chunks\nand into misaligned slices of unaligned lengths, and compare the\nbytes with the file. On Linux, a third test writes twice with\nunaligned lengths and checks that the second write either fails or\nlands right after the first.\n\n* fix(platform): keep the Linux direct I/O position after the bytes moved\n\nUnder O_DIRECT, a buffer of unaligned address or length goes through\nan aligned bounce buffer, and the kernel reads or writes the padded\nlength. The device reported the unpadded count but left the file\nposition after the padding. A read of 1000 bytes left the next read\n3096 bytes too far on, so it returned the wrong bytes. A write of 1000\nbytes left the next write 3096 bytes on, past a gap of zeros.\n\nA bounced read now reads the aligned blocks that cover the request at\nan explicit offset, copies out the bytes asked for, and sets the\nposition just past them. Because the position can now sit inside a\nblock, a read also checks it, and bounces when it is not aligned.\n\nA bounced write still pads to a whole block, then steps the position\nback over the padding. No caller writes an unaligned length except as\nits last write, and the Writer, erase and benchmark all end that way.\nA write that follows one now starts mid-block, where the kernel\nrefuses it with EINVAL instead of leaving a gap.\n\nread_at and write_at now go through Read and Write, so there is one\nbounce path for each.\n\n* fix(platform): read through an aligned buffer under Windows direct I/O\n\nWith direct I/O, a Windows device opens with FILE_FLAG_NO_BUFFERING.\nReadFile on such a handle needs a whole number of sectors at a\nsector-aligned file offset, into a sector-aligned buffer on disks that\nenforce it, and the handle's own file pointer only moves to\nsector-aligned positions. Read passed the caller's buffer straight to\nReadFile. The Verifier reads into a plain Vec, and asks for a short\nlast read when the image ends mid-block. Windows refused those reads,\nand any seek to a position inside a sector, with\nERROR_INVALID_PARAMETER.\n\nWindowsDevice now keeps its own position. Every ReadFile and WriteFile\nnames its offset through OVERLAPPED, and Seek only computes the new\nposition, from the device size for SeekFrom::End. A read that is not\naligned reads the sector-aligned blocks that cover it into an aligned\nbuffer and copies out the bytes asked for, as on Linux. AlignedBuffer\nmoves from linux.rs to a module both platforms share. Its read_through\ntest runs on Linux against a fake device that refuses unaligned\nrequests, so it also covers the arithmetic Windows relies on.\n\nWindows writes still pass the caller's buffer, so a direct I/O write\nstill needs whole sectors. The direct I/O test file no longer imports\nWrite outside the Linux-only test that uses it, which left an unused\nimport on Windows.",
          "timestamp": "2026-11-24T10:21:47-05:00",
          "tree_id": "ee30159f9e58e182018b9ca8ee20fd31ea32adf3",
          "url": "https://github.com/mstephenholl/engraver/commit/93ef3b2f0adafc1786b771ea73cce6073efa27b6"
        },
        "date": 1795534075629,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/16MB",
            "value": 1192.12,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59037.83,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52454.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 18397.78,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 104.42,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 57529.71,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18837.47,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1640.09,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 36.11,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53550.2,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8101.25,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 579.41,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 42.9,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 54184.62,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9477.69,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 679.72,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 124.04,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14777.73,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1752.47,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 97.6,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 104.64,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 105.4,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 107.03,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 104.13,
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
          "id": "b02b852ebb36965aa91e3d3124f8986227fcf42a",
          "message": "fix(cli): read Ctrl+C at a prompt as a key (#74)\n\n* refactor(cli): put the yes/no prompt behind prompt::ask\n\nThe prompt is the one place that reads a key at a terminal, and today it\nis inlined in `confirm` with a dialoguer error type leaking into\n`prompt_error` and the exit-code tests. Move it into a self-contained\n`prompt.rs` that returns `io::Result<Option<bool>>`, where `None` means\nCtrl+C, so an integration test can compile the file and drive it on a\npseudo-terminal.\n\n`confirm` keeps its signature and callers and is the one place that turns\n`None` into a cancelled run. `prompt_error` goes away. The behavior is\nunchanged except that a prompt with no terminal now reads `Error: not a\nterminal` instead of `Error: IO error: not a terminal`.\n\n* test(cli): show Ctrl+C at a prompt waking the signal handler\n\nNo command reaches a prompt without root and a drive, so run\n`prompt::ask` itself on a real pseudo-terminal: a pty on Unix and a\nConPTY on Windows. The test binary re-runs itself as the child with a\n`ctrlc` handler installed as in `engraver`, so the handler's marker and\nthe child's exit code show what Ctrl+C did.\n\nOn Unix a prompt that has read Ctrl+C raises SIGINT itself, so the\nhandler runs and prints its own line next to the caller's. On Windows\nconsole keeps Ctrl+C a console control event, so the handler runs and\nthe prompt keeps waiting. The Ctrl+C test fails on every OS at this\ncommit. The y, n, Enter and ignored-key tests pass.\n\n* test(cli): keep the ConPTY child on the pseudo console\n\nA child started without `STARTF_USESTDHANDLES` keeps the parent's\nstandard handles when those are redirected, as they are under a test\nrunner, so on Windows it wrote to the runner's pipe, found no terminal\nand answered \"not a terminal\" instead of drawing the prompt on the\npseudo console. Name the three standard handles invalid so the child\ntakes the pseudo console for its input and output.\n\nOn windows-latest the Ctrl+C test now fails the way the design says:\nthe handler runs and the prompt keeps waiting until the 10 s timeout.\nThe y, n, Enter and ignored-key tests pass.\n\n* fix(cli): read Ctrl+C at a prompt as a key\n\nCtrl+C at a yes/no prompt must print `Cancelled.` and exit 130 on every\nplatform. It did not. dialoguer reads keys with `Term::read_key`, which on\nUnix raises SIGINT on itself when it reads Ctrl+C. That woke the process's\nCtrl+C handler, which printed `Cancelling... Press Ctrl+C again to force\nexit` on its own thread and raced the `Cancelled.` from `main`; 11 of 20\nruns printed both. On Windows `read_key` keeps processed input on, so\nCtrl+C became a console control event for the handler and the prompt kept\nwaiting.\n\nDraw the prompt with `Term::stderr()` and read keys with `read_key_raw`,\nwhich returns `Key::CtrlC` on both platforms and raises nothing. `y`, `n`\nand Enter answer as before, other keys are ignored, and a stderr that is\nnot a terminal is still refused. The cursor is no longer hidden, so a\nfailed prompt has nothing to restore. That was the only use of dialoguer,\nso drop the dependency.\n\nThe prompt tests from the previous commits now pass on Linux, macOS and\nWindows. Windows has passed the Ctrl+C test, so the README no longer\nlimits the exit code to Linux and macOS.",
          "timestamp": "2026-11-24T15:35:04-05:00",
          "tree_id": "d2bb6c3f80d7c735a144c03e4be37867dd6feab1",
          "url": "https://github.com/mstephenholl/engraver/commit/b02b852ebb36965aa91e3d3124f8986227fcf42a"
        },
        "date": 1795552868250,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 79.49,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 82,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 79.35,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 79.46,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 79.61,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 15762.47,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 64698.03,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1263.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 79.67,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6519.9,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 57484.76,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 448.7,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 28.13,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6286.59,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 58113.95,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 431.01,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 26.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 51897.2,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 67197.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 11168.85,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 806.19,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 17252.34,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1956.51,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 117.74,
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
          "id": "3845529b4e5ee38eac1f900643098e265279a093",
          "message": "fix(core): skip the hole at the end of an xz file when measuring it (#73)\n\n* test(core): show a sparse terabyte of xz padding still being read\n\nThe size check reads a file's trailing zeros 4 KiB at a time, so an xz file ending in a sparse hole is read in full. The new test gives the check 10 s to measure an xz stream followed by a 1 TiB hole. It fails at this commit.\n\n* fix(core): skip the hole at the end of an xz file when measuring it\n\ntrailing_zeros reads backward 4 KiB at a time, so an xz file followed\nby a 16 GiB sparse hole took 26 s to measure. On Linux, Android, macOS\nand FreeBSD the filesystem knows where the last data region ends. The\nbytes after it are a hole, so they are zeros: count them without\nreading and scan back from the end of the data, as before. If the\nfilesystem can't say, the whole tail is scanned as before.\n\nThe length is unchanged for every file.\n\n* docs: say which compressed images the size check can measure\n\nThe size check uses the length a compressed file records: exact for .xz,\na lower bound for .gz and .zst, none for .bz2. An image it cannot measure\nis written until the drive fills, then fails with ENOSPC.\n\n* docs: say the size check measures local compressed files only\n\nA compressed image from a URL or cloud storage gets no recorded length,\nso the check can't refuse it before writing.",
          "timestamp": "2026-11-24T15:53:49-05:00",
          "tree_id": "7c9bdc65cc755158d3bdd207dbd007332fb279b2",
          "url": "https://github.com/mstephenholl/engraver/commit/3845529b4e5ee38eac1f900643098e265279a093"
        },
        "date": 1795553937373,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 71.22,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 71.14,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 69.4,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 71.75,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 66.88,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 14621.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 60765.02,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1116.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 68.23,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6515.37,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 52888.34,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 449.1,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 27.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 7268.33,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 54542.36,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 508.41,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 31.27,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 48063.85,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 60848.03,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 9781.36,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 668.97,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 9665.06,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1333.36,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 77.61,
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
          "id": "dc4dd345f83cd1809c8c1a3dc1e41b21f0b1b49a",
          "message": "fix(release): make the first release build, install and publish (#75)\n\n* ci(release): dry-run the release workflow on pull requests and dispatch\n\nThe workflow has never run. A manual dispatch ran every job, including the\nrelease job, and named the release after whatever ref was selected.\n\nOnly a tag push may release now. A pull request that touches the workflow\nor install.sh, and a manual dispatch, run the same build under the tag\nv0.0.0-dry-run and stop before the release job. Every archive name uses\nthat TAG instead of github.ref_name.\n\nThe release job's inputs are now built by two jobs of their own. check-crates\nruns cargo publish --workspace --dry-run, and package downloads the archives\ninto one folder, adds the SBOM and writes checksums.txt with bare file\nnames, so sha256sum -c works in a download folder. The release job signs\nevery file in that folder and uploads it.\n\nThe SBOM step asked cargo-sbom for the format cyclonedx-json. cargo-sbom\n0.10.0 rejects it and exits 2; it names the format cyclone_dx_json_1_6. Pin\nthat version and use that name, so the step runs.\n\nAt this commit the aarch64 build fails to link and check-crates fails on\npath dependencies without a version.\n\n* fix(release): build the aarch64 Linux archive on an arm64 runner\n\nThe aarch64-unknown-linux-gnu build ran on ubuntu-latest, an x86_64\nmachine, and had no linker configured for the target, so it could not\nlink. The step after it then ran the aarch64 binary on x86_64 to\ngenerate completions.\n\nBuild it on ubuntu-24.04-arm, where the target is the host. The\ncross-compilation step goes, and the binary can run to write its own\ncompletions and man pages.\n\n* test(release): install each archive through install.sh with sudo\n\nNothing has ever run install.sh against a built archive. The new\nscripts/test-install.sh serves the release files from a local mirror and\nruns install.sh against it, with an INSTALL_DIR and MAN_DIR that root\nowns, so install.sh has to copy through sudo. Case 1 expects a working\ninstall and a man page. Case 2 appends an empty gzip member to every\narchive, which still extracts the same files, and expects install.sh to\nrefuse it. Appending bytes to the archive would not do: GNU tar fails on\nany trailing garbage, so the old pipeline refused it by accident.\n\nThe test-install job runs the script on ubuntu-latest, ubuntu-24.04-arm\nand macos-latest against the files the package job built, and the release\njob waits for it.\n\ninstall.sh takes two new variables so the script can point it at the\nmirror: ENGRAVER_RELEASE_URL for where release files are downloaded from,\nand ENGRAVER_VERSION to skip the GitHub API lookup. Both are in --help.\n\nAt this commit case 1 fails on a runner where the copy goes through sudo\n(the chmod after it is not allowed on a root-owned file), and case 2\nfails because install.sh installs the corrupted archive. On a laptop\nwith a fake sudo only case 2 fails.\n\n* fix(install): leave the installed binary's mode to the archive\n\nWhen INSTALL_DIR is not writable, install.sh copies the binary with\nsudo cp, so the copy belongs to root. The chmod +x after it ran as the\nuser and failed with EPERM. With set -e that ended the script, and\ncompletions and man pages were never installed.\n\nThe chmod was never needed, because cp keeps the executable bit the\narchive gave the binary. Delete it.\n\npackaging/scripts/install.sh is a drifted copy of install.sh with the\nsame chmod. Nothing runs or links to it, so delete it too.\n\n* fix(install): verify the download against the release's checksums\n\ninstall.sh piped curl straight into tar and never looked at the\nchecksums.txt the release publishes, so a damaged or swapped archive was\nextracted and installed as it came. The test from the previous commit\nshows it: an archive with an empty gzip member appended extracts the same\nfiles, and install.sh installed it.\n\nDownload the archive and checksums.txt into the temp dir, take the hash\nfrom the line whose second field is the archive's name, and compare it\nwith the archive's SHA-256, from sha256sum or else shasum -a 256. A\nmissing entry, a missing hashing tool or a different hash ends the\nscript before anything is extracted. verify_checksum runs in the main\nshell, because error exits the shell it runs in and would only leave a\ncommand substitution.\n\n* refactor(cli): rename the package to engraver-cli\n\nThe CLI package was named engraver, and the engraver crate on crates.io\nbelongs to another project, so it could never be published and the\nREADME's cargo install engraver installed someone else's plotter. The\nengraver-cli, engraver-core, engraver-detect and engraver-platform names\nare free.\n\nRename the package to engraver-cli. The binary stays engraver, and so\ndoes the command's name in clap, so nothing a user types changes.\nCargo.lock follows, and every -p engraver in the workflows, install.sh,\nthe release script, the AUR and Homebrew source packages, CONTRIBUTING and\nthe device test's help becomes -p engraver-cli. The CLI README now says\ncargo install engraver-cli installs the engraver command.\n\n* fix(release): publish crates that package and stop hiding failures\n\nThe release job ran cargo publish --no-verify || true for each crate,\nwith sleeps between them, so every failure passed. Packaging would have\nfailed anyway: engraver-cli depends on the three library crates by path\nalone, and crates.io needs a version on every dependency. The release\nscript's sed lines look for engraver-x = { version = \"...\" and never\nmatched them. engraver-gui has no binary and is not for crates.io.\n\nGive the three path dependencies a version, written before the path so\nthe release script's sed lines match, and mark engraver-gui\npublish = false.\n\nThe publish step reads the publishable packages and their versions from\ncargo metadata and asks crates.io which of them it already has. It skips\nthose and publishes the rest with one cargo publish -p ... call, which\norders the crates and waits for the index between them. Rerunning the job\nafter a failure therefore finishes the work. A crates.io answer other\nthan 200 or 404 fails the job, and so does a failed publish.\n\nThe CARGO_REGISTRY_TOKEN secret still has to be added to the repository.\n\n* ci: check the README's checksum for install.sh\n\nREADME.md tells users to check install.sh against a SHA-256 it prints,\ncopied by hand, so it is wrong after any edit to the script. Today it\ngives 3d310d20... and the script hashes to something else.\n\nThe ShellCheck job now runs sha256sum on install.sh and fails with an\nerror on README.md unless the README contains that hash followed by two\nspaces and install.sh, the line sha256sum -c reads. The job runs when\nREADME.md changes as well as when the scripts do. It also lints\nscripts/*.sh now, which holds test-install.sh.\n\nThe README still has the old hash, so this check fails until the next\ncommit.\n\n* docs(readme): give install.sh's current checksum\n\nThe README gave 3d310d20... for install.sh, and the script no longer\nhashes to that. Anyone who followed \"Verify checksum\" got a failure from\nsha256sum -c on an unmodified download.\n\nGive the hash of the script as it is now, 36055664.... The ShellCheck job\nfails from now on when an edit to install.sh leaves this line behind.\n\n* test(release): indent install.sh's output without sed\n\nShellCheck on the CI runner flags echo \"$output\" | sed 's/^/    | /'\nas SC2001, and the ShellCheck job fails on it. Print each line with\nprintf in a read loop instead. The output is the same.\n\n* test(release): fail the install test unless install.sh must use sudo\n\nAt 6855728 the macOS leg passed \"install from a good archive\", although\ninstall.sh still ran chmod on the copy that sudo cp makes. A user can't\nchmod a file root owns, so install.sh must have found INSTALL_DIR\nwritable and copied without sudo: on that runner the directories that\nsudo mkdir made were still writable by the user.\n\nSet the owner, group and mode of both directories explicitly, and fail\nthe test, printing ls -ld, if the user can still write to either. The\ntest then covers the sudo path on every runner or says why it can't.",
          "timestamp": "2026-11-24T17:54:35-05:00",
          "tree_id": "7d2884d8c4560d090fbf966f53dfd0c5d5704e34",
          "url": "https://github.com/mstephenholl/engraver/commit/dc4dd345f83cd1809c8c1a3dc1e41b21f0b1b49a"
        },
        "date": 1795561203555,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 89.26,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 88.77,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 87.53,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 88.95,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 88.02,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 17077.73,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 50824.49,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1395.23,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 88.33,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 7009.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 47620.09,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 488.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6790.21,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 48336.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 481.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.16,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 43126.96,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 64898.34,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 10947.89,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 849.22,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 17101.29,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1961.45,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 121.43,
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
          "id": "2d728794af32e5d8aa458bc40a11b86908a00c76",
          "message": "fix(release): stop a crate crates.io refuses before releasing, and release 0.1.1 (#77)\n\n* ci(release): check the metadata crates.io enforces before releasing\n\nThe v0.1.0 publish uploaded engraver-detect, engraver-platform and\nengraver-core, then crates.io refused engraver-cli: \"expected at most 5\nkeywords per crate\". cargo publish --dry-run passed, because crates.io\napplies its own limits only when it receives an upload.\n\ncheck-crates now reads every publishable crate from cargo metadata and\nfails if one has more than 5 keywords, a keyword crates.io refuses, or\nno description or license. The release job needs check-crates, so a\ncrate like this one now stops the release before anything is published.\n\nAt this commit the step fails: engraver-cli has 6 keywords.\n\n* fix(cli): use the workspace's five keywords\n\nengraver-cli listed six keywords, and crates.io refuses a crate with\nmore than five, so engraver-cli 0.1.0 was never published. Its list\nwithout \"disk\" is the workspace's list, so inherit that.\n\n* chore: bump version to 0.1.1\n\nengraver-cli 0.1.0 never reached crates.io, and the v0.1.0 tag still\nhas the six keywords, so it can't be published from that tag. Release\n0.1.1 instead, with every crate moved together as release.sh does.\n\n* test(cli): check --version against the crate's own version\n\ntest_version_flag expected \"0.1.0\" in the output, so the bump to 0.1.1\nfailed it on every integration runner and in coverage. Compare against\nCARGO_PKG_VERSION instead, so the test checks that --version reports the\nversion Cargo builds and keeps passing across releases.",
          "timestamp": "2026-11-25T09:21:18-05:00",
          "tree_id": "b646605626eaa351c6711f407048a20f4bb0af42",
          "url": "https://github.com/mstephenholl/engraver/commit/2d728794af32e5d8aa458bc40a11b86908a00c76"
        },
        "date": 1795616864576,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 90.81,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 92.19,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.4,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 90.59,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 89.22,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 17129.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 50930.48,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1405,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 89.13,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 7022.55,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 55996.2,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 489.67,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.86,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6962.79,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 56979.46,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 483.11,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.42,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 51427.84,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 53637.96,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 11918.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 865.27,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 17041.95,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 2078.72,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 126.7,
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
          "id": "1accffdd667004fd023e70efa6173014770c54e1",
          "message": "fix: refuse a resume onto another drive, and count a Mac's external boot disk as a system disk (#79)\n\n* refactor(core): pass the target as a TargetIdentity to checkpoints\n\nA checkpoint recorded only the target's size, so a resume could not tell\nthe stick it was made for from another stick at the same path. The next\ncommits make it check the serial and model too. This one only changes the\nshape so the checks have something to compare.\n\nTargetIdentity holds size, serial and model. WriteCheckpoint::new and\nvalidate_checkpoint take it in place of the bare size, and the checkpoint\nstores target_serial and target_model with serde defaults, so a checkpoint\nwritten by 0.1.x still loads and CHECKPOINT_VERSION stays 1. write.rs builds\none TargetIdentity from the Drive and passes it to both calls.\n\nSerial and model are stored and not checked yet, so behavior is unchanged.\nThis breaks the public signatures of WriteCheckpoint::new and\nvalidate_checkpoint, which the next release (0.2.0) has to note.\n\n* test(core): a resume must be for the same drive, and a save must say it failed\n\nThese pin the behavior the next commit adds. They are written first so\nthey fail against the current code, which is the proof they test the bug.\n\nThe existing switched-stick test asserted that a size change is only a\nwarning. It now asserts the checkpoint is invalid, since the rest of the\nimage would land on a stick whose first part was never written. New tests\ncover a serial mismatch, a model mismatch, an unknown serial resuming with a\nwarning, a confirmed drive resuming with none, a 0.1.x checkpoint JSON with\nno serial or model still loading, save() returning Err when the temp file is\n/dev/full, and find_checkpoint leaving a checkpoint from a newer engraver in\nplace.\n\nFail at this commit (6 of 12 in resume_integration):\n  target_size_mismatch_invalidates_checkpoint\n  target_serial_mismatch_invalidates_checkpoint\n  target_model_mismatch_invalidates_checkpoint\n  unknown_serial_resumes_with_a_warning\n  save_fails_when_the_checkpoint_cannot_reach_disk\n  find_checkpoint_keeps_a_checkpoint_it_cannot_load\nThe other six pass, including checkpoint_json_without_identity_fields_still_loads\nand same_drive_resumes_without_a_warning, which guard against over-rejecting.\n\n* fix(core): refuse to resume onto another drive, and make save() report failure\n\nA resume checked only the target's size, and a size change was a warning.\nA different stick of the same size at the same path passed, got the tail of\nthe image, and the run ended \"Write complete!\" with the first part of the\nstick never written. Separately, save() dropped its BufWriter without\nflushing and never synced, so with the temp file on a full disk it returned\nOk(()) while no checkpoint reached disk, and find_checkpoint deleted any\ncheckpoint it could not load, including one from a newer engraver.\n\nvalidate_checkpoint now treats a different size as invalid, and a different\nserial or model as invalid when both sides know them. An unknown serial on\neither side stays valid with a warning, so checkpoints from 0.1.x and drives\nthat report no serial still resume. Each message names both values.\n\nsave() writes through the BufWriter, takes it back with into_inner so a\nfailed flush is an error, syncs the file, and only then renames.\nfind_checkpoint returns the load error and leaves the file in place, and\nsetup_checkpoint in write.rs prints it on stderr and starts a fresh write;\nthe next save replaces the file.\n\nLinux still reports no serial, so a serial check does nothing there until\nthe next commit. This makes the tests from the previous commit pass.\n\n* fix(detect): fill Drive.serial on Linux\n\nThe resume check compares serials, and Linux hard-coded serial to None, so\non Linux the check could only ever warn. macOS and Windows already fill it.\n\nread_serial reads {sys_path}/device/serial first, which is where NVMe and\nMMC keep it. A USB disk reaches the block layer through SCSI, which has no\nserial, so it takes the serial of the nearest ancestor that has an idVendor\nfile, the USB device itself. It does not keep walking when that device has no\nserial: the serial of a hub or root hub names the controller, and would make\nevery disk behind it look like the same drive. The value is trimmed, and an\nempty one is None.\n\nTests use the existing fake sysfs fixtures: NVMe with a padded serial, a USB\ndisk whose hub also has a serial, a USB device with none, an empty serial,\na SATA disk, and a missing device.\n\n* refactor(detect): look up the disk behind the macOS root volume\n\nmacOS can boot from an external APFS disk. Its volumes, mounted at /,\nsit in a container disk that get_disk_info skips, so list_drives has no\nstep that connects / to the physical disk holding it. The next commits\nadd that connection, and this one makes the place for it.\n\nlist_drives now runs diskutil info -plist / and passes the output to\nmark_apfs_boot_disks, which marks the disk behind each APFS physical\nstore as a system disk and leaves a reason that is already set alone.\nwhole_disk_of_store turns a store such as disk4s2 into its whole disk\nand rejects anything that is not disk<N> or disk<N>s<M>. Parsing and\nmarking take plist text, so tests can feed them fixtures.\n\nparse_apfs_physical_stores returns no stores yet, so list_drives\nbehaves as before and a root volume that is not APFS keeps being\ncovered by the mount point check.\n\n* test(detect): check that a macOS boot disk on APFS is a system disk\n\nAn external APFS disk that macOS booted from is listed as a safe target:\nits volumes sit in a container disk that get_disk_info skips, so no\nmount point points at the physical disk. These tests describe what\nlist_drives should do instead.\n\nThe fixture tests feed mark_apfs_boot_disks the diskutil info -plist /\noutput of an external boot disk (disk4s2) and of a Fusion drive (disk0s2\nand disk1s2). They also check that a disk that already has a system\nreason keeps it, that a root with no APFS stores marks nothing, and which\nidentifiers whole_disk_of_store accepts. The last test runs the real\ndiskutil and checks that the root volume's stores exist and that each\nstore's whole disk is in diskutil list's WholeDisks, which checks the key\nnames against what macOS prints. It assumes / is APFS, as it is on\nGitHub's macOS runners.\n\nAt this commit parse_apfs_physical_stores returns no stores, so three\ntests fail on macOS:\n- test_external_boot_disk_is_marked_as_system_disk\n- test_fusion_boot_disks_are_all_marked_as_system_disks\n- test_real_root_volume_stores_are_listed_disks\nThe pre-commit hook builds only the Linux code, so it does not run them.\n\n* fix(detect): mark the disk macOS booted from on APFS as a system disk\n\nOn a Mac booted from an external APFS disk, engraver list showed that\ndisk as a safe target, so write and erase accepted it without --force.\nIts volumes, mounted at /, sit in a container disk that get_disk_info\nskips, and the physical disk has only an unmounted Apple_APFS partition,\nso check_if_system_drive saw an external, ejectable disk with no system\nmount point.\n\nparse_apfs_physical_stores now reads the APFSPhysicalStores array of\ndiskutil info -plist / and returns every APFSPhysicalStore, so a Fusion\ndrive gives both of its stores. mark_apfs_boot_disks maps each store to\nits whole disk and marks that disk as a system disk.\n\nThe whole disk comes from the store's own identifier (disk4s2 gives\ndisk4) and not from a second diskutil info call for ParentWholeDisk. A\nphysical store is always a partition of its disk, the parse needs no\nprocess and no key name that only macOS could confirm, and it cannot\nfail halfway. An identifier that is not disk<N> or disk<N>s<M> marks\nnothing and is logged at debug.\n\nThe three tests that failed at the previous commit now pass.\n\n* refactor(core): give find_checkpoint a serial and validation a for_another_drive flag\n\nTwo follow-ups to refusing a resume onto another drive need new shape in\nthe core API: a checkpoint lookup that can be told which drive it is for,\nand a way for validate_checkpoint to say that a checkpoint is invalid\nbecause of the drive and not because of the source. The next commits fill\nboth in. This one only makes the places for them, so that the behavior\nchange comes separately from the signature change.\n\nfind_checkpoint takes target_serial: Option<&str>, and ignores it for now.\nCheckpointValidation gains for_another_drive, always false for now. The\ncallers are migrated: write.rs passes the drive's serial, and the unit and\nintegration tests pass None.\n\nNo behavior change.\n\n* test: a resume refused for another drive must keep that drive's checkpoint\n\nRefusing to resume onto a drive that differs removes the checkpoint, and\nthe fresh write that follows uses the same file name, because the name is\na hash of the source and the target path only. So the checkpoint of the\ndrive that was refused is lost, and plugging it back in cannot resume.\nA failed save is also logged at warn, which -q hides, and list --json\nleaves out the serial that tells two drives of one model apart. These\ntests describe what should happen instead.\n\nCore: a checkpoint saved for a drive with a serial is not found for\nanother drive at the same path, and survives that drive's save and\nremove. It is found again when its drive comes back at another path. A\ncheckpoint without a serial is still found for a drive that has one, and\nthe drive's own checkpoint is preferred over it. A size, serial or model\nmismatch sets CheckpointValidation::for_another_drive, and a source\nproblem does not.\n\nCLI: serde_json_drives writes the serial, and every drive in list --json\n--all has a serial key.\n\nAt this commit these tests fail:\n- checkpoint_of_another_drive_is_not_found_at_the_same_path\n- checkpoint_of_another_drive_survives_a_save_and_remove_at_the_same_path\n- checkpoint_is_found_when_its_drive_comes_back_at_another_path\n- a_drive_mismatch_is_marked_as_belonging_to_another_drive\n- test_serde_json_drives_includes_the_serial\n- test_serde_json_drives_unknown_serial_is_null\n- test_list_json_has_a_serial_key_for_every_drive\nThese pass already: the fallback to a checkpoint without a serial, the\npreference for the drive's own checkpoint, and the source problems not\nbeing marked. The hook runs only the lib tests, so it does not run these.\n\n* fix(core): name a checkpoint after its drive, and say when one is for another drive\n\nA checkpoint file was named by a hash of the source and the target path.\nA stick that came back at another path lost its checkpoint, and a\ndifferent stick at the same path used the same file, so a fresh write to\nit overwrote the checkpoint of the first one or removed it when it was\ndone. Not removing the file after a refused resume is therefore not\nenough, because the fresh write that follows reuses the name. The drive\nhas to be part of the name.\n\ncheckpoint_filename is now the one place that names a checkpoint file, and\nWriteCheckpoint::filename and find_checkpoint both use it. With a serial,\nthe name comes from the source and the serial, and the path is left out on\npurpose, so a stick plugged back in at /dev/sdc finds its checkpoint. An\nempty serial counts as none. Without a serial, the name is the same hash of\nsource and path as before, which a test pins, so checkpoints from 0.1.x and\nfor drives that report no serial are still found.\n\nfind_checkpoint takes the drive's serial. It looks for the drive's own file\nfirst and falls back to the path-named one, and without a serial it looks\nonly for the path-named one. A file that exists but cannot be loaded is\nstill an error.\n\nvalidate_checkpoint sets CheckpointValidation::for_another_drive for a size,\nserial or model mismatch. Source problems return their own result, so they\nnever set it.\n\nThe tests that failed at the previous commit in engraver-core pass. The CLI\ndoes not use for_another_drive yet.\n\n* fix(cli): keep another drive's checkpoint on a refused resume, and report a failed save on stderr\n\nA resume refused because the drive differs removed the checkpoint, so\nplugging the original drive back in could not resume. It now stays, with a\nline on stdout saying the checkpoint is for another drive and is kept, and\nthe write starts fresh. The fresh write is named after its own drive, so it\ndoes not touch the kept file (the previous commit). A checkpoint refused for\nany other reason, a changed source, is removed as before.\n\nThe file is kept only when the fresh write will not reuse its name. When\nneither the found checkpoint nor the target has a serial, both are named by\nthe path, so the fresh write would replace the file anyway. Keeping it\nthere would be a false promise, so it is removed as before.\n\nA failed checkpoint save was logged at warn, and a failed\nCheckpointManager::default_location at warn too. -q hides both, and the user\nlearns that --resume cannot continue only when they try it. Both now print on\nstderr whatever -q or --silent say, in the style of the warning about a\ncheckpoint that cannot be loaded, and the save warning says --resume cannot\ncontinue this write.\n\nsetup_checkpoint takes the checkpoint manager instead of opening it, and\nopen_checkpoint_manager opens it, so that the resume step can be tested with\na manager in a temporary directory. The new tests run it for the same drive,\nfor another drive that is kept, for the fresh write that follows it, for\ntwo drives without serials, and for a changed source. The pre-commit hook\ndoes not run engraver-cli's tests, so these run in cargo test --workspace.\n\n* fix(cli): put the drive's serial in list --json\n\nA script that has to tell two sticks of the same model apart, or that\nwants to match a drive to the checkpoint it left behind, had no serial to\ngo on: detection reads it on Linux, macOS and Windows, but list --json\nprinted only the vendor and model. serde_json_drives now writes \"serial\"\nafter \"model\", as null for a drive that reports none, so every object has\nthe key.\n\nThe tests added at the test commit for this now pass.",
          "timestamp": "2026-11-25T13:30:00-05:00",
          "tree_id": "1e1d24f7449875cd68bd47ffe06286fde6fa2836",
          "url": "https://github.com/mstephenholl/engraver/commit/1accffdd667004fd023e70efa6173014770c54e1"
        },
        "date": 1795631735834,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 87.66,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 88.38,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 86.15,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 87.72,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 87.1,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 16063.21,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 51870.54,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1387.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 86.92,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6787.29,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 45825.04,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 481.88,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.34,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6765.77,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 48994.75,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 477.37,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.08,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 44880.27,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 55830.6,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 10366.3,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 761.22,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 14724.4,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1923.12,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 111.99,
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
          "id": "a5bb66ce2c0e4abf85c3b9f3d8f7a7d4a3cbbd06",
          "message": "docs: make the docs and man pages match the binary, and run the CLI's unit tests everywhere (#80)\n\n* ci: run the CLI's unit tests on every runner and in the pre-commit hook\n\nengraver-cli is a binary crate, so `cargo test --lib` never builds its\nunit tests: the commands/* tests, including the setup_checkpoint tests in\nwrite.rs, did not run in the Unit Tests matrix (Linux x86_64, Linux arm64,\nmacOS, Windows) or in the pre-commit hook. Only Code Coverage ran them,\nand only on Linux. Locally that is 164 tests on top of the 564 that\n`--lib` runs.\n\nAdd `--bins` to every unit-test command that used `--lib`:\n- ci.yml: the Unit Tests matrix\n- _build.yml: the musl build's test step\n- nightly.yml: the nightly and beta unit-test runs\n- .cargo-husky/hooks/pre-commit, the versioned source of the hook\n\nThe tests have never run on macOS or Windows. `cargo clippy -p\nengraver-cli --all-targets --all-features` compiles them for\nx86_64-pc-windows-msvc and aarch64-apple-darwin with no warning in the\nCLI crate. Whether they pass there is for CI to say.\n\n* ci(release): ship the curated man pages, stamped with the release version\n\nThe release workflow ran `engraver mangen --out-dir man` before the\narchive steps, which overwrote the checked-in man/*.1 with clap's\ngenerated stubs. Every archive therefore carried the 256-line stubs, not\nthe 647 lines of curated pages, and install.sh put those on the user's\nsystem. mangen produces the same eight files the curated set has, so\nnothing is missing from man/.\n\nThe curated pages' .TH lines name the version they were last edited for\n(\"engraver 0.1.0\" today, while the release is 0.1.1), so replace that\nversion with the one the release is built for.\n\n- Remove the \"Generate man pages\" step.\n- Add a bash step before both archive steps that sets the version from\n  TAG (without the leading v) in each man/*.1 .TH line, and fails the job\n  when a page does not carry it afterwards. It uses `sed -i.bak`, which GNU\n  and BSD sed both take, and bash runs on the Windows runner too, so the\n  one step feeds the tar.gz and the zip.\n\nThe mangen subcommand stays in the CLI. scripts/test-install.sh still\nfinds man/engraver.1 in the archive.\n\n* fix(cli): tell the user when a finished write leaves its checkpoint behind\n\nAfter a successful write, execute() removes the write's checkpoint. When\nthat failed, it only logged `tracing::warn!`, and `-q` turns logging off,\nso the user never learned a stale checkpoint was still on disk. A later\n`--resume` of the same image and drive can find it and offer to continue a\nwrite that already finished.\n\nPrint the warning with `eprintln!`, as warn_checkpoint_not_saved does, so\nit shows with `-q` and `--silent` too. It names the checkpoint file\n(CheckpointManager::checkpoint_path), gives the error, and says to delete\nthe file because a later `--resume` may offer to continue a finished write.\n\nNo test covers this: reaching the line takes a completed write to a real\ndevice and a checkpoint file that then refuses removal, and the tests do\nnot write to a device. The message was rendered once from a throwaway test\n(since removed) and the hook passes.\n\n* docs(readme): say the pre-built binaries leave cloud storage out\n\nThe release builds `cargo build --release -p engraver-cli` with default\nfeatures, which are only `partition-info`. The install script downloads\nthat binary, and its source fallback builds the same way. A default build\ntreats `s3://bucket/key` as a local path:\n\n    $ engraver checksum s3://bucket/key\n    Source: s3://bucket/key\n    Error: Failed to validate source: s3://bucket/key\n\nPoint users at `cargo install engraver-cli --features cloud`, which\nbuilds 0.1.1 from crates.io.\n\n* docs(cloud): write Azure sources as azure://account/container/blob\n\nThe source parser accepts only `azure://account/container/blob`\n(crates/engraver-core/src/source.rs, `detect_source_type` and\n`parse_azure_uri`). An `az://` source is read as a local path, even in a\nbuild with the `cloud` feature:\n\n    $ engraver checksum az://my-container/images/debian.iso\n    Error: Failed to validate source: az://my-container/images/debian.iso\n      Caused by: Source not found: az://my-container/images/debian.iso\n\nFix the README example and both places in engraver-write(1).\n\n* docs(config): give the config file path per platform, and how to use it under sudo\n\n`Settings::config_path` is `dirs_next::config_dir()` (dirs-next 2.0.0)\njoined with `engraver/engraver_config.toml`. That is\n`$XDG_CONFIG_HOME` or `~/.config` on Linux,\n`~/Library/Application Support` on macOS, and `%APPDATA%` on Windows.\nThe docs gave `~/.config` for macOS.\n\nThe path follows `HOME`, so `sudo` reading root's file loses the user's\nsettings. `--config-file` is global and read before every command:\n\n    $ engraver config --path\n    /home/<user>/.config/engraver/engraver_config.toml\n    $ env -u XDG_CONFIG_HOME HOME=/root engraver config --path\n    /root/.config/engraver/engraver_config.toml\n\nWith a user config that sets `algorithm = \"md5\"`, `checksum --silent`\nprints the MD5 hash. With `HOME` reset it prints SHA-256. With\n`--config-file \"$(engraver config --path)\"` it prints MD5 again.\n\nFix README.md, the CLI README, docs/quickstart.md, engraver(1),\nengraver-config(1) and the settings.rs module doc.\n\n* docs(exit-codes): say a bad option value exits 1, and add EXIT STATUS to engraver(1)\n\nThe README tables already list 0, 1, 2 and 130, which is every code the\nCLI uses: `main` exits with `exit_for` (1, or 130 when the chain holds\n`Error::Cancelled`), the second-Ctrl+C handler exits 130, and clap exits\n2 on a parse error. They miss one boundary. Only clap's own parse errors\nexit 2. A value Engraver checks itself exits 1:\n\n    $ engraver --bogus            -> exit 2\n    $ engraver checksum           -> exit 2\n    $ engraver completions nope   -> exit 2\n    $ engraver checksum --algorithm nope x\n    Error: Invalid algorithm: nope\n                                  -> exit 1\n    $ engraver checksum /nonexistent/file\n    Error: Failed to validate source: /nonexistent/file\n                                  -> exit 1\n\nThe man pages had no exit status. Add an EXIT STATUS section to\nengraver(1) with the same four codes.\n\n* docs(readme): fix the run-CLI and fuzz commands in Development\n\n`.cargo/config.toml` defines `r = \"run -p engraver-cli --\"`, so\n`cargo r -- list` passes `-- list` to the binary:\n\n    $ cargo r -- list\n    error: unexpected argument 'list' found\n      tip: subcommand 'list' exists; to use it, remove the '--' before it\n    (exit 2)\n    $ cargo r list\n    Found 1 drive(s): ...  (exit 0)\n\nThere is no `fuzz_writer` target, and `cargo fuzz run` has no `-p` flag\n(`error: unexpected argument '-p' found`). The targets live in\ncrates/engraver-core/fuzz/fuzz_targets/. From the repo root,\n`cargo fuzz list --fuzz-dir crates/engraver-core/fuzz` lists them, and\n`cargo +nightly fuzz run --fuzz-dir crates/engraver-core/fuzz\nfuzz_format_functions -- -max_total_time=5` ran 315751 inputs and exited 0\n(cargo-fuzz 0.13.2).\n\n* docs(core): make the README examples compile, and say what WriteConfig::verify does\n\nFive of the six Rust examples in crates/engraver-core/README.md did not\ncompile against 0.1.1:\n\n- Progress, cancellation and checksum used undefined `source`,\n  `target`, `size`, `reader` and `expected`.\n- `auto_detect_checksum` returns `Option`, so `?` fails in a function\n  that returns `Result`.\n- `inspect_partitions` takes `&mut R: Read + Seek`, not a path.\n- Basic write imported `WriteConfig` and never used it.\n\nEach example now defines its inputs. The README says each one is the\nbody of a function that returns `Result<(), Box<dyn Error>>`. A\nthrowaway crate that depends on engraver-core by path builds each block\nas an example with `#![deny(warnings)]`, and all six compile. The five\nthat need no network run and exit 0.\n\n`WriteConfig::verify` is copied into a resume checkpoint, and nothing\nelse reads it. `Writer::write` never verifies. Read-back verification is\n`checksum_algorithm` plus `Writer::write_and_verify`. Fix the README\noptions table and the field doc in writer.rs, and drop `.verify(true)`\nfrom the lib.rs example, which calls `write`.\n\n* docs(man): compare a bare hash in the engraver-checksum(1) script example\n\n`engraver checksum FILE` prints the source, size, progress text, the\nhash and a checksum file line, so the example's\n`[ \"$expected\" = \"$actual\" ]` never matched. Only `--silent` prints the\nbare hash (commands/checksum.rs). On a 6-byte file, with `expected` set\nto its `sha256sum`:\n\n    actual=$(engraver checksum small.txt)          -> MISMATCH\n    actual=$(engraver checksum --silent small.txt) -> Match\n\nUse `--silent` in the example and list it under OPTIONS.\n\n* docs(man): say cloud sources need the cloud feature\n\nThe release archives now carry the curated man pages, and engraver(1)\nand engraver-write(1) list s3://, gs:// and azure:// sources with no\ncaveat. The pre-built binaries are built without the cloud feature, so\nthose sources fail there as local paths that don't exist.\n\nBoth pages, and the CLI README's feature list, now say cloud sources\nneed a build with the cloud feature and that the pre-built binaries\nleave it out.\n\n* docs(contributing): list the commands the pre-commit hook runs\n\nThe README and CONTRIBUTING said the hook runs `cargo test --lib`. This\nbranch's hook runs `cargo test --lib --bins`, and it also runs\n`cargo deny check` when cargo-deny is installed, which neither page\nlisted.",
          "timestamp": "2026-11-25T14:48:50-05:00",
          "tree_id": "d6139e33af8e4934ca09eacb34ddbe90d59d0835",
          "url": "https://github.com/mstephenholl/engraver/commit/a5bb66ce2c0e4abf85c3b9f3d8f7a7d4a3cbbd06"
        },
        "date": 1795636447223,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 90.13,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 90.87,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.17,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 90.13,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 89,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 17073.51,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 62618.37,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1399.15,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 88.47,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6958.9,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 55560.14,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 480.54,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 29.98,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6948.99,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 56769.29,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 483.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 50703.94,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 64320.72,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 11927.2,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 869.79,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 16926.35,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 2048.79,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 120.13,
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
          "id": "4651a876cf1c4c8ff428b2a393164211c4893358",
          "message": "fix: make -q quiet, keep man pages in source installs, run clippy everywhere, and clear cargo's warnings (#81)\n\n* test(cli): pin what -q and quiet = true print\n\n`-q` and `behavior.quiet` only turned logging off, so `engraver -q checksum\nFILE` printed the Source and Size lines, the progress and the result block\nlike a run without it, and `-q list`, `-q config` and `-q verify` printed their\nfull status output. The help and man page promise that -q suppresses\neverything except errors.\n\nAdd integration tests for the behaviour the help describes. `-q` and\n`quiet = true` in a --config-file each leave a checksum run printing only the\nhash, `-q list` and `-q config` print nothing (list --json still prints its\ndocument), and `-q verify` prints nothing on a match and only the error on a\nmismatch. A config with `quiet = false` keeps the normal output. These fail on\nthis commit and pass with the fix that follows.\n\n* fix(cli): make -q hide the command output its help says it hides\n\n`--help` and engraver(1) say `-q` suppresses all output except errors, and\n`behavior.quiet` turns on the same mode from the config file. In fact `main`\nused `-q` only to switch tracing off, so every command printed its Source and\nSize lines, progress and result block as usual. Only `--silent` reached the\ncommands.\n\nThree decisions shared the name `silent`: hide status output, skip\nconfirmation prompts, and hide the Ctrl+C messages. `OutputMode` (Normal,\nQuiet, Silent) now answers each with its own method. `-q` and `quiet = true`\nturn on only the first, so `-q write` and `-q erase` still ask before they\noverwrite a drive, and still print the Ctrl+C messages. `--silent` turns on\nall three, as before.\n\nThat makes a prompt reachable with status output hidden, which was impossible\nwhile `--silent` implied `--yes`. The unsafe-target and resume questions did\nnot name what they were about once the Source and Target lines were hidden. A\nrun that will ask now prints the lines that lead up to the confirmation (the\nimage, the drive, the mount points that will be unmounted, the checkpoint\nfound) and hides only what follows the answer. `hides_lead_up` is the one\nplace that decides this, for write and erase.\n\n`-q list` and `-q config` print nothing, as `--silent` does today, so a config\nwith `quiet = true` makes `engraver list` and `engraver config` print nothing\ntoo. `test_config_loads_custom_settings` set `quiet = true` only to exercise\nthe behavior section, and now sets `skip_confirmation` instead.\n\nDescribe both flags in `--help`, engraver(1) (which did not list `--silent`\nat all), engraver-checksum(1), engraver-config(1), the quickstart and both\nREADMEs.\n\n* docs(readme): give read_buffer_size's real default of 4M\n\nThe configuration example and the options table both said\n`read_buffer_size = \"64K\"`, but the code's default is \"4M\"\n(`DEFAULT_READ_BUFFER_SIZE_STR` in engraver-core's settings.rs). A user who\ncopied the example got a 64K read buffer, 64 times smaller than the default,\nand the table told them that was what they already had.\n\nCompare every other default in the table and example with\n`engraver config --json` on a missing config file. They all match; this was\nthe only one that differed. The `verify = true` in the example is an example\nvalue, not a default, and stays.\n\n* docs(quickstart): stop saying the pre-built binaries are not GA\n\nThe quickstart called the pre-built binaries \"not currently GA\". Release 0.1.1\nships them for Linux, macOS and Windows, along with install.sh, so a reader\nwas told to build from source for a download that exists.\n\nGive the section the heading and the one line the README's \"Pre-built\nBinaries\" section has, and nothing the README does not say.\n\n* fix(install): keep the curated man pages in a source install\n\nWhen `install.sh` cannot reach the release API it builds from source, and it\nran `engraver mangen --out-dir man` before installing the man pages. That\noverwrote the clone's hand-written `man/*.1` with the pages clap generates,\nwhich have no EXAMPLES, NOTES or FILES sections. #80 fixed the same bug in\nrelease.yml, and this path still had it.\n\nDelete the `mangen` call and stamp the built binary's version into the `.TH`\nlines of the checked-in pages, with the same `sed -i.bak -E` command the\nrelease workflow uses, so the pages say 0.1.1 and not the version they were\nlast edited for.\n\n`cargo build` also put its binary in `$CARGO_TARGET_DIR` when the user had set\nit, and the next line copied `target/release/engraver`, which was not there.\nPass `--target-dir target` so the binary is where the script looks for it.\n\nRun through the real source path with git's `insteadOf` pointing the clone\nat a bare copy of this branch, `--no-sudo`, and scratch directories. All 8\ninstalled pages match the branch's `man/` except the `.TH` line, which reads\n`\"engraver 0.1.1\"`, and a `CARGO_TARGET_DIR` set in the environment stayed\nunused. README.md holds install.sh's sha256, so update it in the same commit.\n\n* fix(cli): keep each command's result under -q\n\nThe -q fix hid list's drives, config's output and benchmark's results\ntable along with the status lines. With `quiet = true` in a config\nfile, `engraver config --path` then printed nothing, so the documented\n`sudo engraver --config-file \"$(engraver config --path)\"` passed an\nempty path. checksum already keeps its result, the hash, even under\n--silent.\n\nOutputMode gains hides_results(), which only --silent sets. list and\nconfig take it in place of the status flag, and benchmark uses it for\nits results tables while -q still hides its progress lines. The help\ntext and docs say what -q keeps.\n\nThe list and config tests now check that -q prints the same output as\na plain run, and a new test checks that `config --path` prints the path\nunder `quiet = true`. All three fail on the previous commit.\nNothing can run benchmark here without a device, so its results\nchange has no test.\n\n* refactor(platform): delete WindowsDevice::lock and unlock, which nothing calls\n\nClippy on x86_64-pc-windows-msvc refuses `lock` as dead code, so the\nWindows build could not pass `-D warnings`. Nothing in the workspace\ncalls `lock`, and `unlock` has no caller but `Drop`, which unlocks a lock\nthat is never taken. `lock` could not work anyway: it sends\nFSCTL_LOCK_VOLUME, which Microsoft documents as taking a volume handle,\nand `WindowsDevice::open` returns a `\\\\.\\PhysicalDriveN` handle.\nUnmounting happens in `unmount_windows_device`, through PowerShell.\n\nDelete both methods, the unlock in `Drop`, the two FSCTL imports only they\nused, and \"and volume locking\" from the module doc. DeviceIoControl stays\nfor `get_device_size`.\n\n* fix(detect): clear the clippy lints that only a Windows build compiles\n\nengraver-detect sets `clippy::pedantic`, but `windows.rs` is compiled only\nfor Windows, so Linux clippy never saw its 18 lints and `cargo clippy\n--target x86_64-pc-windows-msvc -- -D warnings` failed on them. Fix each\nin the code:\n\n- needless_raw_string_hashes (4): the PowerShell commands are plain `r\"...\"`.\n- useless_format (2): `\"PowerShell failed\".to_string()`.\n- unnecessary_wraps (1): `parse_powershell_disks` cannot fail, so it\n  returns the Vec and its caller and tests drop the `Result`.\n- redundant_closure_for_method_calls (3): `.map(ToString::to_string)`.\n- single_char_pattern (4): `starts_with('3')` and the like.\n- uninlined_format_args (1): `{mp}`.\n- missing_errors_doc (1): `list_drives` gains an `# Errors` section.\n- unreadable_literal (1) and ignore_without_reason (1), both in tests.\n\nNo behavior changes.\n\n* ci: run clippy on Linux, macOS and Windows\n\nCI ran clippy on ubuntu-latest only, so lints in `cfg(windows)` and\n`cfg(target_os = \"macos\")` code reached main unseen: a dead `lock` method\nand 18 pedantic lints in engraver-detect's windows.rs. The test matrix\nalready builds on all three, so this only costs the lint pass.\n\nMake `clippy` a matrix over ubuntu-latest, macos-latest and\nwindows-latest, with the same command and `fail-fast: false` as the test\nmatrix, so one platform's lints do not hide another's. `ci-success` reads\n`needs.clippy.result`, which is the matrix's aggregate, and `test-unit`\nand the others `need: clippy`, so neither changes. The job's name becomes\n\"Clippy (<os>)\"; nothing requires the old name, since main has no branch\nprotection. The `changes` filter lists `**/*.rs`, `**/Cargo.toml` and\nci.yml, so a PR touching any of them runs the job.\n\n* build: take every shared dependency's version from the workspace table\n\nCargo 1.100 warns `unused workspace dependency` for each\n`[workspace.dependencies]` entry that no crate inherits. All 26 entries\nname a dependency the workspace uses, but for 19 of them the crates gave\nthe version themselves, so the table did nothing and the versions could\ndrift apart. The warnings also buried the others in `cargo check`.\n\nSwitch each dependency that the table lists to `workspace = true` in\nengraver-cli, engraver-core, engraver-detect and engraver-platform. Each\ncrate keeps its own `features` (clap's `wrap_help`, reqwest's `blocking`),\nits `optional` and its target-specific placement. reqwest's\n`default-features = false` is now inherited from the table, which\nalready had it.\n\ntokio is the one mismatch. The table said `features = [\"full\"]`, but its\nonly user, engraver-core, wanted `rt-multi-thread`, and inheriting would\nhave turned `full` on for core: the `cargo tree -e features` output for\nengraver-core gained parking_lot, signal-hook-registry and more. Put\n`rt-multi-thread` on the table entry instead, so the features are the same\nas before.\n\nChecked with a script that records Cargo.lock, `cargo tree -e features\n--target all` for each crate (with all features, default features and no\ndefault features), the cargo warning counts, and the `Cargo.toml` that\n`cargo publish --dry-run` packages for the four published crates. Cargo.lock\nand all 15 trees are byte-identical, the packaged dependency tables are the\nsame, and `unused workspace dependency` goes from 19 warnings to 0.\n\n* build: inherit shared package fields, and drop dependencies nothing uses\n\nCargo warned about 8 unused [workspace.package] fields and 5 unused\ndependencies on every build.\n\n- version, authors, license and repository are the same in every\n  crate, so each crate now inherits them.\n- edition said \"2024\" in the workspace while every crate used \"2021\".\n  The workspace now says \"2021\" and the crates inherit it, so no crate\n  changes edition.\n- homepage, description and categories come out of the table. No crate\n  set homepage, and the crates' descriptions and categories differ.\n- humansize in engraver-cli, and anyhow, tracing, engraver-core and\n  engraver-detect in the engraver-gui stub, were never used.\n\nCargo.lock loses humansize and libm, which only humansize pulled in.\nThe four published crates package with identical [package] tables,\nand only engraver-cli's dependencies change, losing humansize.\nrelease.sh's version edit still bumps every crate, through the\nworkspace version line.\n\n* build(lints): declare the lint policy once, as the crates enforce it\n\nThe workspace lint table asked for clippy's pedantic, nursery and cargo\ngroups, but only the engraver-gui stub inherited it. The other crates\nset their own tables with clippy::all, and engraver-detect added\npedantic. Pointing every crate at the workspace table raised 1016\nwarnings on Linux, so the table described a policy nothing followed.\n\nThe workspace table now holds the policy the crates do follow:\nunsafe_code and missing_docs from rustc, and clippy::all. engraver-cli,\nengraver-core, engraver-platform and engraver-gui inherit it.\nengraver-detect keeps its own table, because cargo can't add pedantic\non top of an inherited one, and restates missing_docs to match.\n\nmissing_docs is new for four crates. It found three test files without\na crate doc, which now have one. engraver-gui's allow for\nclippy::multiple_crate_versions goes, since that lint belonged to the\ncargo group.\n\nclippy --workspace --all-targets --all-features -D warnings passes for\nx86_64-unknown-linux-gnu, x86_64-pc-windows-msvc and\naarch64-apple-darwin.\n\n* fix(cli): keep config's JSON and bare path under --silent\n\n`list --json` and `benchmark --json` print under --silent, and so does\nchecksum's bare hash, because a script asked for them. config hid its\nJSON, its `--path` and its `--init --json` result, so\n`$(engraver --silent config --path)` was empty. The old\ntest_config_path_silent_mode said the path \"should still output\", but\nasserted empty output.\n\nconfig now prints its JSON and the bare path under --silent, and hides\nonly its human-readable output. The --silent help text and engraver(1)\nsay what still prints.\n\ntest_config_path_silent_mode now expects the path, and a new test\nchecks the three JSON outputs. Both fail on the previous commit.",
          "timestamp": "2026-11-25T15:53:07-05:00",
          "tree_id": "a4dbbb8bda2df74b5f4f2b101338c25a8997527e",
          "url": "https://github.com/mstephenholl/engraver/commit/4651a876cf1c4c8ff428b2a393164211c4893358"
        },
        "date": 1795640389507,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "block_size/SHA-256/64KB",
            "value": 91.42,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 92.61,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 88.78,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 90.93,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 89.69,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 16554.56,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 52019.97,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1390.3,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 88.65,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 6904,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 49218.63,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 486.58,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 30.86,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 6842.73,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 50039.3,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 483.61,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 30.45,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 44684.06,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59752.39,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 10855,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/16MB",
            "value": 798.67,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 15370.05,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1969.28,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 124.77,
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
          "id": "d4ad4a1e96f22fddacfe86cc2a29632e3bd0c925",
          "message": "chore: release 0.2.0, and stop a release whose tag doesn't match the crates (#82)\n\n* ci(release): stop a tag that names a version the crates don't have\n\nA v0.2.0 tag pushed on a commit whose crates are still 0.1.1 would build\narchives whose binary reports 0.1.1, stamp 0.2.0 into their man pages,\nand create the v0.2.0 release. publish-crates would then find 0.1.1\nalready on crates.io and pass without publishing anything.\n\nOn tag pushes, check-crates now compares the tag with every publishable\ncrate's version. release needs check-crates, so a mismatch stops the run\nbefore a release is created.\n\n* chore: bump version to 0.2.0\n\n#79 changed engraver-core's public API. WriteCheckpoint::new and\nvalidate_checkpoint take a TargetIdentity, and two public structs gained\nfields. A 0.x crate marks a breaking change by raising its minor\nversion, so the next release is 0.2.0, not 0.1.2.\n\npackaging/scripts/release.sh 0.2.0 made this change. Its commit and tag\nprompts were answered no.",
          "timestamp": "2026-11-25T17:21:58-05:00",
          "tree_id": "94d29df41715f1d507adb6953d5714c49f922a0a",
          "url": "https://github.com/mstephenholl/engraver/commit/d4ad4a1e96f22fddacfe86cc2a29632e3bd0c925"
        },
        "date": 1795645641637,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "checksum/CRC32/16MB",
            "value": 1238.36,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1KB",
            "value": 59257.79,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/64KB",
            "value": 52786.86,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/CRC32/1MB",
            "value": 17734.32,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/16MB",
            "value": 104.25,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1KB",
            "value": 57047.68,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/64KB",
            "value": 18819.05,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-256/1MB",
            "value": 1642.07,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/16MB",
            "value": 36.17,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1KB",
            "value": 53592.09,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/64KB",
            "value": 8132.24,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/MD5/1MB",
            "value": 576.83,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/16MB",
            "value": 42.74,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1KB",
            "value": 54067.91,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/64KB",
            "value": 9483.46,
            "unit": "iter/sec"
          },
          {
            "name": "checksum/SHA-512/1MB",
            "value": 685.55,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/16MB",
            "value": 123.64,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/64KB",
            "value": 15474.61,
            "unit": "iter/sec"
          },
          {
            "name": "compare/identical/1MB",
            "value": 1740.35,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4MB",
            "value": 97.4,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/256KB",
            "value": 105.04,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/64KB",
            "value": 105.05,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/4KB",
            "value": 106.86,
            "unit": "iter/sec"
          },
          {
            "name": "block_size/SHA-256/1MB",
            "value": 103.93,
            "unit": "iter/sec"
          }
        ]
      }
    ]
  }
}