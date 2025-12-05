//! Windows platform implementation

use super::*;

/// Windows implementation of DeviceOps
pub struct WindowsPlatform;

impl DeviceOps for WindowsPlatform {
    fn open_device(_path: &str) -> Result<Box<dyn RawDevice>> {
        todo!("Implement Windows device opening")
    }

    fn unmount_device(_path: &str) -> Result<()> {
        todo!("Implement Windows unmount")
    }

    fn sync_device(_path: &str) -> Result<()> {
        todo!("Implement Windows sync")
    }

    fn has_elevated_privileges() -> bool {
        todo!("Implement Windows privilege check")
    }
}
