//! Linux platform implementation

use super::*;

/// Linux implementation of DeviceOps
pub struct LinuxPlatform;

impl DeviceOps for LinuxPlatform {
    fn open_device(_path: &str) -> Result<Box<dyn RawDevice>> {
        todo!("Implement Linux device opening")
    }

    fn unmount_device(_path: &str) -> Result<()> {
        todo!("Implement Linux unmount")
    }

    fn sync_device(_path: &str) -> Result<()> {
        todo!("Implement Linux sync")
    }

    fn has_elevated_privileges() -> bool {
        todo!("Implement Linux privilege check")
    }
}
