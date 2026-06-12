//! XDG-style path resolution for nohrs data locations.
//!
//! Per `docs/config.md` §1 we resolve `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME` /
//! `$XDG_CACHE_HOME` directly, falling back to `~/.config`, `~/.local/share`,
//! and `~/.cache`. We deliberately do **not** use `dirs::config_dir()` and
//! friends: on macOS those return OS-native paths (`~/Library/...`) which
//! conflict with the dotfile layout nohrs uses on every platform. `dirs` is
//! used only to locate the home directory for the fallback.

use std::path::PathBuf;

/// Application directory name used under each XDG base directory.
const APP_DIR: &str = "nohrs";

/// Resolve an XDG base directory: use the environment variable if it is set to
/// an absolute path (relative values are ignored per the XDG spec), otherwise
/// fall back to `~/<default_subdir>`. Falls back to the current directory if the
/// home directory cannot be determined, so callers always get a usable path.
fn xdg_base(env_var: &str, default_subdir: &str) -> PathBuf {
    if let Some(value) = std::env::var_os(env_var) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return path;
        }
    }
    match dirs::home_dir() {
        Some(home) => home.join(default_subdir),
        None => PathBuf::from(default_subdir),
    }
}

/// `$XDG_CONFIG_HOME` or `~/.config`.
pub fn config_home() -> PathBuf {
    xdg_base("XDG_CONFIG_HOME", ".config")
}

/// `$XDG_DATA_HOME` or `~/.local/share`.
pub fn data_home() -> PathBuf {
    xdg_base("XDG_DATA_HOME", ".local/share")
}

/// `$XDG_CACHE_HOME` or `~/.cache`.
pub fn cache_home() -> PathBuf {
    xdg_base("XDG_CACHE_HOME", ".cache")
}

/// The nohrs config directory (`.../nohrs`).
pub fn config_dir() -> PathBuf {
    config_home().join(APP_DIR)
}

/// The nohrs data directory (`.../nohrs`).
pub fn data_dir() -> PathBuf {
    data_home().join(APP_DIR)
}

/// The nohrs cache directory (`.../nohrs`).
pub fn cache_dir() -> PathBuf {
    cache_home().join(APP_DIR)
}

/// Full path to `config.toml`.
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
// Rust 2024 made `std::env::{set_var, remove_var}` unsafe; these tests must
// mutate the process environment to exercise XDG path resolution.
#[allow(clippy::unwrap_used, unsafe_code)]
mod tests {
    use super::*;
    use crate::config::test_env::env_lock;

    #[test]
    fn config_file_lives_under_config_home() {
        // Reads `XDG_CONFIG_HOME`, so it must serialize with the mutating tests.
        let _guard = env_lock();
        let file = config_file();
        assert!(file.ends_with("nohrs/config.toml"));
    }

    #[test]
    fn absolute_xdg_var_is_honoured() {
        let _guard = env_lock();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test-abs") };
        let dir = config_dir();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        assert_eq!(dir, PathBuf::from("/tmp/xdg-test-abs/nohrs"));
    }

    #[test]
    fn relative_xdg_var_is_ignored() {
        let _guard = env_lock();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "relative/path") };
        let home = config_home();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        // A relative value is ignored per the XDG spec, so we fall back to a home
        // (or cwd) based path, never the relative value itself.
        assert!(!home.ends_with("relative/path"));
    }

    #[test]
    fn each_home_honours_its_own_xdg_var() {
        let _guard = env_lock();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "/tmp/cfg") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("XDG_DATA_HOME", "/tmp/data") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("XDG_CACHE_HOME", "/tmp/cache") };
        let (config, data, cache) = (config_home(), data_home(), cache_home());
        for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"] {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::remove_var(key) };
        }
        assert_eq!(config, PathBuf::from("/tmp/cfg"));
        assert_eq!(data, PathBuf::from("/tmp/data"));
        assert_eq!(cache, PathBuf::from("/tmp/cache"));
    }

    #[test]
    fn app_dirs_append_app_name_and_config_file_is_under_config_dir() {
        // Reads the XDG-derived dirs, so serialize with the mutating tests.
        let _guard = env_lock();
        assert!(config_dir().ends_with("nohrs"));
        assert!(data_dir().ends_with("nohrs"));
        assert!(cache_dir().ends_with("nohrs"));
        assert_eq!(config_file(), config_dir().join("config.toml"));
    }
}
