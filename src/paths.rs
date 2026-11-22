//! User data and cache paths, with environment overrides for isolated runs.

use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    override_or_home("ANARC_CONFIG_DIR", ".config/anarc")
}

pub fn data_dir() -> PathBuf {
    override_or_home("ANARC_DATA_DIR", ".local/share/anarc")
}

pub fn plugin_config_path() -> PathBuf {
    config_dir().join("plugins.toml")
}

/// CEF requires request-context cache paths to be direct children of its root cache path.
///
/// Two windows can run at once, but they must never share one CEF profile:
/// Chromium puts a process-singleton lock in `--user-data-dir` (the root cache
/// path) and a second process opening the same cookies/cache database corrupts
/// it. The launcher claims a slot per window; slot 1 keeps the default profile
/// and every later slot gets its own root, so the rest of the user data
/// (settings, spaces, history) stays shared like a second browser window.
fn normalize_path_separators(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy().replace('/', "\\");
        PathBuf::from(s)
    }
    #[cfg(not(windows))]
    {
        path
    }
}

pub fn cef_root_dir() -> PathBuf {
    normalize_path_separators(instance_cef_root(
        cef_root_from_data_dir(data_dir()),
        resolve_instance_slot(std::env::var_os("ANARC_INSTANCE_SLOT")),
    ))
}

const MAX_INSTANCE_SLOTS: u32 = 9;

fn resolve_instance_slot(value: Option<std::ffi::OsString>) -> Option<u32> {
    let slot = value?.to_str()?.trim().parse::<u32>().ok()?;
    (2..=MAX_INSTANCE_SLOTS).contains(&slot).then_some(slot)
}

fn instance_cef_root(base: PathBuf, slot: Option<u32>) -> PathBuf {
    match slot {
        Some(slot) => base.join(format!("instance-{slot}")),
        None => base,
    }
}

pub fn profile_dir(space_id: &str) -> PathBuf {
    profile_from_cef_root(cef_root_dir(), space_id)
}

fn cef_root_from_data_dir(data_dir: PathBuf) -> PathBuf {
    data_dir.join("cef")
}

fn profile_from_cef_root(cef_root: PathBuf, space_id: &str) -> PathBuf {
    let sanitized_id = space_id
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect::<String>();
    normalize_path_separators(cef_root.join(format!("space-{sanitized_id}")))
}

/// General Lumen cache location, reserved for non-profile caches.
#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    resolve_cache_dir(
        std::env::var_os("ANARC_CACHE_DIR"),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
        std::env::temp_dir(),
    )
}

#[allow(dead_code)]
fn resolve_cache_dir(
    override_value: Option<std::ffi::OsString>,
    xdg_cache: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    temp_dir: PathBuf,
) -> PathBuf {
    override_value.map(PathBuf::from).unwrap_or_else(|| {
        xdg_cache
            .map(PathBuf::from)
            .or_else(|| home.map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or(temp_dir)
            .join("anarc/cef")
    })
}

fn override_or_home(variable: &str, default_suffix: &str) -> PathBuf {
    resolve_override_or_home(
        std::env::var_os(variable),
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
        default_suffix,
    )
}

fn resolve_override_or_home(
    override_value: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    suffix: &str,
) -> PathBuf {
    let path = override_value.map(PathBuf::from).unwrap_or_else(|| {
        let mut base = home.map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        for part in suffix.split('/') {
            base.push(part);
        }
        base
    });
    normalize_path_separators(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_config_and_data_directories_are_used() {
        assert_eq!(
            resolve_override_or_home(Some("/tmp/config".into()), None, ".config/anarc"),
            PathBuf::from("/tmp/config")
        );
        assert_eq!(
            resolve_override_or_home(Some("/tmp/data".into()), None, ".local/share/anarc"),
            PathBuf::from("/tmp/data")
        );
    }

    #[test]
    fn config_and_data_defaults_are_under_home() {
        assert_eq!(
            resolve_override_or_home(None, Some("/home/test".into()), ".config/anarc"),
            PathBuf::from("/home/test/.config/anarc")
        );
        assert_eq!(
            resolve_override_or_home(None, Some("/home/test".into()), ".local/share/anarc"),
            PathBuf::from("/home/test/.local/share/anarc")
        );
    }

    #[test]
    fn explicit_cache_directory_is_used_for_non_profile_caches() {
        assert_eq!(
            resolve_cache_dir(
                Some("/tmp/cache".into()),
                None,
                None,
                PathBuf::from("/var/tmp")
            ),
            PathBuf::from("/tmp/cache")
        );
    }

    #[test]
    fn cache_default_preserves_xdg_and_home_resolution() {
        assert_eq!(
            resolve_cache_dir(
                None,
                Some("/home/test/.cache".into()),
                None,
                PathBuf::from("/tmp")
            ),
            PathBuf::from("/home/test/.cache/anarc/cef")
        );
        assert_eq!(
            resolve_cache_dir(None, None, Some("/home/test".into()), PathBuf::from("/tmp")),
            PathBuf::from("/home/test/.cache/anarc/cef")
        );
    }

    #[test]
    fn cef_space_profiles_are_direct_children_of_the_cef_root() {
        let data_dir =
            resolve_override_or_home(Some("/tmp/lumen-data".into()), None, ".local/share/anarc");
        let cef_root = cef_root_from_data_dir(data_dir);
        let profile = profile_from_cef_root(cef_root.clone(), "personal");

        assert_eq!(cef_root, PathBuf::from("/tmp/lumen-data/cef"));
        assert_eq!(profile, PathBuf::from("/tmp/lumen-data/cef/space-personal"));
    }

    #[test]
    fn cef_space_profile_ids_are_sanitized_as_single_path_components() {
        assert_eq!(
            profile_from_cef_root(PathBuf::from("/tmp/lumen-data/cef"), "../personal"),
            PathBuf::from("/tmp/lumen-data/cef/space-%2E%2E%2Fpersonal")
        );
    }

    #[test]
    fn only_secondary_launch_slots_are_accepted() {
        assert_eq!(resolve_instance_slot(Some("2".into())), Some(2));
        assert_eq!(resolve_instance_slot(Some("9".into())), Some(9));
        assert_eq!(resolve_instance_slot(Some("1".into())), None);
        assert_eq!(resolve_instance_slot(Some("0".into())), None);
        assert_eq!(resolve_instance_slot(Some("10".into())), None);
        assert_eq!(resolve_instance_slot(Some("second".into())), None);
        assert_eq!(resolve_instance_slot(None), None);
    }

    #[test]
    fn secondary_slots_get_their_own_cef_root_with_direct_child_profiles() {
        let base = PathBuf::from("/tmp/lumen-data/cef");
        assert_eq!(instance_cef_root(base.clone(), None), base);
        let secondary = instance_cef_root(base, Some(2));
        assert_eq!(secondary, PathBuf::from("/tmp/lumen-data/cef/instance-2"));
        assert_eq!(
            profile_from_cef_root(secondary, "personal"),
            PathBuf::from("/tmp/lumen-data/cef/instance-2/space-personal")
        );
    }
}

#[cfg(unix)]
pub(crate) mod agent_socket {
    use super::{PathBuf, resolve_instance_slot};
    use std::ffi::OsString;
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Seek, Write};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    unsafe extern "C" {
        fn getuid() -> u32;
        fn kill(pid: i32, signal: i32) -> i32;
    }

    // `resolved` and `environment` are consumed by the anarc-ctl binary, which
    // includes this file via #[path]; the browser binary only needs `bind`.
    #[allow(dead_code)]
    pub(crate) struct Discovery {
        pub(crate) resolved: PathBuf,
        pub(crate) bind: PathBuf,
        pub(crate) environment: [(&'static str, Option<OsString>); 6],
    }

    pub(crate) fn resolve_socket(explicit: Option<&Path>, instance: Option<u32>) -> Discovery {
        let environment = [
            "ANARC_CTL_SOCKET", "ANARC_SOCKET_PATH", "XDG_RUNTIME_DIR",
            "ANARC_DATA_DIR", "HOME", "ANARC_INSTANCE_SLOT",
        ].map(|name| (name, std::env::var_os(name)));
        let [override_path, legacy_override, runtime, data, home, slot] =
            environment.each_ref().map(|(_, value)| value.as_deref());
        if let Some(path) = explicit.or_else(|| override_path.or(legacy_override).map(Path::new)) {
            return Discovery { resolved: path.into(), bind: path.into(), environment };
        }

        let instance = instance.or_else(|| resolve_instance_slot(slot.map(OsString::from)));
        let filename = match instance {
            Some(slot) if slot > 1 => format!("ctl-{slot}.sock"),
            _ => "ctl.sock".into(),
        };
        let runtime = runtime.map(PathBuf::from);
        let uid = unsafe { getuid() };
        let standard_runtime = PathBuf::from(format!("/run/user/{uid}"));
        let default_data = home.map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
            .join(".local/share/anarc");
        let data_socket = data.map(PathBuf::from).map(|dir| dir.join("agent").join(&filename));
        let home_socket = default_data.join("agent").join(&filename);
        let runtime_socket = runtime.as_ref().map(|dir| dir.join("anarc").join(&filename));

        // Explicit data directories keep isolated browser runs independent.
        let bind = data_socket.as_ref().or(runtime_socket.as_ref()).unwrap_or(&home_socket).clone();
        let directories = [
            runtime.as_ref().map(|dir| dir.join("anarc")),
            data_socket.as_ref().and_then(|path| path.parent()).map(Path::to_owned),
            home_socket.parent().map(Path::to_owned),
            runtime.is_none().then(|| standard_runtime.join("anarc")),
        ];
        let candidates = directories.iter().flatten().map(|dir| dir.join(&filename));
        let is_socket = |path: &Path| fs::symlink_metadata(path).is_ok_and(|metadata| {
            metadata.file_type().is_socket() && metadata.uid() == uid
        });
        let mut resolved = candidates.into_iter().find(|path| is_socket(path));
        if resolved.is_none() && instance.is_none() {
            // A harness can lack the launcher's slot. A unique instance remains discoverable.
            let mut sockets: Vec<_> = directories.iter().flatten()
                .flat_map(|dir| (2..=super::MAX_INSTANCE_SLOTS).map(move |slot| dir.join(format!("ctl-{slot}.sock"))))
                .filter(|path| is_socket(path))
                .collect();
            sockets.sort_unstable();
            sockets.dedup();
            if sockets.len() == 1 {
                resolved = sockets.pop();
            }
        }
        if resolved.is_none() && instance.is_none_or(|slot| slot == 1) {
            let legacy_candidates = [
                Some(runtime.unwrap_or(standard_runtime).join("anarc-agent.sock")),
                Some(PathBuf::from(format!("/tmp/anarc-agent-{uid}.sock"))),
            ];
            resolved = legacy_candidates.into_iter().flatten().find(|path| is_socket(path));
        }
        let resolved = resolved.unwrap_or_else(|| bind.clone());
        Discovery { resolved, bind, environment }
    }

    fn owner_path(socket: &Path) -> PathBuf {
        let mut path = socket.as_os_str().to_os_string();
        path.push(".owner");
        path.into()
    }

    pub(crate) fn lock_owner(socket: &Path, create: bool) -> io::Result<File> {
        let file = OpenOptions::new().read(true).write(true).create(create)
            .mode(0o600).custom_flags(0o400000 /* O_NOFOLLOW */)
            .open(owner_path(socket))?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.uid() != unsafe { getuid() } {
            return Err(io::Error::other("invalid socket owner file"));
        }
        file.try_lock().map_err(io::Error::from)?;
        Ok(file)
    }

    pub(crate) fn record_owner(mut owner: &File, socket: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(socket)?;
        owner.set_len(0)?;
        owner.rewind()?;
        writeln!(owner, "{} {} {}", std::process::id(), metadata.dev(), metadata.ino())
    }

    pub(crate) fn remove_dead_socket(socket: &Path, owner: &File) -> bool {
        use std::io::Read;
        let mut record = String::new();
        if owner.take(128).read_to_string(&mut record).is_err() {
            return false;
        }
        let mut fields = record.split_whitespace();
        let (Some(pid), Some(dev), Some(ino), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return false;
        };
        let Ok(pid) = pid.parse::<i32>() else { return false };
        if pid <= 0 || unsafe { kill(pid, 0) } == 0 || io::Error::last_os_error().raw_os_error() != Some(3) {
            return false;
        }
        let Ok(metadata) = fs::symlink_metadata(socket) else { return false };
        if !metadata.file_type().is_socket()
            || metadata.uid() != unsafe { getuid() }
            || Some(metadata.dev()) != dev.parse().ok()
            || Some(metadata.ino()) != ino.parse().ok()
        {
            return false;
        }
        matches!(UnixStream::connect(socket), Err(error) if error.kind() == io::ErrorKind::ConnectionRefused)
            && fs::remove_file(socket).is_ok()
    }
}
