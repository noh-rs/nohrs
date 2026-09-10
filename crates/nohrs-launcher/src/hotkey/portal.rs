//! The Wayland path to a global shortcut: the XDG desktop portal.
//!
//! Wayland deliberately has no equivalent of an X11 passive grab — a client
//! cannot ask the compositor for keys it will receive while another window is
//! focused, because that is exactly the capability a keylogger wants. The
//! sanctioned route is `org.freedesktop.portal.GlobalShortcuts`: the application
//! *asks* for a shortcut, the desktop asks the user, and the compositor delivers
//! an `Activated` signal over D-Bus when it fires.
//!
//! That makes this a slower, chattier, and more failure-prone path than the X11
//! grab in the parent module, and one whose chord the user can overrule. It is
//! also the only one that works on a modern GNOME or KDE session, which is why
//! the launcher speaks it rather than telling Wayland users their hotkey does
//! nothing.
//!
//! The portal interface reached version 1 in xdg-desktop-portal 1.17 and is
//! implemented by GNOME 45+ and KDE Plasma 5.27+. Where it is missing the
//! request fails and there is no global shortcut at all: a grab would bind and
//! then only fire over XWayland windows, so the caller reports the failure and
//! leaves the in-app binding as the way in.

use anyhow::{Context as _, Result, bail};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt as _;

/// Identifier the portal files the binding under.
///
/// The desktop remembers a user's chosen trigger against this string, so it is
/// part of the app's persisted state: changing it loses their customisation.
pub const SHORTCUT_ID: &str = "toggle-launcher";

/// What the desktop's shortcut settings show next to the binding.
const DESCRIPTION: &str = "Open the nohrs launcher";

/// Whether this process is talking to a Wayland compositor.
///
/// `WAYLAND_DISPLAY` is what the Wayland client libraries themselves look for,
/// so it agrees with the session GPUI ends up in. Under XWayland it is also set,
/// which is the answer we want: an X11 grab made by an XWayland client only sees
/// keys pressed over X11 windows, so such a session needs the portal too.
pub fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Binds `trigger` with the portal and calls `on_activated` for every press.
///
/// Runs until the connection drops or the stream ends, so callers spawn it and
/// let it own its session: dropping the returned future closes the session and
/// unbinds the shortcut.
///
/// `trigger` is a *preferred* trigger in the syntax of the XDG shortcuts
/// specification (for example `CTRL+SHIFT+space`). The desktop is free to bind
/// something else — the user may already have that chord — so it is a request,
/// not an instruction.
pub async fn listen(trigger: &str, on_bound: impl FnOnce(), on_activated: impl Fn()) -> Result<()> {
    let shortcuts = GlobalShortcuts::new()
        .await
        .context("no GlobalShortcuts portal on the session bus")?;
    let session = shortcuts
        .create_session()
        .await
        .context("the portal refused a shortcuts session")?;

    // Subscribed before binding, not after: the compositor may deliver an
    // activation as soon as the binding exists, and a stream opened afterwards
    // would miss anything pressed in the gap.
    let mut activated = shortcuts
        .receive_activated()
        .await
        .context("could not subscribe to portal shortcut activations")?;

    let shortcut = NewShortcut::new(SHORTCUT_ID, DESCRIPTION).preferred_trigger(Some(trigger));
    let request = shortcuts
        .bind_shortcuts(&session, &[shortcut], None)
        .await
        .context("could not ask the portal to bind the launcher shortcut")?;
    // The user is asked to approve the binding, and can decline it. A refusal is
    // reported as a failed response rather than a transport error.
    let bound = request
        .response()
        .context("the desktop did not grant the launcher shortcut")?;

    match bound.shortcuts().iter().find(|s| s.id() == SHORTCUT_ID) {
        Some(shortcut) => {
            tracing::info!(
                "launcher bound to {} by the desktop portal",
                shortcut.trigger_description()
            );
            on_bound();
        }
        // Nothing to listen for: the portal accepted the request but bound no
        // shortcut, so say so instead of waiting on a signal that cannot come.
        None => bail!("the portal bound no shortcut for {SHORTCUT_ID}"),
    }

    while let Some(event) = activated.next().await {
        if event.shortcut_id() == SHORTCUT_ID {
            on_activated();
        }
    }

    // The stream ending means the portal hung up; the session goes with it.
    session.close().await.ok();
    bail!("the desktop portal stopped delivering shortcut activations")
}
