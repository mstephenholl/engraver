# io_uring write benchmark

`IoUringDevice` in `engraver-platform` writes through io_uring instead of `write(2)`. It ships behind the `io_uring` feature, and nothing calls it yet. This benchmark measured whether it writes faster before anyone wired it into the Writer.

On NVMe and tmpfs it did not. On a USB SSD it did, because it keeps more requests queued at the drive. Larger synchronous writes queue just as many, and they came within 1% of io_uring. So the Writer integration is not planned. Two changes to the existing sync path get almost all of the gain.

## Result

### NVMe and tmpfs

On an NVMe SSD, the four engines had medians between 1036 and 1105 MiB/s. Runs of one engine varied more than the medians of different engines did. A deeper io_uring queue did not help. The kernel splits each 4 MiB write into 32 requests of 128 KiB (`/sys/block/nvme0n1/queue/max_sectors_kb`), and `iostat` showed an average of 30 requests queued at the drive during the runs. One synchronous write already keeps the drive busy.

On tmpfs, no device limits the write. There io_uring was no faster than a synchronous write from an aligned buffer, and at queue depth 1 it was slower.

### USB SSD

With engraver's default 4 MiB chunks, `uring-qd8` wrote 938 MiB/s and `sync-aligned` wrote 875 MiB/s. Each engine's runs stayed within 1% of its median, and every `uring-qd8` run beat every `sync-aligned` run.

The difference is queue depth. The kernel splits writes to this drive into 512 KiB requests, so one 4 MiB write is 8 requests. During the synchronous runs, `iostat` showed about 4 requests queued, because the queue empties before the next write starts. `uring-qd8` submits 64 requests at a time and kept 15 to 24 queued.

A larger synchronous write fills the queue as well. With 32 MiB chunks, `sync-aligned` kept about 31 requests queued and wrote 927 MiB/s. `uring-qd8` wrote 934 MiB/s, 0.8% more. Across all chunk sizes, the best median was 945 MiB/s, from `uring-qd8` with 8 MiB chunks. That is close to what a 10 Gbps USB link carries in practice, so the link is probably the limit. That is inferred.

### The bounce-buffer copy

`engraver write` passes a buffer that is not block aligned, so `LinuxDevice` copies every chunk into an aligned bounce buffer before it writes. On NVMe that copy takes 1.5 to 1.9 times the CPU per GiB, and throughput does not change. On tmpfs it makes writes 21% slower.

On the USB SSD it makes writes 17% slower with 4 MiB chunks. The copy took 0.22 s of CPU per GiB, and `sync` took 0.23 s longer per GiB than `sync-aligned`. With synchronous writes, nothing is queued at the drive while the CPU copies, so the copy time adds directly to the write time.

## What this means for the Writer

- Aligning the Writer's buffer removes the copy. On the USB SSD, that alone takes 4 MiB writes from 729 to 875 MiB/s.
- Aligned 32 MiB writes reached 927 MiB/s. That is 27% faster than `engraver write` today and within 2% of the best io_uring result.
- A USB 3 stick is still untested. Most sticks use the USB Mass Storage Bulk-Only protocol, which runs one command at a time, so a deeper queue is not expected to help there. That is inferred.

## Numbers

Each cell is the median MiB/s over 5 runs, then the median CPU time (user plus system) in ms per GiB. Each run wrote 1 GiB and then read it back to verify it. All 140 runs verified with 0 errors.

NVMe and tmpfs, with 4 MiB chunks:

| Target | `sync` (today) | `sync-aligned` | `uring-qd1` | `uring-qdN` |
|---|---|---|---|---|
| NVMe, N = 8 | 1036, 590 | 1084, 310 | 1099, 308 | 1068, 321 |
| NVMe, N = 32 | 1086, 504 | 1105, 330 | 1085, 335 | 1098, 354 |
| tmpfs, N = 8 | 2125, 481 | 2684, 379 | 2285, 426 | 2641, 390 |

USB SSD, with N = 8:

| Chunk | `sync` (today) | `sync-aligned` | `uring-qd1` | `uring-qd8` |
|---|---|---|---|---|
| 4 MiB | 729, 572 | 875, 342 | 850, 366 | 938, 254 |
| 8 MiB | 802, 470 | 911, 346 | 886, 362 | 945, 477 |
| 16 MiB | 809, 481 | 916, 351 | 911, 354 | 933, 322 |
| 32 MiB | 818, 403 | 927, 230 | 924, 227 | 934, 206 |

The engines are:

- `sync`: what `engraver write` does today. The device opens with the chunk size as its block size, and every chunk goes through the bounce-buffer copy and then `write(2)`.
- `sync-aligned`: `write(2)` from a 4096-byte aligned buffer, with no copy.
- `uring-qd1`: `IoUringDevice::write_batch` with one chunk per call.
- `uring-qdN`: `write_batch` with N chunks per call. Each batch finishes before the next one starts.

The machine was an Intel Core Ultra 7 155U with 14 cores, Linux 7.0.0, and a 512 GB PCIe NVMe SSD formatted ext4. The load average was about 1. The USB SSD was a 1 TB Kingston NV3 in an enclosure with a Realtek RTL9210 bridge, on a 10 Gbps USB-C port.

## Limits of this measurement

- The NVMe target was a file on ext4, not the raw device, because that drive holds the operating system. The USB SSD was measured as a raw device.
- In the first round on NVMe, the drive's write cache was still draining, so those runs were faster than the rest. The medians include them. On the USB SSD, dropping the first round moves no median by more than 0.7%.
- The USB SSD is one device, and it is much faster than a typical USB stick.
- `uring-qdN` waits for each batch to finish. A loop that refills the queue as writes complete was not tested. On NVMe, and on the USB SSD with 32 MiB chunks, a synchronous write already kept about 30 requests queued, so a refill loop is not expected to help. That is inferred.
- CPU time comes from `getrusage(RUSAGE_SELF)`, so kernel work on behalf of the device on other CPUs is not counted. On the USB SSD, CPU time also moved with chunk size in ways this benchmark does not explain. `uring-qd8` used 477 ms per GiB with 8 MiB chunks and 206 to 322 ms at the other sizes.

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

Add `--chunk 32M` to compare chunk sizes. On a slow device, use `--size` to write less than 1 GiB per run.

An NVMe drive in a USB enclosure can draw more power than a USB-A port supplies. The enclosure measured here failed writes on a 5 Gbps USB-A port and worked on USB-C. Too little power is the likely cause. That is inferred.
