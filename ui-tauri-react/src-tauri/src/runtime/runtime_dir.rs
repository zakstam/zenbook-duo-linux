use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use nix::unistd::Uid;

use crate::runtime::paths;

pub fn ensure_target_user_runtime_dir() -> Result<(), String> {
    let (uid, gid) = target_identity();
    ensure_dir_with_owner(&paths::user_runtime_dir(uid), Some((uid, gid)))
}

pub fn ensure_current_user_runtime_dir() -> Result<(), String> {
    let dir = paths::current_user_runtime_dir();
    ensure_dir_with_owner(&dir, None)?;
    validate_current_user_dir(&dir)
}

pub fn ensure_dir_owned_like_parent(dir: &Path) -> Result<(), String> {
    let owner = dir
        .parent()
        .and_then(owner_from_metadata)
        .or_else(target_identity_opt);
    ensure_dir_with_owner(dir, owner)
}

fn ensure_dir_with_owner(dir: &Path, owner: Option<(u32, u32)>) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    if let Some((uid, gid)) = owner {
        let metadata =
            fs::metadata(dir).map_err(|e| format!("Failed to stat {}: {e}", dir.display()))?;
        if should_repair_ownership(
            Uid::current().as_raw(),
            metadata.uid(),
            metadata.gid(),
            uid,
            gid,
        ) {
            chown_path(dir, uid, gid)?;
        }
    }

    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Failed to set {} permissions: {e}", dir.display()))?;

    Ok(())
}

fn validate_current_user_dir(dir: &Path) -> Result<(), String> {
    let metadata =
        fs::metadata(dir).map_err(|e| format!("Failed to stat {}: {e}", dir.display()))?;
    let current_uid = Uid::current().as_raw();
    if metadata.uid() != current_uid {
        return Err(format!(
            "Session runtime dir {} is owned by uid {}, expected uid {}. \
This usually means a root-owned helper created it; restart the system daemon or repair ownership.",
            dir.display(),
            metadata.uid(),
            current_uid,
        ));
    }

    let mode = metadata.permissions().mode() & 0o700;
    if mode != 0o700 {
        return Err(format!(
            "Session runtime dir {} has mode {:o}, expected 700.",
            dir.display(),
            metadata.permissions().mode() & 0o777,
        ));
    }

    Ok(())
}

fn target_identity() -> (u32, u32) {
    let uid = std::env::var("ZENBOOK_DUO_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| Uid::current().as_raw());
    let gid = std::env::var("ZENBOOK_DUO_GID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(uid);
    (uid, gid)
}

fn target_identity_opt() -> Option<(u32, u32)> {
    let uid = std::env::var("ZENBOOK_DUO_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())?;
    let gid = std::env::var("ZENBOOK_DUO_GID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(uid);
    Some((uid, gid))
}

fn owner_from_metadata(path: &Path) -> Option<(u32, u32)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.uid(), metadata.gid()))
}

fn should_repair_ownership(
    running_uid: u32,
    current_uid: u32,
    current_gid: u32,
    target_uid: u32,
    target_gid: u32,
) -> bool {
    running_uid == 0 && (current_uid != target_uid || current_gid != target_gid)
}

fn chown_path(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    let path_cstr = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("Path contains interior NUL: {}", path.display()))?;
    let result = unsafe { libc::chown(path_cstr.as_ptr(), uid, gid) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "Failed to chown {} to {}:{}: {}",
            path.display(),
            uid,
            gid,
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zenbook-duo-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn creates_missing_runtime_dir_with_secure_mode() {
        let dir = temp_dir("create");
        ensure_dir_with_owner(&dir, None).unwrap();

        let metadata = fs::metadata(&dir).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repairs_existing_runtime_dir_permissions() {
        let dir = temp_dir("chmod");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        ensure_dir_with_owner(&dir, None).unwrap();

        let metadata = fs::metadata(&dir).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ownership_repair_only_runs_for_root_with_mismatched_owner() {
        assert!(should_repair_ownership(0, 0, 0, 1000, 1001));
        assert!(!should_repair_ownership(1000, 0, 0, 1000, 1001));
        assert!(!should_repair_ownership(0, 1000, 1001, 1000, 1001));
    }
}
