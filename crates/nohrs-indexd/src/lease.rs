//! How the daemon knows when to stop.
//!
//! The daemon lives exactly as long as someone is holding onto it, and the
//! count of those someones is kept by the kernel rather than by this code:
//! each client is a socket connection, and a connection is closed when its
//! process ends **however it ends** — cleanly, panicking, or killed outright.
//! The server reads that as end-of-stream.
//!
//! `Drop` cannot be the mechanism. It does not run on `SIGKILL`, and it does
//! not reach across a process boundary to decrement anything here. What Rust
//! contributes is the shape: a connection is served while holding a [`Lease`],
//! the lease is a parameter of the serving function rather than something the
//! function has to remember to release, and the end of stream ends the function
//! and with it the lease. `Arc` in the model, the kernel in the implementation.
//!
//! One thing an `Arc` cannot express is why this is not simply an `Arc`: the
//! daemon waits a grace period after the last client leaves, so that a `noh
//! index build` followed a moment later by another does not pay to start a
//! second daemon. `Arc` reports that the count reached zero and then forgets
//! everything, including how to cancel what that set in motion. So the count
//! and the moment it reached zero are kept explicitly, and a client arriving
//! during the grace period simply raises the count again.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The default grace period: how long the daemon stays up with no clients.
///
/// Long enough that a handful of one-shot commands in a row reuse one daemon,
/// short enough that nothing lingers unexplained after the app quits.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(90);

#[derive(Debug, Default)]
struct State {
    live: usize,
    /// When the count last reached zero, and `None` while anyone is holding a
    /// lease. This is what lets a client cancel a shutdown that is pending.
    idle_since: Option<Instant>,
    /// Set by [`Leases::release`] to end the wait whatever the count says.
    released: bool,
}

/// The daemon's clients, counted.
#[derive(Debug)]
pub struct Leases {
    state: Mutex<State>,
    changed: Condvar,
    grace: Duration,
}

impl Leases {
    /// Counts clients, with `grace` between the last one leaving and
    /// [`Leases::wait_until_idle`] returning.
    pub fn new(grace: Duration) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                live: 0,
                // Nobody has connected yet, and the grace period is already
                // running: a daemon nothing ever talks to must not stay up.
                idle_since: Some(Instant::now()),
                released: false,
            }),
            changed: Condvar::new(),
            grace,
        })
    }

    /// Takes a lease. The daemon stays up while it is alive.
    pub fn take(self: &Arc<Self>) -> Lease {
        let mut state = self.lock();
        state.live += 1;
        state.idle_since = None;
        drop(state);
        self.changed.notify_all();
        Lease {
            leases: Arc::clone(self),
        }
    }

    /// How many clients are holding a lease.
    pub fn live(&self) -> usize {
        self.lock().live
    }

    /// Ends [`Leases::wait_until_idle`] now, whoever is still connected.
    ///
    /// What `Request::Stop` and a signal both come down to.
    pub fn release(&self) {
        let mut state = self.lock();
        state.released = true;
        drop(state);
        self.changed.notify_all();
    }

    /// Blocks until no lease has existed for the whole grace period, or until
    /// [`Leases::release`] is called.
    ///
    /// Waits on a condition variable rather than polling, so a daemon with
    /// nothing to do costs nothing to keep around.
    pub fn wait_until_idle(&self) {
        let mut state = self.lock();
        loop {
            if state.released {
                return;
            }
            match state.idle_since {
                // Somebody is connected. Nothing to do until that changes.
                None => state = self.wait(state),
                Some(since) => match self.grace.checked_sub(since.elapsed()) {
                    None => return,
                    Some(remaining) => state = self.wait_for(state, remaining),
                },
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // The lock only ever guards a count and an instant, so a panic
        // elsewhere cannot have left them half-written; refusing to shut the
        // daemon down over it would be worse than reading them.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn wait_for<'a>(
        &self,
        state: MutexGuard<'a, State>,
        how_long: Duration,
    ) -> MutexGuard<'a, State> {
        self.changed
            .wait_timeout(state, how_long)
            .unwrap_or_else(PoisonError::into_inner)
            .0
    }
}

/// One client's claim on the daemon.
///
/// Held for the length of a connection. Taking it by value in the function that
/// serves the connection is the point: there is no way to serve a client
/// without holding one, and no way to hold one past the end of the serving.
#[derive(Debug)]
pub struct Lease {
    leases: Arc<Leases>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.leases.lock();
        state.live = state.live.saturating_sub(1);
        if state.live == 0 {
            state.idle_since = Some(Instant::now());
        }
        drop(state);
        self.leases.changed.notify_all();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Short enough to keep the tests quick, long enough that a scheduler
    /// hiccup does not read as an expiry.
    const GRACE: Duration = Duration::from_millis(200);

    #[test]
    fn a_daemon_nobody_talks_to_does_not_stay_up() {
        let leases = Leases::new(GRACE);

        let started = Instant::now();
        leases.wait_until_idle();

        assert!(
            started.elapsed() >= GRACE,
            "gave up before the grace period was out"
        );
    }

    #[test]
    fn the_wait_lasts_as_long_as_a_client_does() {
        let leases = Leases::new(GRACE);
        let lease = leases.take();
        assert_eq!(leases.live(), 1);

        let leaver = std::thread::spawn({
            let leases = Arc::clone(&leases);
            move || {
                std::thread::sleep(GRACE * 2);
                drop(lease);
                assert_eq!(leases.live(), 0);
            }
        });

        let started = Instant::now();
        leases.wait_until_idle();
        // Joined, because a panic stays in the thread it happened on: without
        // this the assertion above could not fail the test.
        leaver.join().unwrap();

        assert!(
            started.elapsed() >= GRACE * 3,
            "the grace period started before the client left"
        );
    }

    #[test]
    fn a_client_arriving_during_the_grace_period_cancels_the_shutdown() {
        let leases = Leases::new(GRACE);
        drop(leases.take());

        // Arrives while the daemon is counting down, which has to call it off.
        let keeper = std::thread::spawn({
            let leases = Arc::clone(&leases);
            move || {
                std::thread::sleep(GRACE / 2);
                let lease = leases.take();
                std::thread::sleep(GRACE * 2);
                drop(lease);
            }
        });

        let started = Instant::now();
        leases.wait_until_idle();
        keeper.join().unwrap();

        assert!(
            started.elapsed() >= GRACE * 3,
            "shut down while a client was connected"
        );
    }

    #[test]
    fn leases_are_counted_rather_than_flagged() {
        let leases = Leases::new(GRACE);
        let first = leases.take();
        let second = leases.take();
        assert_eq!(leases.live(), 2);

        drop(first);
        assert_eq!(leases.live(), 1);
        // One client leaving is not the last one leaving: no countdown yet.
        assert!(leases.lock().idle_since.is_none());

        drop(second);
        assert!(leases.lock().idle_since.is_some());
    }

    #[test]
    fn being_told_to_stop_does_not_wait_for_anyone() {
        let leases = Leases::new(Duration::from_secs(3600));
        let _lease = leases.take();

        std::thread::spawn({
            let leases = Arc::clone(&leases);
            move || leases.release()
        });

        let started = Instant::now();
        leases.wait_until_idle();

        assert!(
            started.elapsed() < Duration::from_secs(30),
            "a stop waited for the grace period"
        );
    }
}
