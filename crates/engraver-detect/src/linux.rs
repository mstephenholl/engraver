//! Linux drive detection implementation
//!
//! Uses /sys/block for device enumeration and for the device-mapper and md devices stacked
//! on a disk (`holders/`), /proc/mounts for mount point detection and /proc/swaps for
//! swap partitions.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result, UsbSpeed};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tracing::{debug, trace, warn};

/// Mount information for a device
#[derive(Debug, Clone)]
pub(crate) struct MountInfo {
    pub mount_point: String,
    pub filesystem: Option<String>,
}

/// A device-mapper or md device stacked on a disk or one of its partitions
///
/// `LUKS`, `LVM` and software RAID mount their filesystems from `/dev/mapper/*`,
/// `/dev/dm-N` or `/dev/mdN`, so no mount entry names the underlying partition.
/// The kernel records the stacking in sysfs `holders/` directories instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Holder {
    /// The held block device: the disk itself or one of its partitions, such as `sda2`
    pub held: String,
    /// The device stacked on it, such as `dm-0` or `md0`
    pub by: String,
}

/// List all drives on Linux
///
/// # Errors
///
/// Returns an error if:
/// - `/sys/block` directory doesn't exist or can't be read
/// - `/proc/mounts` can't be read for mount point detection
/// - Individual device entries can't be parsed
pub fn list_drives() -> Result<Vec<Drive>> {
    let mut drives = Vec::new();
    let mount_map = get_mount_info()?;
    let swap_devices = get_swap_devices();
    let label_map = get_partition_labels();

    let block_dir = Path::new("/sys/block");
    if !block_dir.exists() {
        return Err(DetectError::EnumerationFailed {
            message: "/sys/block not found".to_string(),
            source: None,
        });
    }

    for entry in fs::read_dir(block_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip loop devices, ram disks, and device-mapper
        if should_skip_device(&name) {
            continue;
        }

        if let Some(drive) = parse_block_device(&name, &mount_map, &swap_devices, &label_map) {
            drives.push(drive);
        }
    }

    Ok(drives)
}

/// Check if a device should be skipped
pub(crate) fn should_skip_device(name: &str) -> bool {
    name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("dm-")
        || name.starts_with("zram")
        || name.starts_with("sr")  // CD/DVD drives
        || name.starts_with("fd") // Floppy drives
}

/// Parse a block device from /sys/block
fn parse_block_device(
    name: &str,
    mount_map: &HashMap<String, Vec<MountInfo>>,
    swap_devices: &[String],
    label_map: &HashMap<String, String>,
) -> Option<Drive> {
    let sys_path = format!("/sys/block/{name}");
    let dev_path = format!("/dev/{name}");

    if !Path::new(&sys_path).exists() {
        return None;
    }

    // Get size (in 512-byte sectors)
    let size_path = format!("{sys_path}/size");
    let size = read_sys_value(&size_path)
        .inspect_err(|e| warn!("Failed to read size from {size_path}: {e}"))
        .ok()
        .and_then(|s| {
            s.parse::<u64>()
                .inspect_err(|e| debug!("Failed to parse size for {name}: {e}"))
                .ok()
        })
        .map_or(0, |sectors| sectors * 512);

    if size == 0 {
        debug!("Skipping device {name}: size is 0");
        return None;
    }

    let vendor = read_sys_value(&format!("{sys_path}/device/vendor"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let model = read_sys_value(&format!("{sys_path}/device/model"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let drive_type = detect_drive_type(name, &sys_path);

    // Many USB bridges report `removable` as 0 for fixed media such as USB SSDs.
    // A USB-attached disk is external, so count it as removable. This matches
    // macOS (ejectable) and Windows (external media).
    let removable = drive_type == DriveType::Usb
        || read_sys_value(&format!("{sys_path}/removable")).is_ok_and(|s| s.trim() == "1");

    let partitions = get_partitions(name, mount_map, label_map);

    let mount_points = disk_mount_points(&dev_path, &partitions, mount_map);
    let swaps = swap_names(&dev_path, &partitions, swap_devices);

    let holders = read_holders(Path::new(&sys_path), name);

    let (is_system, system_reason) =
        check_if_system_drive(name, &mount_points, &holders, &swaps, removable);

    // Detect USB speed for USB drives
    let usb_speed = if drive_type == DriveType::Usb {
        detect_usb_speed(&sys_path)
    } else {
        None
    };

    let display_name = match (&vendor, &model) {
        (Some(v), Some(m)) => format!("{v} {m}"),
        (None, Some(m)) => m.clone(),
        (Some(v), None) => v.clone(),
        (None, None) => name.to_string(),
    };

    Some(Drive {
        path: dev_path.clone(),
        raw_path: dev_path,
        name: display_name,
        size,
        removable,
        is_system,
        drive_type,
        vendor,
        model,
        serial: None,
        mount_points,
        partitions,
        system_reason,
        usb_speed,
    })
}

/// Get every mount from /proc/mounts, keyed by device
///
/// A device can be mounted more than once, such as a btrfs partition at `/` and at
/// `/.snapshots`, so each device maps to all of its mounts in file order.
///
/// # Errors
///
/// Returns an error if `/proc/mounts` cannot be read.
pub(crate) fn get_mount_info() -> Result<HashMap<String, Vec<MountInfo>>> {
    let content =
        fs::read_to_string("/proc/mounts").map_err(|e| DetectError::EnumerationFailed {
            message: "Failed to read /proc/mounts".to_string(),
            source: Some(e),
        })?;

    Ok(parse_mounts(&content, Path::new("/sys")))
}

/// Group the lines of /proc/mounts by device
///
/// A `/dev/root` source is the kernel's name for a root filesystem mounted without an
/// initramfs, so it is replaced by the partition that `resolve_dev_root` finds under
/// `sysfs_root`. It stays `/dev/root` when that fails.
pub(crate) fn parse_mounts(content: &str, sysfs_root: &Path) -> HashMap<String, Vec<MountInfo>> {
    let mut mounts: HashMap<String, Vec<MountInfo>> = HashMap::new();

    for (device, info) in content.lines().filter_map(parse_mount_line) {
        let device = if device == "/dev/root" {
            resolve_dev_root(Path::new(&info.mount_point), sysfs_root)
                .map_or(device, |name| format!("/dev/{name}"))
        } else {
            device
        };
        mounts.entry(device).or_default().push(info);
    }

    mounts
}

/// Find the block device behind a `/dev/root` mount
///
/// Reads the device number of the filesystem at `mount_point`, then follows the
/// `dev/block/MAJOR:MINOR` symlink under `sysfs_root` (`/sys` on a real system) and
/// returns its last component, such as `mmcblk0p2`.
pub(crate) fn resolve_dev_root(mount_point: &Path, sysfs_root: &Path) -> Option<String> {
    let dev = fs::metadata(mount_point).ok()?.dev();
    let link =
        sysfs_root
            .join("dev/block")
            .join(format!("{}:{}", libc::major(dev), libc::minor(dev)));

    fs::read_link(link)
        .ok()?
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// Parse a single line from /proc/mounts
/// Format: device `mount_point` filesystem options dump pass
///
/// The kernel escapes a space, tab, newline or backslash in the device and the mount
/// point as an octal escape such as `\040`. Both come back decoded.
pub(crate) fn parse_mount_line(line: &str) -> Option<(String, MountInfo)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 3 {
        let device = unescape_mount_field(parts[0]);
        let mount_point = unescape_mount_field(parts[1]);
        let filesystem = parts[2].to_string();

        // Filter out pseudo filesystems
        let fs = if filesystem == "devtmpfs"
            || filesystem == "sysfs"
            || filesystem == "proc"
            || filesystem == "tmpfs"
            || filesystem == "securityfs"
            || filesystem == "cgroup2"
        {
            None
        } else {
            Some(filesystem)
        };

        Some((
            device,
            MountInfo {
                mount_point,
                filesystem: fs,
            },
        ))
    } else {
        None
    }
}

/// Decode the octal escapes in a `/proc/mounts` or `/proc/swaps` field
///
/// The kernel writes a space, tab, newline or backslash as `\040`, `\011`, `\012` or
/// `\134`. Anything other than a backslash and an octal byte from `\000` to `\377` is
/// kept as it is.
fn unescape_mount_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some([b'\\', d0 @ b'0'..=b'3', d1 @ b'0'..=b'7', d2 @ b'0'..=b'7']) =
            bytes.get(i..i + 4)
        {
            decoded.push(((d0 - b'0') << 6) | ((d1 - b'0') << 3) | (d2 - b'0'));
            i += 4;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Get the block devices in use as swap from /proc/swaps
///
/// An unreadable file means no swap is reported, which is logged because it leaves a
/// gap in the protection. The kernel's exclusive open still refuses a swap device.
pub(crate) fn get_swap_devices() -> Vec<String> {
    match fs::read_to_string("/proc/swaps") {
        Ok(content) => parse_swaps(&content),
        Err(e) => {
            warn!("Failed to read /proc/swaps, swap partitions will not be detected: {e}");
            Vec::new()
        }
    }
}

/// Parse /proc/swaps into the paths of the swap partitions
///
/// Each line is `Filename Type Size Used Priority`. Swap files are skipped. They live on
/// a mounted filesystem, which the mount check covers. The header line is skipped
/// because its type column is not `partition`.
pub(crate) fn parse_swaps(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let path = fields.next()?;
            (fields.next()? == "partition").then(|| unescape_mount_field(path))
        })
        .collect()
}

/// Every mount point of a disk and its partitions
///
/// A filesystem can sit on the whole disk with no partition table. A partition counts
/// as system when any of its mount points does, so the list keeps them all.
fn disk_mount_points(
    disk_path: &str,
    partitions: &[Partition],
    mount_map: &HashMap<String, Vec<MountInfo>>,
) -> Vec<String> {
    std::iter::once(disk_path)
        .chain(partitions.iter().map(|p| p.path.as_str()))
        .filter_map(|path| mount_map.get(path))
        .flatten()
        .map(|mount| mount.mount_point.clone())
        .collect()
}

/// Names of a disk and its partitions that are active swap, such as `sda3`
fn swap_names(disk_path: &str, partitions: &[Partition], swap_devices: &[String]) -> Vec<String> {
    std::iter::once(disk_path)
        .chain(partitions.iter().map(|p| p.path.as_str()))
        .filter(|path| swap_devices.iter().any(|swap| swap == path))
        .filter_map(|path| path.strip_prefix("/dev/"))
        .map(str::to_string)
        .collect()
}

/// Get partition labels from /dev/disk/by-label/
///
/// Returns a map of device path -> label
pub(crate) fn get_partition_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let label_dir = Path::new("/dev/disk/by-label");

    if let Ok(entries) = fs::read_dir(label_dir).inspect_err(|e| {
        debug!(
            "Failed to read label directory {}: {e}",
            label_dir.display()
        );
    }) {
        for entry in entries.flatten() {
            let label = entry.file_name().to_string_lossy().to_string();
            // Decode URL-encoded characters in label (e.g., \x20 for space)
            let label = decode_label(&label);

            if let Ok(target) = fs::read_link(entry.path()) {
                // Resolve the symlink to get the actual device path
                // Target is usually something like "../../sdb1"
                if let Some(device_name) = target.file_name() {
                    let device_path = format!("/dev/{}", device_name.to_string_lossy());
                    labels.insert(device_path, label);
                }
            }
        }
    }

    labels
}

/// Decode URL-encoded label (handles \x20 style escapes)
fn decode_label(label: &str) -> String {
    let mut result = String::new();
    let mut chars = label.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next(); // consume 'x'
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('\\');
                result.push('x');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Check whether a `/sys/block/<disk>` entry is one of the disk's partitions
///
/// Partitions are the entries named after the disk, such as `sda1` or `nvme0n1p2`.
fn is_partition_entry(device_name: &str, entry_name: &str) -> bool {
    entry_name.starts_with(device_name) && entry_name != device_name
}

/// List the entry names in a sysfs directory, sorted. A missing directory is empty.
fn sorted_entry_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Read the device-mapper and md devices stacked on a disk or any of its partitions
///
/// `sys_path` is the disk's sysfs node, such as `/sys/block/sda`. A device stacked
/// on the whole disk is listed under `<disk>/holders/`, and one stacked on a
/// partition under `<disk>/<partition>/holders/`. The disk comes first, then its
/// partitions by name. A node with no `holders/` directory holds nothing.
pub(crate) fn read_holders(sys_path: &Path, device_name: &str) -> Vec<Holder> {
    let mut nodes = vec![(device_name.to_string(), sys_path.to_path_buf())];
    for name in sorted_entry_names(sys_path) {
        if is_partition_entry(device_name, &name) {
            let dir = sys_path.join(&name);
            nodes.push((name, dir));
        }
    }

    let mut holders = Vec::new();
    for (held, dir) in nodes {
        for by in sorted_entry_names(&dir.join("holders")) {
            holders.push(Holder {
                held: held.clone(),
                by,
            });
        }
    }
    holders
}

/// Get partitions for a device
fn get_partitions(
    device_name: &str,
    mount_map: &HashMap<String, Vec<MountInfo>>,
    label_map: &HashMap<String, String>,
) -> Vec<Partition> {
    let mut partitions = Vec::new();
    let sys_path = format!("/sys/block/{device_name}");

    if let Ok(entries) = fs::read_dir(&sys_path)
        .inspect_err(|e| debug!("Failed to enumerate partitions for {device_name}: {e}"))
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if is_partition_entry(device_name, &name) {
                let part_path = format!("/dev/{name}");
                let part_sys_path = format!("{sys_path}/{name}");

                let size_path = format!("{part_sys_path}/size");
                let size = read_sys_value(&size_path)
                    .inspect_err(|e| debug!("Failed to read partition size from {size_path}: {e}"))
                    .ok()
                    .and_then(|s| {
                        s.parse::<u64>()
                            .inspect_err(|e| {
                                debug!("Failed to parse partition size for {name}: {e}");
                            })
                            .ok()
                    })
                    .map_or(0, |sectors| sectors * 512);

                // Get mount info (mount point and filesystem)
                let mount_info = mount_map.get(&part_path).and_then(|mounts| mounts.first());
                let mount_point = mount_info.map(|m| m.mount_point.clone());
                let filesystem = mount_info.and_then(|m| m.filesystem.clone());

                // Get label from /dev/disk/by-label/
                let label = label_map.get(&part_path).cloned();

                partitions.push(Partition {
                    path: part_path,
                    label,
                    filesystem,
                    size,
                    mount_point,
                });
            }
        }
    }

    partitions.sort_by(|a, b| a.path.cmp(&b.path));
    partitions
}

/// Detect drive type from name and sysfs info
pub(crate) fn detect_drive_type(name: &str, sys_path: &str) -> DriveType {
    // NVMe devices
    if name.starts_with("nvme") {
        return DriveType::Nvme;
    }

    // SD cards via MMC subsystem
    if name.starts_with("mmcblk") {
        return DriveType::SdCard;
    }

    // USB disks reach the block layer through SCSI (usb-storage and uas), so the
    // disk's own subsystem is `scsi`. The `usb` bus only shows up further up.
    if is_usb_attached(sys_path) {
        return DriveType::Usb;
    }

    // Check the device's own subsystem
    if let Ok(subsystem_link) = fs::read_link(format!("{sys_path}/device/subsystem")) {
        if let Some(subsystem_name) = subsystem_link.file_name() {
            let sub = subsystem_name.to_string_lossy();
            if sub == "ata" || sub == "scsi" {
                return DriveType::Sata;
            }
        }
    }

    // Check removable attribute as fallback for USB
    let removable = read_sys_value(&format!("{sys_path}/removable")).is_ok_and(|s| s.trim() == "1");

    if removable && name.starts_with("sd") {
        return DriveType::Usb;
    }

    DriveType::Other
}

/// Check if a drive is a system drive
///
/// The first rule that matches gives the reason. `swaps` names the disk or
/// partitions in use as swap.
pub(crate) fn check_if_system_drive(
    name: &str,
    mount_points: &[String],
    holders: &[Holder],
    swaps: &[String],
    removable: bool,
) -> (bool, Option<String>) {
    // Check mount points for system paths
    for mp in mount_points {
        if is_system_mount_point(mp) {
            return (true, Some(format!("Contains system mount point: {mp}")));
        }
    }

    // A filesystem on LUKS, LVM or md is mounted from the stacked device, not from
    // the partition, so the mount check above cannot see it. Writing under an open
    // device-mapper or md device is unsafe whether it holds `/` or data.
    if let Some(Holder { held, by }) = holders.first() {
        let kind = if held == name { "Disk" } else { "Partition" };
        return (
            true,
            Some(format!(
                "{kind} {held} is in use by {by} (device-mapper or md, such as LUKS or LVM)"
            )),
        );
    }

    // Swap holds memory contents, so writing under it corrupts a running system
    if let Some(held) = swaps.first() {
        let kind = if held == name { "Disk" } else { "Partition" };
        return (true, Some(format!("{kind} {held} is active swap space")));
    }

    // Non-removable drives are likely system drives
    if !removable {
        return (true, Some("Non-removable internal drive".to_string()));
    }

    (false, None)
}

/// Walk up the sysfs device tree from a block device, nearest ancestor first
///
/// Starts at the canonicalized `{sys_path}/device` node and stops at `/sys` or
/// `/`, or after 15 levels so a malformed tree cannot loop. Yields nothing if
/// the device link cannot be resolved.
fn device_ancestors(sys_path: &str) -> impl Iterator<Item = PathBuf> {
    let device = Path::new(sys_path).join("device").canonicalize().ok();
    std::iter::successors(device, |dir| {
        dir.parent()
            .filter(|parent| *parent != Path::new("/sys") && *parent != Path::new("/"))
            .map(Path::to_path_buf)
    })
    .take(15)
}

/// Check whether a block device hangs off the USB bus
///
/// USB disks reach the block layer through SCSI, so the `usb` subsystem is
/// only found on an ancestor of the device node.
fn is_usb_attached(sys_path: &str) -> bool {
    device_ancestors(sys_path).any(|dir| {
        fs::read_link(dir.join("subsystem"))
            .is_ok_and(|link| link.file_name().is_some_and(|subsystem| subsystem == "usb"))
    })
}

/// Detect USB connection speed for a block device
///
/// Traverses the sysfs device hierarchy upward from the block device
/// to find the USB device node, which contains the `speed` attribute.
///
/// Returns `None` for non-USB devices or if speed cannot be determined.
pub(crate) fn detect_usb_speed(sys_path: &str) -> Option<UsbSpeed> {
    // USB devices have a "speed" file containing the connection speed in Mbps
    device_ancestors(sys_path).find_map(|dir| {
        let speed_file = dir.join("speed");
        let speed_str = fs::read_to_string(&speed_file).ok()?;
        match speed_str.trim().parse::<u32>() {
            Ok(mbps) => Some(UsbSpeed::from_mbps(mbps)),
            Err(e) => {
                trace!(
                    "Failed to parse USB speed from {}: {e}",
                    speed_file.display()
                );
                None
            }
        }
    })
}

/// Read a value from sysfs
fn read_sys_value(path: &str) -> Result<String> {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(DetectError::Io)
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // Fake sysfs fixtures
    // -------------------------------------------------------------------------

    /// Create `dir` and point its `subsystem` link at `<root>/bus/<bus>`, as sysfs does.
    fn add_node(root: &Path, dir: &Path, bus: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::create_dir_all(root.join("bus").join(bus)).unwrap();
        symlink(root.join("bus").join(bus), dir.join("subsystem")).unwrap();
    }

    /// Expose a SCSI device's block node as `<root>/block/<name>` and return that path.
    ///
    /// Laid out like real sysfs: the block node lives under the SCSI device,
    /// its `device` link is relative, and `/sys/block/<name>` is a symlink to it.
    fn add_block_disk(root: &Path, name: &str, scsi_dev: &Path) -> String {
        let node = scsi_dev.join("block").join(name);
        fs::create_dir_all(&node).unwrap();
        let scsi_name = scsi_dev.file_name().unwrap().to_str().unwrap();
        symlink(format!("../../../{scsi_name}"), node.join("device")).unwrap();

        fs::create_dir_all(root.join("block")).unwrap();
        let link = root.join("block").join(name);
        symlink(&node, &link).unwrap();
        link.to_str().unwrap().to_string()
    }

    /// A disk behind a UAS bridge, with the ancestry of a real USB SSD enclosure.
    ///
    /// The disk's own subsystem is `scsi`. The `usb` bus only appears further up.
    fn usb_attached_disk(root: &Path) -> String {
        let pci = root.join("devices/pci0000:00/0000:00:0d.0");
        let usb_root_hub = pci.join("usb2");
        let usb_device = usb_root_hub.join("2-3");
        let usb_interface = usb_device.join("2-3:1.0");
        let host = usb_interface.join("host0");
        let target = host.join("target0:0:0");
        let scsi_dev = target.join("0:0:0:0");

        for (dir, bus) in [
            (&pci, "pci"),
            (&usb_root_hub, "usb"),
            (&usb_device, "usb"),
            (&usb_interface, "usb"),
            (&host, "scsi"),
            (&target, "scsi"),
            (&scsi_dev, "scsi"),
        ] {
            add_node(root, dir, bus);
        }
        fs::write(usb_root_hub.join("speed"), "20000\n").unwrap();
        fs::write(usb_device.join("speed"), "10000\n").unwrap();

        add_block_disk(root, "sda", &scsi_dev)
    }

    /// A disk on an internal SATA controller: `scsi` all the way up to the PCI device.
    fn sata_attached_disk(root: &Path) -> String {
        let pci = root.join("devices/pci0000:00/0000:00:17.0");
        let host = pci.join("ata1/host0");
        let target = host.join("target0:0:0");
        let scsi_dev = target.join("0:0:0:0");

        for (dir, bus) in [
            (&pci, "pci"),
            (&host, "scsi"),
            (&target, "scsi"),
            (&scsi_dev, "scsi"),
        ] {
            add_node(root, dir, bus);
        }

        add_block_disk(root, "sda", &scsi_dev)
    }

    // -------------------------------------------------------------------------
    // should_skip_device tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_should_skip_loop_devices() {
        assert!(should_skip_device("loop0"));
        assert!(should_skip_device("loop1"));
        assert!(should_skip_device("loop99"));
    }

    #[test]
    fn test_should_skip_ram_devices() {
        assert!(should_skip_device("ram0"));
        assert!(should_skip_device("ram15"));
    }

    #[test]
    fn test_should_skip_device_mapper() {
        assert!(should_skip_device("dm-0"));
        assert!(should_skip_device("dm-1"));
    }

    #[test]
    fn test_should_skip_zram() {
        assert!(should_skip_device("zram0"));
    }

    #[test]
    fn test_should_skip_optical() {
        assert!(should_skip_device("sr0"));
        assert!(should_skip_device("sr1"));
    }

    #[test]
    fn test_should_not_skip_real_devices() {
        assert!(!should_skip_device("sda"));
        assert!(!should_skip_device("sdb"));
        assert!(!should_skip_device("nvme0n1"));
        assert!(!should_skip_device("mmcblk0"));
        assert!(!should_skip_device("vda"));
    }

    // -------------------------------------------------------------------------
    // parse_mount_line tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_mount_line_basic() {
        let line = "/dev/sda1 / ext4 rw,relatime 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "/dev/sda1");
        assert_eq!(info.mount_point, "/");
        assert_eq!(info.filesystem, Some("ext4".to_string()));
    }

    #[test]
    fn test_parse_mount_line_with_spaces() {
        let line = "/dev/sdb1 /mnt/my\\040drive vfat rw 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "/dev/sdb1");
        assert_eq!(info.mount_point, "/mnt/my drive");
        assert_eq!(info.filesystem, Some("vfat".to_string()));
    }

    #[test]
    fn test_parse_mount_line_tmpfs() {
        let line = "tmpfs /tmp tmpfs rw,nosuid,nodev 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "tmpfs");
        assert_eq!(info.mount_point, "/tmp");
        // tmpfs is filtered out as pseudo filesystem
        assert_eq!(info.filesystem, None);
    }

    #[test]
    fn test_parse_mount_line_empty() {
        let result = parse_mount_line("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_mount_line_single_field() {
        let result = parse_mount_line("/dev/sda1");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_mount_line_two_fields() {
        // Two fields is not enough (need at least device, mount_point, filesystem)
        let result = parse_mount_line("/dev/sda1 /");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_mount_line_decodes_tab_newline_and_backslash() {
        let line = "/dev/sdb1 /mnt/a\\011b\\012c\\134d ext4 rw 0 0";
        let (_, info) = parse_mount_line(line).unwrap();
        assert_eq!(info.mount_point, "/mnt/a\tb\nc\\d");
    }

    #[test]
    fn test_parse_mount_line_decodes_each_escape_once() {
        // `\134040` is an escaped backslash followed by the text `040`, not a space
        let (_, info) = parse_mount_line("/dev/sdb1 /mnt/x\\134040y ext4 rw 0 0").unwrap();
        assert_eq!(info.mount_point, "/mnt/x\\040y");
    }

    #[test]
    fn test_parse_mount_line_keeps_text_that_is_not_an_octal_escape() {
        for path in [
            "/mnt/plain",
            "/mnt/a\\b",
            "/mnt/a\\04",
            "/mnt/a\\0x0",
            "/mnt/a\\400",
        ] {
            let line = format!("/dev/sdb1 {path} ext4 rw 0 0");
            assert_eq!(parse_mount_line(&line).unwrap().1.mount_point, path);
        }
    }

    #[test]
    fn test_parse_mount_line_decodes_device() {
        let (device, _) =
            parse_mount_line("/dev/disk/by-label/My\\040Disk /mnt/disk ext4 rw 0 0").unwrap();
        assert_eq!(device, "/dev/disk/by-label/My Disk");
    }

    // -------------------------------------------------------------------------
    // parse_mounts tests
    // -------------------------------------------------------------------------

    /// Mount points of `sda` and its partitions in `mounts`, as `parse_block_device` sees them
    fn sda_mount_points(mounts: &HashMap<String, Vec<MountInfo>>) -> Vec<String> {
        let partitions: Vec<Partition> = ["/dev/sda1", "/dev/sda2"]
            .map(|path| Partition {
                path: path.to_string(),
                ..Partition::default()
            })
            .into();
        disk_mount_points("/dev/sda", &partitions, mounts)
    }

    #[test]
    fn test_parse_mounts_keeps_every_mount_of_a_device() {
        let mounts = parse_mounts(
            "/dev/sda1 /boot/efi vfat rw 0 0\n\
             /dev/sda2 / btrfs rw,subvol=/@ 0 0\n\
             /dev/sda2 /.snapshots btrfs rw,subvol=/@/.snapshots 0 0\n",
            Path::new("/nonexistent"),
        );

        let root: Vec<&str> = mounts["/dev/sda2"]
            .iter()
            .map(|m| m.mount_point.as_str())
            .collect();
        assert_eq!(root, ["/", "/.snapshots"]);
    }

    #[test]
    fn test_disk_with_root_remounted_at_snapshots_is_system() {
        // The later mount of the same partition must not hide `/`
        let mounts = parse_mounts(
            "/dev/sda2 / btrfs rw 0 0\n/dev/sda2 /.snapshots btrfs rw 0 0\n",
            Path::new("/nonexistent"),
        );

        let (is_system, reason) =
            check_if_system_drive("sda", &sda_mount_points(&mounts), &[], &[], true);

        assert!(is_system);
        assert_eq!(reason.unwrap(), "Contains system mount point: /");
    }

    #[test]
    fn test_filesystem_on_the_whole_disk_is_system() {
        // A filesystem made on `/dev/sda` itself, with no partition table
        let mounts = parse_mounts("/dev/sda / ext4 rw 0 0\n", Path::new("/nonexistent"));

        let (is_system, reason) =
            check_if_system_drive("sda", &sda_mount_points(&mounts), &[], &[], true);

        assert!(is_system);
        assert_eq!(reason.unwrap(), "Contains system mount point: /");
    }

    #[test]
    fn test_disk_with_only_a_snapshots_mount_is_not_system() {
        let mounts = parse_mounts(
            "/dev/sda2 /.snapshots btrfs rw 0 0\n",
            Path::new("/nonexistent"),
        );

        let (is_system, _) =
            check_if_system_drive("sda", &sda_mount_points(&mounts), &[], &[], true);

        assert!(!is_system);
    }

    #[test]
    fn test_disk_with_efi_partition_mounted_at_efi_is_system() {
        let mounts = parse_mounts("/dev/sda1 /efi vfat rw 0 0\n", Path::new("/nonexistent"));

        let (is_system, reason) =
            check_if_system_drive("sda", &sda_mount_points(&mounts), &[], &[], true);

        assert!(is_system);
        assert_eq!(reason.unwrap(), "Contains system mount point: /efi");
    }

    #[test]
    fn test_disk_mounted_under_a_path_with_a_space_lists_the_decoded_path() {
        let mounts = parse_mounts(
            "/dev/sda1 /media/user/My\\040Backup ext4 rw 0 0\n",
            Path::new("/nonexistent"),
        );

        assert_eq!(sda_mount_points(&mounts), ["/media/user/My Backup"]);
    }

    // -------------------------------------------------------------------------
    // /dev/root resolution tests
    // -------------------------------------------------------------------------

    /// A fake sysfs whose `dev/block` entry for the filesystem holding `mount_point`
    /// links to a partition named `partition`.
    fn sysfs_with_root_on(root: &Path, mount_point: &Path, partition: &str) -> PathBuf {
        let dev = fs::metadata(mount_point).unwrap().dev();
        let sysfs = root.join("sys");
        let block = sysfs.join("dev/block");
        fs::create_dir_all(&block).unwrap();
        symlink(
            format!("../../devices/platform/soc/mmc0/block/mmcblk0/{partition}"),
            block.join(format!("{}:{}", libc::major(dev), libc::minor(dev))),
        )
        .unwrap();
        sysfs
    }

    #[test]
    fn test_resolve_dev_root_reads_the_partition_name_from_sysfs() {
        let root = TempDir::new().unwrap();
        let sysfs = sysfs_with_root_on(root.path(), root.path(), "mmcblk0p2");

        assert_eq!(
            resolve_dev_root(root.path(), &sysfs),
            Some("mmcblk0p2".to_string())
        );
    }

    #[test]
    fn test_resolve_dev_root_without_a_sysfs_entry_is_none() {
        let root = TempDir::new().unwrap();
        let empty_sysfs = root.path().join("sys");
        fs::create_dir_all(empty_sysfs.join("dev/block")).unwrap();

        assert_eq!(resolve_dev_root(root.path(), &empty_sysfs), None);
    }

    #[test]
    fn test_resolve_dev_root_for_a_missing_mount_point_is_none() {
        let root = TempDir::new().unwrap();

        assert_eq!(
            resolve_dev_root(&root.path().join("missing"), root.path()),
            None
        );
    }

    #[test]
    fn test_parse_mounts_attributes_dev_root_to_its_partition() {
        let root = TempDir::new().unwrap();
        let mount_point = root.path().join("rootfs");
        fs::create_dir_all(&mount_point).unwrap();
        let sysfs = sysfs_with_root_on(root.path(), &mount_point, "mmcblk0p2");

        let mounts = parse_mounts(
            &format!("/dev/root {} ext4 rw 0 0\n", mount_point.display()),
            &sysfs,
        );

        assert!(!mounts.contains_key("/dev/root"));
        assert_eq!(
            mounts["/dev/mmcblk0p2"][0].mount_point,
            mount_point.to_str().unwrap()
        );
    }

    #[test]
    fn test_parse_mounts_keeps_dev_root_when_it_cannot_be_resolved() {
        let root = TempDir::new().unwrap();
        let mounts = parse_mounts("/dev/root / ext4 rw 0 0\n", &root.path().join("sys"));

        assert_eq!(mounts["/dev/root"][0].mount_point, "/");
    }

    // -------------------------------------------------------------------------
    // parse_swaps tests
    // -------------------------------------------------------------------------

    const SWAPS: &str = "\
Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority
/dev/nvme0n1p3                          partition\t8388604\t\t0\t\t-2
/swapfile                               file\t\t2097148\t\t0\t\t-3
/dev/disk/by-label/swap\\040two          partition\t1048572\t\t0\t\t-4
";

    #[test]
    fn test_parse_swaps_lists_partitions_and_skips_swap_files() {
        assert_eq!(
            parse_swaps(SWAPS),
            ["/dev/nvme0n1p3", "/dev/disk/by-label/swap two"]
        );
    }

    #[test]
    fn test_parse_swaps_with_no_swap_is_empty() {
        assert_eq!(
            parse_swaps("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n"),
            Vec::<String>::new()
        );
        assert_eq!(parse_swaps(""), Vec::<String>::new());
    }

    #[test]
    fn test_swap_names_pick_the_disk_and_partitions_in_use() {
        let partitions: Vec<Partition> = ["/dev/sda1", "/dev/sda3"]
            .map(|path| Partition {
                path: path.to_string(),
                ..Partition::default()
            })
            .into();
        let swap_devices = parse_swaps(
            "Filename Type Size Used Priority\n\
             /dev/sda3 partition 1024 0 -2\n\
             /dev/sdab1 partition 1024 0 -3\n",
        );

        assert_eq!(swap_names("/dev/sda", &partitions, &swap_devices), ["sda3"]);
        assert_eq!(
            swap_names("/dev/sdb", &[], &swap_devices),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_check_if_system_drive_swap_partition_on_removable_disk() {
        let (is_system, reason) =
            check_if_system_drive("sdb", &[], &[], &["sdb2".to_string()], true);

        assert!(is_system);
        assert_eq!(reason.unwrap(), "Partition sdb2 is active swap space");
    }

    #[test]
    fn test_check_if_system_drive_whole_disk_swap() {
        let (is_system, reason) =
            check_if_system_drive("sdb", &[], &[], &["sdb".to_string()], true);

        assert!(is_system);
        assert_eq!(reason.unwrap(), "Disk sdb is active swap space");
    }

    // -------------------------------------------------------------------------
    // decode_label tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_label_simple() {
        assert_eq!(decode_label("UBUNTU"), "UBUNTU");
        assert_eq!(decode_label("my-drive"), "my-drive");
    }

    #[test]
    fn test_decode_label_with_space() {
        assert_eq!(decode_label("My\\x20Drive"), "My Drive");
    }

    #[test]
    fn test_decode_label_multiple_escapes() {
        assert_eq!(decode_label("A\\x20B\\x20C"), "A B C");
    }

    #[test]
    fn test_decode_label_invalid_escape() {
        // Invalid hex should be preserved
        assert_eq!(decode_label("Test\\xZZ"), "Test\\xZZ");
    }

    // -------------------------------------------------------------------------
    // detect_drive_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_drive_type_nvme() {
        assert_eq!(
            detect_drive_type("nvme0n1", "/sys/block/nvme0n1"),
            DriveType::Nvme
        );
        assert_eq!(
            detect_drive_type("nvme1n1", "/sys/block/nvme1n1"),
            DriveType::Nvme
        );
    }

    #[test]
    fn test_detect_drive_type_sd_card() {
        assert_eq!(
            detect_drive_type("mmcblk0", "/sys/block/mmcblk0"),
            DriveType::SdCard
        );
        assert_eq!(
            detect_drive_type("mmcblk1", "/sys/block/mmcblk1"),
            DriveType::SdCard
        );
    }

    #[test]
    fn test_detect_drive_type_unknown() {
        // Without sysfs, we get Other for regular devices
        assert_eq!(detect_drive_type("sda", "/nonexistent"), DriveType::Other);
    }

    #[test]
    fn test_detect_drive_type_usb_disk_behind_scsi() {
        let root = TempDir::new().unwrap();
        let sys_path = usb_attached_disk(root.path());

        assert_eq!(detect_drive_type("sda", &sys_path), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_sata_without_usb_ancestor() {
        let root = TempDir::new().unwrap();
        let sys_path = sata_attached_disk(root.path());

        assert_eq!(detect_drive_type("sda", &sys_path), DriveType::Sata);
    }

    // -------------------------------------------------------------------------
    // check_if_system_drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_if_system_drive_root() {
        let (is_system, reason) = check_if_system_drive("sda", &["/".to_string()], &[], &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("system mount point"));
    }

    #[test]
    fn test_check_if_system_drive_home() {
        let (is_system, reason) =
            check_if_system_drive("sda", &["/home".to_string()], &[], &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("/home"));
    }

    #[test]
    fn test_check_if_system_drive_boot() {
        let (is_system, _reason) =
            check_if_system_drive("sda", &["/boot".to_string()], &[], &[], false);
        assert!(is_system);
    }

    #[test]
    fn test_check_if_system_drive_non_removable() {
        let (is_system, reason) = check_if_system_drive("sda", &[], &[], &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("Non-removable"));
    }

    #[test]
    fn test_check_if_system_drive_removable_no_system_mounts() {
        let (is_system, reason) =
            check_if_system_drive("sdb", &["/mnt/usb".to_string()], &[], &[], true);
        assert!(!is_system);
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_if_system_drive_nvme_non_removable_is_system() {
        // An internal NVMe disk, the boot disk included, reports as non-removable
        let (is_system, reason) = check_if_system_drive("nvme0n1", &[], &[], &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("Non-removable"));
    }

    #[test]
    fn test_check_if_system_drive_removable_safe() {
        let mount_points = vec![
            "/media/user/USB".to_string(),
            "/run/media/user/disk".to_string(),
        ];
        let (is_system, _) = check_if_system_drive("sdc", &mount_points, &[], &[], true);
        assert!(!is_system);
    }

    #[test]
    fn test_check_if_system_drive_held_partition_on_removable_disk() {
        // A USB SSD whose root is on LUKS or LVM: no mount names the partition
        let holders = [Holder {
            held: "sda2".to_string(),
            by: "dm-0".to_string(),
        }];
        let (is_system, reason) = check_if_system_drive("sda", &[], &holders, &[], true);
        assert!(is_system);
        assert_eq!(
            reason.unwrap(),
            "Partition sda2 is in use by dm-0 (device-mapper or md, such as LUKS or LVM)"
        );
    }

    #[test]
    fn test_check_if_system_drive_held_whole_disk() {
        let holders = [Holder {
            held: "sdb".to_string(),
            by: "md0".to_string(),
        }];
        let (is_system, reason) = check_if_system_drive("sdb", &[], &holders, &[], true);
        assert!(is_system);
        let reason = reason.unwrap();
        assert!(reason.starts_with("Disk sdb "), "{reason}");
        assert!(reason.contains("md0"), "{reason}");
    }

    #[test]
    fn test_check_if_system_drive_held_nvme_without_mounts() {
        // A held NVMe partition is reported by the holders rule before the non-removable one
        let holders = [Holder {
            held: "nvme1n1p1".to_string(),
            by: "dm-0".to_string(),
        }];
        let (is_system, reason) = check_if_system_drive("nvme1n1", &[], &holders, &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("nvme1n1p1"));
    }

    #[test]
    fn test_check_if_system_drive_no_holders_unchanged() {
        let (is_system, reason) = check_if_system_drive("sdb", &[], &[], &[], true);
        assert!(!is_system);
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_if_system_drive_system_mount_wins_over_holders() {
        let holders = [Holder {
            held: "sda2".to_string(),
            by: "dm-0".to_string(),
        }];
        let (is_system, reason) =
            check_if_system_drive("sda", &["/boot".to_string()], &holders, &[], true);
        assert!(is_system);
        let reason = reason.unwrap();
        assert!(reason.contains("system mount point"), "{reason}");
        assert!(!reason.contains("dm-0"), "{reason}");
    }

    #[test]
    fn test_check_if_system_drive_holders_win_over_non_removable() {
        let holders = [Holder {
            held: "sda2".to_string(),
            by: "dm-0".to_string(),
        }];
        let (_, reason) = check_if_system_drive("sda", &[], &holders, &[], false);
        assert!(reason.unwrap().contains("dm-0"));
    }

    // -------------------------------------------------------------------------
    // read_holders tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_read_holders_finds_device_stacked_on_partition() {
        let root = TempDir::new().unwrap();
        let disk = root.path().join("block/sda");
        fs::create_dir_all(disk.join("sda1/holders")).unwrap();
        fs::create_dir_all(disk.join("sda2/holders/dm-0")).unwrap();
        fs::create_dir_all(disk.join("holders")).unwrap();

        assert_eq!(
            read_holders(&disk, "sda"),
            vec![Holder {
                held: "sda2".to_string(),
                by: "dm-0".to_string(),
            }]
        );
    }

    #[test]
    fn test_read_holders_finds_device_stacked_on_whole_disk() {
        let root = TempDir::new().unwrap();
        let disk = root.path().join("block/sdb");
        fs::create_dir_all(disk.join("holders/md0")).unwrap();

        assert_eq!(
            read_holders(&disk, "sdb"),
            vec![Holder {
                held: "sdb".to_string(),
                by: "md0".to_string(),
            }]
        );
    }

    #[test]
    fn test_read_holders_reads_nvme_partitions() {
        let root = TempDir::new().unwrap();
        let disk = root.path().join("block/nvme0n1");
        fs::create_dir_all(disk.join("nvme0n1p3/holders/dm-1")).unwrap();

        assert_eq!(
            read_holders(&disk, "nvme0n1"),
            vec![Holder {
                held: "nvme0n1p3".to_string(),
                by: "dm-1".to_string(),
            }]
        );
    }

    #[test]
    fn test_read_holders_empty_when_nothing_is_stacked() {
        let root = TempDir::new().unwrap();
        let disk = root.path().join("block/sda");
        fs::create_dir_all(disk.join("sda1/holders")).unwrap();
        fs::create_dir_all(disk.join("sda2")).unwrap();

        assert_eq!(read_holders(&disk, "sda"), Vec::<Holder>::new());
    }

    #[test]
    fn test_read_holders_missing_disk_is_empty() {
        let root = TempDir::new().unwrap();

        assert_eq!(
            read_holders(&root.path().join("block/sdz"), "sdz"),
            Vec::<Holder>::new()
        );
    }

    #[test]
    fn test_read_holders_orders_disk_before_partitions() {
        let root = TempDir::new().unwrap();
        let disk = root.path().join("block/sda");
        fs::create_dir_all(disk.join("sda2/holders/dm-1")).unwrap();
        fs::create_dir_all(disk.join("sda1/holders/dm-0")).unwrap();
        fs::create_dir_all(disk.join("holders/md0")).unwrap();

        let held: Vec<(String, String)> = read_holders(&disk, "sda")
            .into_iter()
            .map(|h| (h.held, h.by))
            .collect();
        assert_eq!(
            held,
            [("sda", "md0"), ("sda1", "dm-0"), ("sda2", "dm-1")]
                .map(|(a, b)| (a.to_string(), b.to_string()))
        );
    }

    // -------------------------------------------------------------------------
    // Integration tests (require actual Linux system)
    // -------------------------------------------------------------------------

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_get_mount_info_real() {
        let mounts = get_mount_info();
        assert!(mounts.is_ok(), "Should be able to read /proc/mounts");

        let mounts = mounts.unwrap();
        // There should be at least the root mount
        assert!(
            mounts
                .values()
                .flatten()
                .any(|info| info.mount_point == "/"),
            "Should find root mount point"
        );
        // Root should have a filesystem
        let root_info = mounts
            .values()
            .flatten()
            .find(|info| info.mount_point == "/");
        assert!(root_info.is_some());
        assert!(root_info.unwrap().filesystem.is_some());
    }

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_get_partition_labels_real() {
        // This test just verifies the function doesn't panic
        // Labels may or may not exist on the system
        let labels = get_partition_labels();
        // If there are labels, they should map to valid device paths
        for (device, label) in &labels {
            assert!(
                device.starts_with("/dev/"),
                "Device should start with /dev/"
            );
            assert!(!label.is_empty(), "Label should not be empty");
        }
    }

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_list_drives_real() {
        let drives = list_drives();
        assert!(drives.is_ok(), "Should be able to list drives");

        let drives = drives.unwrap();
        // On most Linux systems, there's at least one drive
        assert!(!drives.is_empty(), "Should find at least one drive");
    }

    // -------------------------------------------------------------------------
    // USB speed detection tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_usb_speed_from_mbps() {
        assert_eq!(UsbSpeed::from_mbps(1), UsbSpeed::Low);
        assert_eq!(UsbSpeed::from_mbps(12), UsbSpeed::Full);
        assert_eq!(UsbSpeed::from_mbps(480), UsbSpeed::High);
        assert_eq!(UsbSpeed::from_mbps(5000), UsbSpeed::SuperSpeed);
        assert_eq!(UsbSpeed::from_mbps(10000), UsbSpeed::SuperSpeedPlus);
        assert_eq!(UsbSpeed::from_mbps(20000), UsbSpeed::SuperSpeedPlus20);
    }

    #[test]
    fn test_usb_speed_is_slow() {
        assert!(UsbSpeed::Low.is_slow());
        assert!(UsbSpeed::Full.is_slow());
        assert!(UsbSpeed::High.is_slow());
        assert!(!UsbSpeed::SuperSpeed.is_slow());
        assert!(!UsbSpeed::SuperSpeedPlus.is_slow());
        assert!(!UsbSpeed::SuperSpeedPlus20.is_slow());
        assert!(!UsbSpeed::Unknown.is_slow());
    }

    #[test]
    fn test_usb_speed_display() {
        assert_eq!(UsbSpeed::High.to_string(), "USB 2.0 (480 Mbps)");
        assert_eq!(UsbSpeed::SuperSpeed.to_string(), "USB 3.0 (5 Gbps)");
    }

    #[test]
    fn test_detect_usb_speed_nonexistent() {
        // Non-existent path should return None
        assert!(detect_usb_speed("/sys/block/nonexistent").is_none());
    }

    #[test]
    fn test_detect_usb_speed_uses_nearest_usb_device() {
        // The enclosure negotiated 10 Gbps. Its root hub advertises 20 Gbps.
        let root = TempDir::new().unwrap();
        let sys_path = usb_attached_disk(root.path());

        assert_eq!(detect_usb_speed(&sys_path), Some(UsbSpeed::SuperSpeedPlus));
    }
}
