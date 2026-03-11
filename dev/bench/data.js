window.BENCHMARK_DATA = {
  "lastUpdate": 1773234394851,
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
      }
    ]
  }
}