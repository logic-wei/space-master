use crate::fsutil::volume::{volume_info, VolumeInfo};
use crate::model::error::{AppError, AppResult};

/// Reports capacity for the volume holding the home directory. Deliberately not
/// "/": on APFS that is the sealed system volume, while everything we clean lives
/// on the data volume.
#[tauri::command]
pub fn get_volume_info() -> AppResult<VolumeInfo> {
    let home = std::env::home_dir().ok_or_else(|| AppError::InvalidPath("$HOME".to_string()))?;
    volume_info(&home)
}
