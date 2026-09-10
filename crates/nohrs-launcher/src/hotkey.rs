//! The OS-global summon key.
//!
//! docs/launcher.md §2: the launcher answers a key pressed *anywhere*, not only
//! while nohrs is the front application. That is outside GPUI's keymap, which
//! only ever sees events the OS has already routed to a nohrs window, so the
//! registration goes through the platform instead.
//!
//! There are two ways to ask, and which one works depends on the session rather
//! than the operating system:
//!
//! * A **passive grab** — Carbon hot keys on macOS, `XGrabKey` on X11, a
//!   low-level hook on Windows — via the `global-hotkey` crate. Synchronous, and
//!   the chord is whatever the application asked for.
//! * The **desktop portal** on Wayland, where a grab does not exist by design.
//!   The compositor owns the binding and the user approves it. See [`portal`].
//!
//! [`install`] picks between them. The in-app `Cmd+K` binding is the other half
//! of §2 and lives in the binary; this one works with nohrs in the background.

/// The Wayland path to a global shortcut.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod portal;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
// Only the portal backend reaches for `background_spawn`; the grab backend uses
// `App`'s own methods, so on a platform without the portal this trait would be
// an unused import.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use gpui::AppContext as _;
use gpui::{App, Global};

/// How often the grab backend's channel is drained.
///
/// `global-hotkey` delivers presses to a channel of its own, off GPUI's event
/// loop, so they are collected by polling. 40ms keeps the summon well inside the
/// 100ms budget of docs/launcher.md §12 at the cost of one cheap wake-up per
/// interval. The portal backend needs no equivalent: its activations arrive as
/// D-Bus signals that wake the task on their own.
const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Holds the grab registration for the process's lifetime.
///
/// Dropping the manager unregisters the key with the OS, so it is parked in a
/// GPUI global rather than left to fall off the end of the installing function.
struct Registration {
    _manager: GlobalHotKeyManager,
}

impl Global for Registration {}

/// Reports, on the foreground, whether the global shortcut is actually in
/// place. Boxed because the portal path carries it across an await.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
type OutcomeReport = Box<dyn FnOnce(&mut App, Result<()>)>;

/// Which mechanism ended up carrying the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// An OS-level passive grab: macOS, X11, or Windows.
    Grab,
    /// The XDG desktop portal, on Wayland.
    Portal,
}

/// The summon chord, in the form each backend needs.
///
/// The two spellings are kept together, and checked against each other in the
/// tests, because they are the same chord described twice: `global-hotkey`
/// wants modifier bitflags and a `Code`, the portal wants a string in the XDG
/// shortcuts syntax.
#[derive(Debug, Clone, Copy)]
pub struct Chord {
    /// The chord as the grab backends take it.
    pub hotkey: HotKey,
    /// The chord as the portal takes it, per the XDG shortcuts specification.
    pub portal_trigger: &'static str,
}

/// The default summon chord.
///
/// `Cmd+Space` belongs to Spotlight and is deliberately left alone
/// (docs/launcher.md §2), so macOS gets `Cmd+Shift+Space` and every other
/// platform the same shape with its own command key, `Ctrl+Shift+Space`.
pub fn default_chord() -> Chord {
    if cfg!(target_os = "macos") {
        Chord {
            hotkey: HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Space),
            portal_trigger: "LOGO+SHIFT+space",
        }
    } else {
        Chord {
            hotkey: HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space),
            portal_trigger: "CTRL+SHIFT+space",
        }
    }
}

/// Registers the global summon key and calls `on_summon` whenever it fires.
///
/// A Wayland session gets the portal and nothing else. Falling back to a grab
/// there would be worse than failing: made from an XWayland client it binds
/// successfully and then only fires over X11 windows, so the shortcut would look
/// like it worked and silently do nothing over native applications. Everywhere
/// else the grab is both the right answer and the reliable one.
///
/// Returns which backend took it. Failure is the caller's to survive rather than
/// fatal: the chord can be unavailable because another application owns it,
/// because there is no display to grab on, or because the user declined the
/// portal's request. nohrs should still run in all of those cases — the in-app
/// binding keeps working — so the error is returned for the caller to log.
///
/// Note that the portal path returns as soon as the request is *sent*. The
/// binding is confirmed asynchronously and reported from its own task, because
/// the desktop may be putting the request in front of the user first.
pub fn install(
    cx: &mut App,
    on_summon: impl Fn(&mut App) + 'static,
    on_outcome: impl FnOnce(&mut App, Result<()>) + 'static,
) -> Result<Backend> {
    let chord = default_chord();

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if portal::is_wayland_session() {
        install_portal(cx, chord, Rc::new(on_summon), Box::new(on_outcome));
        return Ok(Backend::Portal);
    }

    install_grab(cx, chord, on_summon)?;
    on_outcome(cx, Ok(()));
    Ok(Backend::Grab)
}

/// Registers the chord with `global-hotkey` and polls its channel.
fn install_grab(cx: &mut App, chord: Chord, on_summon: impl Fn(&mut App) + 'static) -> Result<()> {
    let manager =
        GlobalHotKeyManager::new().context("could not reach the OS global-hotkey service")?;
    manager
        .register(chord.hotkey)
        .with_context(|| format!("could not register the global hotkey {}", describe(&chord)))?;
    cx.set_global(Registration { _manager: manager });

    let hotkey_id = chord.hotkey.id();
    cx.spawn(async move |cx| {
        // A held chord auto-repeats `Pressed` for as long as it is down. Summon
        // on the edge into "pressed" only, so holding the key opens the launcher
        // once instead of toggling it every poll interval.
        let mut held = false;
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;

            let mut summoned = false;
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id != hotkey_id {
                    continue;
                }
                match event.state {
                    HotKeyState::Pressed if !held => {
                        held = true;
                        summoned = true;
                    }
                    HotKeyState::Pressed => {}
                    HotKeyState::Released => held = false,
                }
            }

            if summoned && cx.update(|cx| on_summon(cx)).is_err() {
                // The application is gone; nothing left to summon into.
                break;
            }
        }
    })
    .detach();

    Ok(())
}

/// Starts the portal conversation and forwards its activations.
///
/// The D-Bus session talks over threads, so it runs on the background executor;
/// `on_summon` opens windows and therefore cannot leave the foreground. Only
/// bare notifications cross between them, over channels.
///
/// `on_outcome` is called once, on the foreground, with whether the binding was
/// granted. The caller cannot learn that synchronously — the desktop may be
/// asking the user first — and a launcher that has no way to be summoned is not
/// something to discover in a log file.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn install_portal(
    cx: &mut App,
    chord: Chord,
    on_summon: Rc<dyn Fn(&mut App)>,
    on_outcome: OutcomeReport,
) {
    // Bounded at one: a burst of activations arriving while the foreground is
    // busy should collapse into a single summon rather than queue up toggles.
    let (sender, receiver) = async_channel::bounded::<()>(1);
    let (bound_sender, bound_receiver) = async_channel::bounded::<Result<()>>(1);

    cx.background_spawn({
        let bound_sender = bound_sender.clone();
        async move {
            let result = portal::listen(
                chord.portal_trigger,
                || {
                    bound_sender.try_send(Ok(())).ok();
                },
                || {
                    // A full channel already holds an unhandled summon, so
                    // dropping this one is coalescing, not a lost event.
                    sender.try_send(()).ok();
                },
            )
            .await;
            if let Err(error) = result {
                // Only reported if the binding never succeeded; a later failure
                // (the portal hanging up) finds the channel already used.
                bound_sender.try_send(Err(error)).ok();
            }
        }
    })
    .detach();

    cx.spawn(async move |cx| {
        if let Ok(outcome) = bound_receiver.recv().await {
            if cx.update(|cx| on_outcome(cx, outcome)).is_err() {
                return;
            }
        }
        // Ends when the sender drops, which is when the portal task has given up.
        while receiver.recv().await.is_ok() {
            if cx.update(|cx| on_summon(cx)).is_err() {
                break; // The application is gone; nothing left to summon into.
            }
        }
    })
    .detach();
}

/// Renders a chord the way its platform's keyboards label it, for logs and UI.
pub fn describe(chord: &Chord) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if chord
        .hotkey
        .mods
        .intersects(Modifiers::SUPER | Modifiers::META)
    {
        parts.push(if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Super"
        });
    }
    if chord.hotkey.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if chord.hotkey.mods.contains(Modifiers::ALT) {
        parts.push(if cfg!(target_os = "macos") {
            "Option"
        } else {
            "Alt"
        });
    }
    if chord.hotkey.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift");
    }
    let key = match chord.hotkey.key {
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
        let chord = default_chord();
        assert_eq!(chord.hotkey.key, Code::Space);
        // Shift is what keeps the chord clear of macOS's Cmd+Space.
        assert!(chord.hotkey.mods.contains(Modifiers::SHIFT));

        if cfg!(target_os = "macos") {
            // Asserted the way `describe` reads it, not as the chord was built:
            // see `the_command_modifier_is_normalised_on_the_way_in`.
            assert!(
                chord
                    .hotkey
                    .mods
                    .intersects(Modifiers::SUPER | Modifiers::META)
            );
            assert!(!chord.hotkey.mods.contains(Modifiers::CONTROL));
        } else {
            assert!(chord.hotkey.mods.contains(Modifiers::CONTROL));
        }
    }

    #[test]
    fn the_command_modifier_is_normalised_on_the_way_in() {
        // `HotKey::new` rewrites META into SUPER, so a chord built from META does
        // not `contain(META)` afterwards. Anything reading the stored modifiers
        // has to accept either, which is why `describe` uses `intersects`.
        //
        // Pinned here rather than left implicit because the mistake is invisible
        // on Linux — nothing normalises CONTROL — and only surfaces on the one
        // platform that uses the command key.
        let built = HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Space);
        assert!(!built.mods.contains(Modifiers::META));
        assert!(built.mods.contains(Modifiers::SUPER));
        assert!(built.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn both_spellings_of_the_chord_agree() {
        let chord = default_chord();
        let trigger = chord.portal_trigger;

        // The portal spells modifiers CTRL / SHIFT / ALT / LOGO and keys in
        // lowercase, so the two forms cannot be compared directly — but every
        // modifier present in one must be present in the other.
        assert_eq!(
            trigger.contains("SHIFT"),
            chord.hotkey.mods.contains(Modifiers::SHIFT)
        );
        assert_eq!(
            trigger.contains("CTRL"),
            chord.hotkey.mods.contains(Modifiers::CONTROL)
        );
        assert_eq!(
            trigger.contains("LOGO"),
            chord
                .hotkey
                .mods
                .intersects(Modifiers::SUPER | Modifiers::META)
        );
        assert!(trigger.ends_with("space"), "{trigger}");
    }

    #[test]
    fn a_chord_is_described_in_platform_terms() {
        let described = describe(&default_chord());
        assert!(described.ends_with("Space"), "{described}");
        assert!(described.contains("Shift"), "{described}");
        if cfg!(target_os = "macos") {
            assert_eq!(described, "Cmd+Shift+Space");
        } else {
            assert_eq!(described, "Ctrl+Shift+Space");
        }
    }
}
