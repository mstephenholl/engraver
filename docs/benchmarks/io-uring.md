# io_uring write benchmark

`IoUringDevice` in `engraver-platform` writes through io_uring instead of `write(2)`. It ships behind the `io_uring` feature, and nothing calls it yet. This benchmark measured whether it writes faster before anyone wired it into the Writer. It did not, so the Writer integration is not planned.

## Result

On an NVMe SSD, the four engines had medians between 1036 and 1105 MiB/s. Runs of one engine varied more than the medians of different engines did. A deeper io_uring queue did not help. The kernel splits each 4 MiB write into 32 requests of 128 KiB (`/sys/block/nvme0n1/queue/max_sectors_kb`), and `iostat` showed an average of 30 requests queued at the drive during the runs. One synchronous write already keeps the drive busy.

On tmpfs, no device limits the write. There io_uring was no faster than a synchronous write from an aligned buffer, and at queue depth 1 it was slower.

A USB 3 stick is slower than this NVMe drive, so io_uring is not expected to help there either. That is inferred. A run on a USB stick is still to do.

The cost that does show up is a copy. `engraver write` passes a buffer that is not block aligned, so `LinuxDevice` copies every chunk into an aligned bounce buffer before it writes. On NVMe that copy takes 1.5 to 1.9 times the CPU per GiB, and throughput does not change. On tmpfs it makes writes 21% slower. Aligning the Writer's buffer removes the copy without io_uring.

## Numbers

Each cell is the median MiB/s over 5 runs, then the median CPU time (user plus system) in ms per GiB. Each run wrote 1 GiB in 4 MiB chunks and then read it back to verify it. All 60 runs verified with 0 errors.

| Target | `sync` (today) | `sync-aligned` | `uring-qd1` | `uring-qdN` |
|---|---|---|---|---|
| NVMe, N = 8 | 1036, 590 | 1084, 310 | 1099, 308 | 1068, 321 |
| NVMe, N = 32 | 1086, 504 | 1105, 330 | 1085, 335 | 1098, 354 |
| tmpfs, N = 8 | 2125, 481 | 2684, 379 | 2285, 426 | 2641, 390 |

The engines are:

- `sync`: what `engraver write` does today. The device opens with the chunk size as its block size, and every chunk goes through the bounce-buffer copy and then `write(2)`.
- `sync-aligned`: `write(2)` from a 4096-byte aligned buffer, with no copy.
- `uring-qd1`: `IoUringDevice::write_batch` with one chunk per call.
- `uring-qdN`: `write_batch` with N chunks per call. Each batch finishes before the next one starts.

The machine was an Intel Core Ultra 7 155U with 14 cores, Linux 7.0.0, and a 512 GB PCIe NVMe SSD formatted ext4. The load average was about 1.

## Limits of this measurement

- The NVMe target was a file on ext4, not the raw device, because that drive holds the operating system.
- In the first round on NVMe, the drive's write cache was still draining, so those runs were faster than the rest. The medians include them.
- `uring-qdN` waits for each batch to finish. A loop that refills the queue as writes complete was not tested. It is not expected to help, because the drive queue is already full at queue depth 1. That is inferred.
- CPU time comes from `getrusage(RUSAGE_SELF)`, so kernel work on behalf of the device on other CPUs is not counted.

## Run it again

The benchmark is `crates/engraver-platform/examples/uring_bench.rs`. Build it with the `io_uring` feature:

```sh
cargo build --release -p engraver-platform --example uring_bench --features io_uring
```

To measure a filesystem, give the benchmark a file path on it:

```sh
target/release/examples/uring_bench /path/on/that/filesystem/uring_bench.img
```

The next command erases the whole device. To measure a raw device such as a USB stick, run the benchmark as root with `--destroy-data`:

```sh
sudo target/release/examples/uring_bench /dev/sdX --destroy-data
```

On a slow device, use `--size` to write less than 1 GiB per run.
