//! Resolves the current user's home directory with Unix account fallback.

use std::path::{Path, PathBuf};

/// Returns the best-effort home directory for the current process user.
pub fn user_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        select_preferred_unix_home_dir(env_home_dir(), account_home_dir())
    }
}

#[cfg(not(target_os = "windows"))]
fn env_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn account_home_dir() -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::mem;
    use std::os::unix::ffi::OsStringExt;
    use std::ptr;

    // Query the passwd database so root processes can still resolve `/root`
    // even when HOME is unset or incorrectly forced to `/`.
    unsafe {
        let uid = libc::geteuid();
        let buffer_size = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
            size if size < 0 => 512usize,
            size => size as usize,
        };
        let mut buffer = Vec::with_capacity(buffer_size);
        let mut passwd: libc::passwd = mem::zeroed();
        let mut result = ptr::null_mut();

        match libc::getpwuid_r(
            uid,
            &mut passwd,
            buffer.as_mut_ptr(),
            buffer.capacity(),
            &mut result,
        ) {
            0 if !result.is_null() => {
                let bytes = CStr::from_ptr(passwd.pw_dir).to_bytes();
                if bytes.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
                }
            }
            _ => None,
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn select_preferred_unix_home_dir(
    env_home: Option<PathBuf>,
    account_home: Option<PathBuf>,
) -> Option<PathBuf> {
    match (env_home, account_home) {
        (Some(env_home), Some(account_home))
            if env_home == Path::new("/") && account_home != env_home =>
        {
            Some(account_home)
        }
        (Some(env_home), _) => Some(env_home),
        (None, Some(account_home)) => Some(account_home),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::select_preferred_unix_home_dir;
    use std::path::{Path, PathBuf};

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn select_preferred_unix_home_dir_prefers_account_home_for_root_fs_home() {
        let selected =
            select_preferred_unix_home_dir(Some(PathBuf::from("/")), Some(PathBuf::from("/root")));

        assert_eq!(selected, Some(PathBuf::from("/root")));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn select_preferred_unix_home_dir_preserves_explicit_env_home() {
        let selected = select_preferred_unix_home_dir(
            Some(PathBuf::from("/srv/aor")),
            Some(PathBuf::from("/root")),
        );

        assert_eq!(selected, Some(PathBuf::from("/srv/aor")));
        assert_ne!(selected.as_deref(), Some(Path::new("/root")));
    }
}
