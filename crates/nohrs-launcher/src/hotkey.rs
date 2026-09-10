//! The OS-global summon key.
//!
//! docs/launcher.md §2: the launcher answers a key pressed *anywhere*, not only
//! while nohrs is the front application. That is outside GPUI's keymap, which
//! only ever sees events the OS has already routed to a nohrs window, so the
//! registration goes through the platform instead — Carbon hot keys on macOS, an
//! X11 passive grab on Linux, both via the `global-hotkey` crate.
//!
//! The in-app `Cmd+K` binding is the other half of §2 and lives in the binary;
//! this one works with nohrs in the background.

use std::time::Duration;

use anyhow::{Context as _, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{App, Global};

/// How often the hotkey channel is drained.
///
/// `global-hotkey` delivers presses to a channel of its own, off GPUI's event
/// loop, so they are collected by polling. 40ms keeps the summon well inside the
/// 100ms budget of docs/launcher.md §12 at the cost of one cheap wake-up per
/// interval.
const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Holds the registration for the process's lifetime.
///
/// Dropping the manager unregisters the key with the OS, so it is parked in a
/// GPUI global rather than left to fall off the end of the installing function.
struct Registration {
    _manager: GlobalHotKeyManager,
}

impl Global for Registration {}

/// The default summon chord.
///
/// `Cmd+Space` belongs to Spotlight and is deliberately left alone
/// (docs/launcher.md §2), so macOS gets `Cmd+Shift+Space` and every other
/// platform the same shape with its own command key, `Ctrl+Shift+Space`.
pub fn default_hotkey() -> HotKey {
    let modifiers = if cfg!(target_os = "macos") {
        Modifiers::META | Modifiers::SHIFT
    } else {
        Modifiers::CONTROL | Modifiers::SHIFT
    };
    HotKey::new(Some(modifiers), Code::Space)
}

/// Registers the global summon key and calls `on_summon` whenever it is pressed.
///
/// Returns the registered chord so the caller can report it. Failure is the
/// caller's to survive rather than fatal: the key can be unavailable because
/// another application already owns it, because there is no display to grab on,
/// or because the session is Wayland, where a global grab needs a desktop portal
/// that this crate does not speak. In every one of those cases nohrs should
/// still run — the in-app binding keeps working — so the error is returned for
/// the caller to log rather than propagated as a startup failure.
pub fn install(cx: &mut App, on_summon: impl Fn(&mut App) + 'static) -> Result<HotKey> {
    let hotkey = default_hotkey();
    let manager =
        GlobalHotKeyManager::new().context("could not reach the OS global-hotkey service")?;
    manager
        .register(hotkey)
        .with_context(|| format!("could not register the global hotkey {hotkey:?}"))?;
    cx.set_global(Registration { _manager: manager });

    let hotkey_id = hotkey.id();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;

            // Drain the channel and collapse a burst into a single summon: a
            // held key repeats, and toggling once per repeat would leave the
            // launcher flickering open and shut.
            let mut summoned = false;
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == hotkey_id && event.state == HotKeyState::Pressed {
                    summoned = true;
                }
            }

            if summoned && cx.update(|cx| on_summon(cx)).is_err() {
                // The application is gone; nothing left to summon into.
                break;
            }
        }
    })
    .detach();

    Ok(hotkey)
}

/// Renders a chord the way its platform's keyboards label it, for logs and UI.
pub fn describe(hotkey: &HotKey) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if hotkey.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        parts.push(if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Super"
        });
    }
    if hotkey.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        parts.push(if cfg!(target_os = "macos") {
            "Option"
        } else {
            "Alt"
        });
    }
    if hotkey.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift");
    }
    let key = match hotkey.key {
        Code::Space => "Space".to_string(),
        other => format!("{other:?}"),
    };
    parts.push(&key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_chord_leaves_spotlight_alone() {
        let hotkey = default_hotkey();
        assert_eq!(hotkey.key, Code::Space);
        // Shift is what keeps the chord clear of macOS's Cmd+Space.
        assert!(hotkey.mods.contains(Modifiers::SHIFT));

        if cfg!(target_os = "macos") {
            assert!(hotkey.mods.contains(Modifiers::META));
            assert!(!hotkey.mods.contains(Modifiers::CONTROL));
        } else {
            assert!(hotkey.mods.contains(Modifiers::CONTROL));
        }
    }

    #[test]
    fn a_chord_is_described_in_platform_terms() {
        let described = describe(&default_hotkey());
        assert!(described.ends_with("Space"), "{described}");
        assert!(described.contains("Shift"), "{described}");
        if cfg!(target_os = "macos") {
            assert_eq!(described, "Cmd+Shift+Space");
        } else {
            assert_eq!(described, "Ctrl+Shift+Space");
        }
    }
}
