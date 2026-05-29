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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn config_file_lives_under_config_home() {
        let file = config_file();
        assert!(file.ends_with("nohrs/config.toml"));
    }

    #[test]
    fn absolute_xdg_var_is_honoured() {
        // Guard against other tests mutating the same process env concurrently
        // by setting and reading within the same call sequence.
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test-abs");
        let dir = config_dir();
        std::env::remove_var("XDG_CONFIG_HOME");
        assert_eq!(dir, PathBuf::from("/tmp/xdg-test-abs/nohrs"));
    }
}
