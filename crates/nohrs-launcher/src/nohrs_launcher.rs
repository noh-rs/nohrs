//! The Launcher pillar: a Raycast-style floating search window.
//!
//! One of the app's two top-level pillars (the other being the Explorer, rooted
//! at `nohrs_pages::RootView`). It is a separate window with its own view tree
//! and depends only downward — on `nohrs-ui`, `nohrs-services` and `nohrs-core`
//! — so the Explorer and the launcher never reference each other.
//!
//! This is the search half of docs/launcher.md: a query field, a fuzzy-ranked
//! list of files and folders, `Enter` to open one, and the global hotkey (§2)
//! that summons it over any application. The command framework (§4), detail
//! pane (§7) and push-pop navigation (§8) build on the same pipeline and are not
//! implemented yet.
//!
//! The pipeline follows docs/architecture.md §3.2:
//!
//! ```text
//! keystroke → LauncherView::on_query_changed  (debounced 50ms)
//!           → FileNameIndex::snapshot          (services)
//!           → ranking::rank                    (nucleo + boosts, background)
//!           → Vec<LauncherItem> → cx.notify    (foreground)
//! ```

/// The launcher's own single-line search field.
pub mod field;
/// The OS-global summon key.
pub mod hotkey;
/// Fuzzy matching and ranking of index entries into result rows.
pub mod ranking;
/// The launcher window's root view.
pub mod view;
/// Window placement, options, and open/toggle.
pub mod window;

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, AppContext, actions};
use nohrs_services::search::file_index::{FileIndexConfig, FileNameIndex};

pub use crate::ranking::{ItemKind, LauncherItem, rank};
pub use crate::view::LauncherView;
pub use crate::window::{
    LAUNCHER_HEIGHT, LAUNCHER_WIDTH, launcher_bounds, launcher_window_options,
    open_keep_alive_window, open_launcher, toggle_launcher,
};

actions!(
    launcher,
    [
        /// Open the launcher, or close it if it is already open.
        ToggleLauncher
    ]
);

/// The name index the launcher searches, plus the root it was built from.
///
/// Held by the application for its whole run: the scan is what makes a query
/// answerable within a keystroke, so it must outlive any one launcher window.
#[derive(Clone)]
pub struct LauncherIndex {
    /// The shared index. Searchable immediately; empty until the scan lands.
    pub index: Arc<FileNameIndex>,
    /// Directory the index covers, used for ranking and for `~` in subtitles.
    pub home: Option<PathBuf>,
}

impl LauncherIndex {
    /// Starts a home-directory scan on the background executor and returns the
    /// handle to it. Returns immediately: the launcher opens against an empty
    /// index and fills in when the scan completes, rather than blocking startup.
    ///
    /// A missing home directory is not fatal — the launcher opens and reports
    /// that it has nothing indexed.
    pub fn start(cx: &mut App) -> Self {
        // The search field's editing keys are bound once for the process, not
        // per window, so a launcher summoned later is already typable.
        field::init(cx);

        let index = Arc::new(FileNameIndex::new());
        let config = match FileIndexConfig::for_home() {
            Ok(config) => config,
            Err(error) => {
                tracing::error!("launcher search disabled, no home directory: {error}");
                return Self { index, home: None };
            }
        };

        let home = Some(config.root.clone());
        cx.background_spawn({
            let index = index.clone();
            async move { index.rebuild(&config) }
        })
        .detach();

        Self { index, home }
    }

    /// Opens the launcher window, or closes it if one is already open.
    pub fn toggle(&self, cx: &mut App) -> anyhow::Result<()> {
        toggle_launcher(self.index.clone(), self.home.clone(), cx)
    }

    /// Registers the OS-global summon key, so the launcher answers from any
    /// application rather than only when nohrs is in front.
    ///
    /// Never fatal: a chord the OS will not give up — already taken, no display,
    /// or a Wayland session, where a global grab needs a portal `global-hotkey`
    /// does not speak — leaves nohrs running with the in-app binding alone. The
    /// outcome is logged either way, because "my hotkey does nothing" is
    /// otherwise a silent mystery.
    pub fn install_global_hotkey(&self, cx: &mut App) {
        let launcher = self.clone();
        match hotkey::install(cx, move |cx| {
            if let Err(error) = launcher.toggle(cx) {
                tracing::error!("failed to toggle the launcher from the global hotkey: {error}");
            }
        }) {
            Ok(hotkey::Backend::Grab) => tracing::info!(
                "launcher listening on {}",
                hotkey::describe(&hotkey::default_chord())
            ),
            // The portal confirms the binding asynchronously — and may ask the
            // user first — so its own task reports the chord that was granted.
            Ok(hotkey::Backend::Portal) => {
                tracing::info!("requesting a global shortcut from the desktop portal")
            }
            Err(error) => tracing::warn!(
                "global hotkey unavailable, launcher can only be summoned from nohrs: {error:#}"
            ),
        }
    }
}
