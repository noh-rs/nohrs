//! Application entry / startup sequence.
//!
//! The binary is the only layer allowed to depend on every crate, so it wires
//! the pillars together: it builds the shared window chrome from `nohrs-ui`,
//! initializes services that need the async runtime, and opens the Explorer
//! window hosting `nohrs_pages::RootView` and the launcher window hosting
//! `nohrs_launcher::LauncherView`. Neither pillar references the other; the
//! binary owns the keybinding that summons the launcher from anywhere.

use crate::cli::{Cli, Command};
use gpui::{App, AppContext, Application, Bounds, KeyBinding, px, size};
use gpui_component::Root;
use gpui_component::resizable::ResizableState;
use nohrs_core::config::{self, ConfigOverride};
use nohrs_core::telemetry::logging::{FileLogConfig, init_logging_with_file};
use nohrs_launcher::{LauncherIndex, ToggleLauncher};
use nohrs_pages::RootView;
use nohrs_services::search::SearchService;
use nohrs_store::{KvStore, RedbKvStore, StoreLogConfig};
use nohrs_ui::assets::Assets;
use nohrs_ui::components::layout::unified_toolbar::UNIFIED_TOOLBAR_HEIGHT;
use nohrs_ui::window::{self, traffic_lights::TrafficLightsHook};
use std::sync::Arc;

pub struct NohrsApp;

impl NohrsApp {
    pub fn run(cli: &Cli) {
        // Held for the whole run: dropping it stops the log file being written.
        // Installed before the config is read so a failure to load the config is
        // itself recorded, which means the file sink uses its defaults rather
        // than anything the user set — see `docs/logging.md` §4.
        let _log_guard = init_logging_with_file(&FileLogConfig::default());

        // Load configuration before opening the window: defaults < file < env <
        // CLI (config.md §3). A missing file is created with defaults so users
        // have something to edit and the watcher has a target. Parse failures are
        // non-fatal — we fall back to defaults and surface the error in the UI.
        let config_path = config::paths::config_file();
        if let Err(error) = config::ensure_exists(&config_path) {
            tracing::warn!("could not create {}: {error}", config_path.display());
        }
        let (mut config, diagnostics) = config::load_from_path(&config_path);
        let config_overrides = vec![ConfigOverride::from_env(), cli.overrides()];
        for over in &config_overrides {
            config.apply_override(over);
        }
        let config_error = config::report_diagnostics(&diagnostics);

        // GPUI drives the app (replacing `#[tokio::main]`; ADR 0004, async-runtime.md
        // §7). The search service is now tokio-free — its file watcher and progress
        // channels run on std threads and runtime-agnostic channels — so no async
        // runtime needs to be entered here.
        let launcher_only = matches!(cli.command, Some(Command::Launcher));

        Application::new().with_assets(Assets).run(move |app: &mut App| {
            gpui_component::init(app);

            // The name index is built once for the whole run and shared by every
            // launcher window, so summoning the launcher never waits on a scan.
            let launcher = LauncherIndex::start(app);
            // Two ways in (docs/launcher.md §2): a chord the OS routes to nohrs
            // from any application, and one GPUI handles while nohrs is in front.
            launcher.install_global_hotkey(app);
            install_in_app_launcher_key(launcher.clone(), app);

            if launcher_only {
                // Resident mode: nohrs is a background process whose only job is
                // to answer the global hotkey, so it must outlive each dismissal
                // of the launcher window.
                if let Err(error) = nohrs_launcher::open_keep_alive_window(app) {
                    tracing::error!("failed to open the keep-alive window: {error}");
                }
                if let Err(error) = launcher.toggle(app) {
                    tracing::error!("failed to open launcher window: {error}");
                }
                return;
            }

            let resizable = app.new(|_| ResizableState::default());
            let bounds = Bounds::centered(
                None,
                size(px(config::WINDOW_WIDTH), px(config::WINDOW_HEIGHT)),
                app,
            );
            let traffic_lights = TrafficLightsHook::new().center_vertically(UNIFIED_TOOLBAR_HEIGHT);
            let window_options = window::unified_window_options(bounds, &traffic_lights);

            // Open the host KV store (`state.redb`) for tab/session restore
            // (docs/persistence.md §3). Failure is non-fatal: the app starts
            // without session persistence rather than crashing.
            let store: Option<Arc<dyn KvStore>> = open_host_store();

            let opened = app.open_window(window_options, {
                let config = config.clone();
                let config_path = config_path.clone();
                let config_overrides = config_overrides.clone();
                let config_error = config_error.clone();
                let store = store.clone();
                move |window, cx| {
                    // Initialize SearchService. Failure is non-fatal: the app starts
                    // with full-text search disabled rather than crashing.
                    let search_service: Option<Arc<SearchService>> = match SearchService::new() {
                        Ok(service) => Some(Arc::new(service)),
                        Err(e) => {
                            tracing::error!(
                                "Failed to initialize search service; starting with search disabled: {}",
                                e
                            );
                            None
                        }
                    };

                    // Kick off initial indexing on GPUI's background executor, which
                    // is a thread pool (replacing tokio::task::spawn_blocking;
                    // async-runtime.md §2).
                    if let Some(service) = &search_service {
                        if let Some(job) = service.take_initial_indexing_job() {
                            cx.background_spawn(async move { job.run() }).detach();
                        }
                    }

                    let view = cx.new(|cx| {
                        RootView::new(
                            resizable.clone(),
                            search_service,
                            store,
                            config,
                            config_path,
                            config_overrides,
                            config_error,
                            window,
                            cx,
                        )
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                }
            });
            if let Err(error) = opened {
                tracing::error!("failed to open main window: {error}");
            }
        });
    }
}

/// Binds the launcher's in-app summon key and handles it at application level.
///
/// This is the half of docs/launcher.md §2 that GPUI can serve: it only fires
/// while a nohrs window has focus. Summoning the launcher over *other*
/// applications is the OS-level registration in `nohrs_launcher::hotkey`.
///
/// The handler is global rather than attached to a window so the same key works
/// from the explorer, from a launcher window (where it toggles the launcher
/// closed), and from any window added later — without either pillar having to
/// know the other exists.
///
/// `cmd-k` is the platform key on macOS and `ctrl-k` its equivalent elsewhere;
/// both are bound so the binding is right on either platform.
fn install_in_app_launcher_key(launcher: LauncherIndex, app: &mut App) {
    app.bind_keys([
        KeyBinding::new("cmd-k", ToggleLauncher, None),
        KeyBinding::new("ctrl-k", ToggleLauncher, None),
    ]);
    app.on_action(move |_: &ToggleLauncher, cx: &mut App| {
        if let Err(error) = launcher.toggle(cx) {
            tracing::error!("failed to toggle launcher window: {error}");
        }
    });
}

/// Opens the host KV store at `<data_dir>/state.redb`, creating the data
/// directory if needed. Returns `None` (and logs) on any failure so a broken or
/// unwritable store degrades to "no session persistence" rather than a crash.
fn open_host_store() -> Option<Arc<dyn KvStore>> {
    let data_dir = config::paths::data_dir();
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        tracing::error!("could not create data dir {}: {error}", data_dir.display());
        return None;
    }
    let path = data_dir.join("state.redb");
    match RedbKvStore::open(&path, &StoreLogConfig::default()) {
        Ok(store) => Some(Arc::new(store)),
        Err(error) => {
            tracing::error!("could not open KV store {}: {error}", path.display());
            None
        }
    }
}
