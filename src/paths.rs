//! User data and cache paths, with environment overrides for isolated runs.

use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    override_or_home("LUMEN_CONFIG_DIR", ".config/lumen")
}

pub fn data_dir() -> PathBuf {
    override_or_home("LUMEN_DATA_DIR", ".local/share/lumen")
}

pub fn plugin_config_path() -> PathBuf {
    config_dir().join("plugins.toml")
}

/// CEF requires request-context cache paths to be direct children of its root cache path.
pub fn cef_root_dir() -> PathBuf {
    cef_root_from_data_dir(data_dir())
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
    cef_root.join(format!("space-{sanitized_id}"))
}

/// General Lumen cache location, reserved for non-profile caches.
#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    resolve_cache_dir(
        std::env::var_os("LUMEN_CACHE_DIR"),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
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
            .join("lumen/cef")
    })
}

fn override_or_home(variable: &str, default_suffix: &str) -> PathBuf {
    resolve_override_or_home(
        std::env::var_os(variable),
        std::env::var_os("HOME"),
        default_suffix,
    )
}

fn resolve_override_or_home(
    override_value: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    suffix: &str,
) -> PathBuf {
    override_value.map(PathBuf::from).unwrap_or_else(|| {
        home.map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(suffix)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_config_and_data_directories_are_used() {
        assert_eq!(
            resolve_override_or_home(Some("/tmp/config".into()), None, ".config/lumen"),
            PathBuf::from("/tmp/config")
        );
        assert_eq!(
            resolve_override_or_home(Some("/tmp/data".into()), None, ".local/share/lumen"),
            PathBuf::from("/tmp/data")
        );
    }

    #[test]
    fn config_and_data_defaults_are_under_home() {
        assert_eq!(
            resolve_override_or_home(None, Some("/home/test".into()), ".config/lumen"),
            PathBuf::from("/home/test/.config/lumen")
        );
        assert_eq!(
            resolve_override_or_home(None, Some("/home/test".into()), ".local/share/lumen"),
            PathBuf::from("/home/test/.local/share/lumen")
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
            PathBuf::from("/home/test/.cache/lumen/cef")
        );
        assert_eq!(
            resolve_cache_dir(None, None, Some("/home/test".into()), PathBuf::from("/tmp")),
            PathBuf::from("/home/test/.cache/lumen/cef")
        );
    }

    #[test]
    fn cef_space_profiles_are_direct_children_of_the_cef_root() {
        let data_dir =
            resolve_override_or_home(Some("/tmp/lumen-data".into()), None, ".local/share/lumen");
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
}
