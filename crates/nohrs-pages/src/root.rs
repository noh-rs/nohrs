//! Root view of the Explorer window — one of the app's two top-level "pillars"
//! (the other being the launcher window, `nohrs-launcher`, added in P3).
//!
//! This lives in `nohrs-pages` rather than the binary because it is the
//! Explorer pillar's window root: it owns the page entities and routes between
//! them, depending only downward on `nohrs-ui` (chrome) and `nohrs-services`
//! (search). The binary just opens a window hosting this view; the future
//! launcher window will be a symmetric root in `nohrs-launcher`.

use crate::explorer::ExplorerPage;
use crate::{
    PageKind, extensions::ExtensionsPage, git::GitPage, s3::S3Page, settings::SettingsPage,
};
use gpui::{
    AnyElement, App, AsyncWindowContext, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, Render, WeakEntity, Window, div, prelude::*, px, rgb,
};
use gpui_component::resizable::ResizableState;
use gpui_component::{Icon, Root, Theme, ThemeMode as GpuiThemeMode};
use nohrs_core::config::{self, Config, ConfigOverride, ConfigWatcher};
use nohrs_core::telemetry::LogErr;
use nohrs_services::search::SearchService;
use nohrs_store::KvStore;
use nohrs_ui::components::layout::footer::{FooterProps, footer};
use nohrs_ui::components::layout::unified_toolbar::{
    AccountMenuAction, AccountMenuCommand, UnifiedToolbarProps, unified_toolbar,
};
use nohrs_ui::theme::theme;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tracing::info;

/// The application root view that hosts the page sidebar, the active page, and
/// the shared configuration and search state.
pub struct RootView {
    current_page: PageKind,
    focus_handle: FocusHandle,
    // Page entities
    explorer: Entity<ExplorerPage>,
    git: Entity<GitPage>,
    s3: Entity<S3Page>,
    extensions: Entity<ExtensionsPage>,
    settings: Entity<SettingsPage>,
    search_service: Option<Arc<SearchService>>,
    indexing_progress: Option<f32>,
    // Currently-applied configuration and the inputs needed to recompute it on
    // hot reload: the file path and the env/CLI override layers that sit above
    // the file (config.md §3).
    config: Config,
    config_path: PathBuf,
    config_overrides: Vec<ConfigOverride>,
    // Latest config load error, surfaced in the footer. Held here (not in the
    // explorer's transient status) so it is not cleared by an explorer
    // directory reload and survives across pages.
    config_status: Option<String>,
    // Kept alive for the window's lifetime so the OS watch is not dropped.
    _config_watcher: Option<ConfigWatcher>,
}

impl RootView {
    /// Build the Explorer window root: instantiate the page entities and start
    /// the indexing-progress poll. `resizable` is created at the application
    /// level and the search service is initialized by the binary (it owns the
    /// async runtime), keeping `nohrs-pages` free of runtime concerns.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resizable: Entity<ResizableState>,
        search_service: Option<Arc<SearchService>>,
        store: Option<Arc<dyn KvStore>>,
        config: Config,
        config_path: PathBuf,
        config_overrides: Vec<ConfigOverride>,
        config_error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let restore_tabs = config.explorer.restore_tabs;
        let explorer = cx.new(|cx| {
            ExplorerPage::new(
                resizable,
                search_service.clone(),
                store,
                restore_tabs,
                window,
                cx,
            )
        });
        let git = cx.new(|_cx| GitPage::new());
        let s3 = cx.new(|_cx| S3Page::new());
        let extensions = cx.new(|_cx| ExtensionsPage::new());
        let settings = cx.new(|_cx| SettingsPage::new());

        let mut view = RootView {
            current_page: PageKind::Explorer,
            focus_handle,
            explorer,
            git,
            s3,
            extensions,
            settings,
            search_service,
            indexing_progress: Some(1.0), // Start as hidden/done
            // Start from defaults so the initial `apply_config` below treats the
            // loaded config as a change and applies theme + ui uniformly.
            config: Config::default(),
            config_path,
            config_overrides,
            config_status: None,
            _config_watcher: None,
        };
        view.start_progress_loop(window, cx);
        view.apply_config(config, config_error, window, cx);
        view.start_config_watch(window, cx);
        view
    }

    /// Apply a freshly-merged configuration to the live UI: switch the theme
    /// mode, propagate `[ui]` settings to the explorer, and surface any load
    /// error in the status bar. Safe to call repeatedly (hot reload).
    pub fn apply_config(
        &mut self,
        config: Config,
        config_error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if config.theme.mode != self.config.theme.mode {
            let mode = match config.theme.mode {
                config::ThemeMode::Light => GpuiThemeMode::Light,
                config::ThemeMode::Dark => GpuiThemeMode::Dark,
                // `system` follows the OS appearance reported by the window.
                config::ThemeMode::System => GpuiThemeMode::from(window.appearance()),
            };
            Theme::change(mode, Some(window), cx);
        }

        // Condense the (possibly multi-line) diagnostic to a single line plus the
        // file path for the one-line status bar; full detail is in the logs.
        self.config_status = config_error.as_ref().map(|error| {
            let summary = error.lines().next().unwrap_or(error.as_str());
            format!("config: {summary} ({})", self.config_path.display())
        });

        let ui = config.ui.clone();
        let explorer_cfg = config.explorer.clone();
        self.explorer.update(cx, |page, cx| {
            page.apply_config_ui(&ui, cx);
            page.apply_config_explorer(&explorer_cfg, cx);
        });

        self.config = config;
        cx.notify();
    }

    /// Watch `config.toml` and re-apply on change. The `notify` callback runs on
    /// a background thread and only pings a channel; a foreground poll loop does
    /// the reload so all entity updates happen on the GPUI thread.
    fn start_config_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sender, receiver) = mpsc::channel::<()>();
        match ConfigWatcher::new(&self.config_path, move || {
            // A closed channel just means the app is shutting down.
            sender.send(()).log_err();
        }) {
            Ok(watcher) => self._config_watcher = Some(watcher),
            Err(error) => {
                tracing::warn!("config hot reload disabled: {error}");
                return;
            }
        }

        cx.spawn_in(
            window,
            move |this: WeakEntity<RootView>, cx: &mut AsyncWindowContext| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                        let mut changed = false;
                        let mut disconnected = false;
                        loop {
                            match receiver.try_recv() {
                                Ok(()) => changed = true,
                                Err(mpsc::TryRecvError::Empty) => break,
                                // The watcher was dropped; stop polling rather
                                // than spinning every 400ms forever.
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    disconnected = true;
                                    break;
                                }
                            }
                        }
                        if disconnected {
                            break;
                        }
                        if !changed {
                            continue;
                        }
                        let update = this.update_in(&mut cx, |this, window, cx| {
                            let (mut config, diagnostics) =
                                config::load_from_path(&this.config_path);
                            for over in &this.config_overrides {
                                config.apply_override(over);
                            }
                            let config_error = config::report_diagnostics(&diagnostics);
                            this.apply_config(config, config_error, window, cx);
                        });
                        if update.is_err() {
                            break; // The view (and window) is gone.
                        }
                    }
                }
            },
        )
        .detach();
    }

    /// Switches the active page, notifying for a redraw only if it changed.
    pub fn set_page(&mut self, page: PageKind, cx: &mut Context<Self>) {
        if self.current_page != page {
            self.current_page = page;
            cx.notify();
        }
    }

    fn start_progress_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Without a search service there is no indexing progress to poll, so avoid
        // rescheduling a per-frame no-op forever.
        if self.search_service.is_none() {
            return;
        }

        if let Some(progress) = self.check_progress_update() {
            self.indexing_progress = Some(progress);
            cx.notify();
        }

        // Poll every frame (simple and effective for this case)
        cx.on_next_frame(window, |view: &mut RootView, window, cx| {
            view.start_progress_loop(window, cx);
        });
    }

    fn check_progress_update(&self) -> Option<f32> {
        let service = self.search_service.as_ref()?;
        let rx = service.progress_subscription();
        // Just read current value
        let val = *rx.borrow();
        Some(val)
    }
}

impl Focusable for RootView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = unified_toolbar(
            UnifiedToolbarProps {
                account_name: "syuya2036".to_string(),
                account_plan: "Free".to_string(),
            },
            cx,
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .relative()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_account_action))
            .child(toolbar)
            .child(
                // Main content: toolbar + page
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    .child(
                        // Left navigation toolbar
                        self.render_navigation(cx),
                    )
                    .child(
                        // Main content area - render active page
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(0.0))
                            .child(self.render_active_page(window, cx)),
                    ),
            )
            .child(
                // Footer status bar
                {
                    // A config load error takes precedence over the explorer's
                    // transient status and is always shown as an error.
                    let (status_message, status_is_error) = match &self.config_status {
                        Some(message) => (Some(message.clone()), true),
                        None => match self.explorer.read(cx).status_for_footer(cx) {
                            Some((text, is_error)) => (Some(text), is_error),
                            None => (None, false),
                        },
                    };
                    let props = FooterProps {
                        indexing_progress: self.indexing_progress,
                        status_message,
                        status_is_error,
                        ..Default::default()
                    };
                    footer(props, cx)
                },
            )
            .children(Root::render_modal_layer(window, cx))
            .children(Root::render_drawer_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl RootView {
    fn handle_account_action(
        &mut self,
        action: &AccountMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            AccountMenuCommand::ProfileSummary => {
                window.prevent_default();
            }
            AccountMenuCommand::Settings => self.set_page(PageKind::Settings, cx),
            AccountMenuCommand::Extensions => self.set_page(PageKind::Extensions, cx),
            AccountMenuCommand::Keymap
            | AccountMenuCommand::Themes
            | AccountMenuCommand::IconThemes => {
                info!(?action.command, "Account menu item not yet implemented");
                window.prevent_default();
            }
            AccountMenuCommand::SignOut => {
                info!("Sign out requested");
            }
        }
    }

    fn render_navigation(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active_page = self.current_page;

        div()
            .w(px(64.0))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(rgb(theme::TOOLBAR_BG))
            .border_r_1()
            .border_color(rgb(theme::TOOLBAR_BORDER))
            .py(px(16.0))
            .child(
                // Page navigation buttons
                div().flex().flex_col().items_center().gap_2().children(
                    PageKind::all().into_iter().map(|page| {
                        let is_active = active_page == page;
                        self.navigation_button(page, is_active, cx)
                    }),
                ),
            )
    }

    fn navigation_button(
        &self,
        page: PageKind,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(("nav-btn", page as usize))
            .w(px(48.0))
            .h(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .when(active, |this| {
                this.bg(rgb(theme::TOOLBAR_ACTIVE_BG)).shadow_sm()
            })
            .when(!active, |this| {
                this.hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
            })
            .on_click(cx.listener(move |view, _event, _window, cx| {
                view.set_page(page, cx);
            }))
            .child(
                Icon::new(Icon::empty())
                    .path(page.icon_path())
                    .size_5()
                    .text_color(rgb(if active {
                        theme::TOOLBAR_ACTIVE_TEXT
                    } else {
                        theme::TOOLBAR_TEXT
                    })),
            )
    }

    fn render_active_page(&self, _window: &mut Window, _cx: &mut Context<Self>) -> AnyElement {
        match self.current_page {
            PageKind::Explorer => self.explorer.clone().into_any_element(),

            PageKind::Git => self.git.clone().into_any_element(),
            PageKind::S3 => self.s3.clone().into_any_element(),
            PageKind::Extensions => self.extensions.clone().into_any_element(),
            PageKind::Settings => self.settings.clone().into_any_element(),
        }
    }
}
