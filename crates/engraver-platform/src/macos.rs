//! macOS platform implementation

use super::*;

/// macOS implementation of DeviceOps
pub struct MacosPlatform;

impl DeviceOps for MacosPlatform {
    fn open_device(_path: &str) -> Result<Box<dyn RawDevice>> {
        todo!("Implement macOS device opening")
    }

    fn unmount_device(_path: &str) -> Result<()> {
        todo!("Implement macOS unmount")
    }

    fn sync_device(_path: &str) -> Result<()> {
        todo!("Implement macOS sync")
    }

    fn has_elevated_privileges() -> bool {
        todo!("Implement macOS privilege check")
    }
}
