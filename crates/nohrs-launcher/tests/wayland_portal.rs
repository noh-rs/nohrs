//! Exercises the Wayland global-shortcut path against a mock desktop portal.
//!
//! On Wayland the portal is the only thing between the launcher and the user's
//! keyboard, and there is no compositor in CI to try it against — so the D-Bus
//! conversation is driven against a portal this test owns: a private bus, an
//! object serving `org.freedesktop.portal.GlobalShortcuts`, and an `Activated`
//! signal that has to come back out of `portal::listen` as a summon.
//!
//! What this proves is that nohrs speaks the protocol correctly. Whether a given
//! desktop implements the portal at all is its business, and is what the
//! fallback and the warning in `hotkey.rs` are for.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]
// The mock stands in for a system service, so it does what one does: unwrap on
// invariants the bus itself guarantees.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// `set_var` is unsafe from edition 2024 because another thread could be reading
// the environment. Here it runs before the bus connection — and so before any
// thread that would read it — exists, and ashpd latches the address on first
// use, which leaves no other way to point it at a test bus.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use async_io::Timer;
use futures_lite::future;
use nohrs_launcher::hotkey::portal;
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, interface};

/// The trigger the launcher asks for; the mock echoes it back as granted.
const TRIGGER: &str = "CTRL+SHIFT+space";

/// The id the launcher files its binding under, and the one an `Activated`
/// signal has to carry to count as a summon.
use portal::SHORTCUT_ID;

/// A private session bus, killed when the test ends.
struct PrivateBus {
    process: Child,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.process.kill().ok();
        self.process.wait().ok();
    }
}

impl PrivateBus {
    /// Starts a bus and points this process at it, or returns `None` when
    /// `dbus-daemon` is not installed.
    fn start() -> Option<Self> {
        let mut process = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = process.stdout.take()?;
        let mut address = String::new();
        BufReader::new(stdout).read_line(&mut address).ok()?;
        let address = address.trim().to_string();
        if address.is_empty() {
            return None;
        }

        // ashpd caches the session connection in a `OnceLock` on first use, so
        // this has to be set before anything in the launcher touches D-Bus. The
        // test binary is a fresh process, so it is.
        //
        // SAFETY-adjacent: single-threaded at this point, before any task is
        // spawned that could read the environment concurrently.
        unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &address) };
        Some(Self { process })
    }
}

/// Reproduces the object path ashpd derives for a request or session.
///
/// The portal does not get to choose these: the client computes the path from
/// its own bus name and a token it generated, subscribes there, and *then*
/// calls. A mock that returns anything else is never heard from again — which
/// is precisely the mistake this test is here to catch.
fn portal_path(kind: &str, sender: &str, token: &str) -> OwnedObjectPath {
    let unique = sender.trim_start_matches(':').replace('.', "_");
    ObjectPath::try_from(format!(
        "/org/freedesktop/portal/desktop/{kind}/{unique}/{token}"
    ))
    .unwrap()
    .into()
}

fn string_option(options: &HashMap<String, OwnedValue>, key: &str) -> String {
    options
        .get(key)
        .and_then(|value| value.downcast_ref::<&str>().ok())
        .unwrap_or_default()
        .to_string()
}

/// Emits the `Response` a portal request is completed by.
async fn emit_response(
    connection: &Connection,
    path: &OwnedObjectPath,
    results: HashMap<&str, Value<'_>>,
) -> zbus::Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            path,
            "org.freedesktop.portal.Request",
            "Response",
            // 0 is the spec's "success"; anything else is a refusal.
            &(0u32, results),
        )
        .await
}

struct MockPortal {
    /// Announces the session path once a shortcut has been bound, so the test
    /// knows when it is meaningful to fire the shortcut.
    bound: async_channel::Sender<OwnedObjectPath>,
}

#[interface(name = "org.freedesktop.portal.GlobalShortcuts")]
impl MockPortal {
    /// ashpd reads this before anything else and treats its absence as "no such
    /// portal", so the mock has to carry it.
    #[zbus(property)]
    fn version(&self) -> u32 {
        1
    }

    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = header.sender().unwrap().to_string();
        let request = portal_path("request", &sender, &string_option(&options, "handle_token"));
        let session = portal_path(
            "session",
            &sender,
            &string_option(&options, "session_handle_token"),
        );

        // Real portals send this as a string rather than an object path; ashpd
        // accepts either, and the string is what it will meet in the wild.
        let results = HashMap::from([("session_handle", Value::from(session.as_str()))]);
        emit_response(connection, &request, results).await?;
        Ok(request)
    }

    async fn bind_shortcuts(
        &self,
        session_handle: OwnedObjectPath,
        shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = header.sender().unwrap().to_string();
        let request = portal_path("request", &sender, &string_option(&options, "handle_token"));

        // Grant exactly what was asked for, echoing the requested trigger back
        // as the one the desktop settled on.
        let granted: Vec<(String, HashMap<&str, Value>)> = shortcuts
            .iter()
            .map(|(id, info)| {
                let description = string_option(info, "description");
                let trigger = string_option(info, "preferred_trigger");
                (
                    id.clone(),
                    HashMap::from([
                        ("description", Value::from(description)),
                        ("trigger_description", Value::from(trigger)),
                    ]),
                )
            })
            .collect();

        let results = HashMap::from([("shortcuts", Value::from(granted))]);
        emit_response(connection, &request, results).await?;
        self.bound.try_send(session_handle).ok();
        Ok(request)
    }
}

/// Fires the shortcut, as a compositor would.
async fn emit_activated(connection: &Connection, session: &OwnedObjectPath) -> zbus::Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.GlobalShortcuts",
            "Activated",
            &(
                session,
                SHORTCUT_ID,
                0u64,
                HashMap::<String, OwnedValue>::new(),
            ),
        )
        .await
}

#[test]
fn a_portal_activation_becomes_a_summon() {
    let Some(_bus) = PrivateBus::start() else {
        // Without dbus-daemon there is no bus to mock a portal on. Skipping is
        // honest; failing would only report the sandbox, not the code.
        eprintln!("skipping: dbus-daemon is not installed");
        return;
    };

    let result: anyhow::Result<()> = future::block_on(async {
        let (bound_sender, bound_receiver) = async_channel::bounded(1);
        let connection = zbus::connection::Builder::session()?
            .name("org.freedesktop.portal.Desktop")?
            .serve_at(
                "/org/freedesktop/portal/desktop",
                MockPortal {
                    bound: bound_sender,
                },
            )?
            .build()
            .await?;

        let (summoned_sender, summoned_receiver) = async_channel::bounded::<()>(1);
        let listen = portal::listen(TRIGGER, move || {
            summoned_sender.try_send(()).ok();
        });

        let drive = async {
            let session = bound_receiver.recv().await?;

            // The launcher only subscribes to `Activated` once the binding is
            // confirmed, so a single signal could land in the gap before that.
            // Repeating until the summon arrives removes the race without
            // pretending a fixed sleep is long enough.
            for _ in 0..50 {
                emit_activated(&connection, &session).await?;
                let summoned =
                    future::or(async { summoned_receiver.recv().await.is_ok() }, async {
                        Timer::after(Duration::from_millis(100)).await;
                        false
                    })
                    .await;
                if summoned {
                    return Ok(());
                }
            }
            anyhow::bail!("the portal activation never reached the launcher")
        };

        // `listen` only returns on failure, so whichever finishes first is the
        // verdict: the driver succeeding, or the listener explaining why not.
        future::or(drive, listen).await
    });

    result.expect("a bound portal shortcut should summon the launcher");
}
