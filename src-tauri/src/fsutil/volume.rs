use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::model::error::{AppError, AppResult};

/// Capacity of an APFS volume, reported so that `used_bytes + available_bytes`
/// always equals `total_bytes`.
///
/// The numbers here describe the whole APFS *container*, not just this volume:
/// `statfs` on the Data volume counts blocks consumed by the System, Preboot,
/// Recovery and VM volumes as unavailable. Apple's `df` subtracts those from its
/// Used column, which is why our `used_bytes` reads roughly 20 GB higher than
/// `df` — matching it would mean under-reporting what actually fills the disk.
///
/// `available_bytes` is the one figure every tool agrees on, and the one that
/// grows when we delete something, so it is the basis for "space reclaimed".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub mount_point: PathBuf,
    pub total_bytes: u64,
    /// Everything not available to the user: this volume's files, other volumes
    /// in the container, and reserved blocks. Defined as total - available.
    pub used_bytes: u64,
    /// `f_bavail`: blocks an unprivileged user can actually consume. Matches the
    /// Avail column of `df` and the figure Finder shows.
    pub available_bytes: u64,
}

pub fn volume_info(path: &Path) -> AppResult<VolumeInfo> {
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::InvalidPath(path.display().to_string()))?;

    // SAFETY: statfs only writes into the out-param we supply, which has the
    // correct type and size.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return Err(AppError::Io(std::io::Error::last_os_error()));
    }

    let bs = st.f_bsize as u64;
    let total = st.f_blocks.saturating_mul(bs);
    let available = st.f_bavail.saturating_mul(bs);

    Ok(VolumeInfo {
        mount_point: path.to_path_buf(),
        total_bytes: total,
        used_bytes: total.saturating_sub(available),
        available_bytes: available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_plausible_numbers_for_root() {
        let v = volume_info(Path::new("/")).unwrap();
        assert!(v.total_bytes > 0);
        assert!(v.available_bytes <= v.total_bytes);
        assert_eq!(v.used_bytes + v.available_bytes, v.total_bytes);
    }

    #[test]
    fn missing_path_is_an_error() {
        assert!(volume_info(Path::new("/nope/definitely/not/here")).is_err());
    }
}
