use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use anyhow::{Result, anyhow};
use windows::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
    SHFILEOPSTRUCTW, SHFileOperationW,
};
use windows::core::PCWSTR;

pub fn move_to_recycle_bin(path: &Path) -> Result<()> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.extend([0, 0]);
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_ptr()),
        fFlags: (FOF_ALLOWUNDO
            | FOF_NOCONFIRMATION
            | FOF_NOERRORUI
            | FOF_SILENT
            | FOF_WANTNUKEWARNING)
            .0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 || operation.fAnyOperationsAborted.as_bool() {
        return Err(anyhow!(
            "failed to move {} to the recycle bin (code {code})",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_leaves_its_folder() {
        let path = std::env::temp_dir().join(format!("replay-recycle-{}.txt", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        move_to_recycle_bin(&path).unwrap();
        assert!(!path.exists());
    }
}
