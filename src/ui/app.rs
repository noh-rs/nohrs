#![cfg(feature = "gui")]

use crate::core::telemetry::logging::init_logging;
use crate::pages::{
    explorer::ExplorerPage, extensions::ExtensionsPage, git::GitPage, s3::S3Page,
    search::SearchPage, settings::SettingsPage, PageKind,
};
use crate::ui::assets::Assets;
use crate::ui::components::layout::footer::{footer, FooterProps};
use crate::ui::components::layout::unified_toolbar::{
    unified_toolbar, AccountMenuAction, AccountMenuCommand, UnifiedToolbarProps,
    UNIFIED_TOOLBAR_HEIGHT,
};
use crate::ui::theme::theme;
use crate::ui::window::{self, traffic_lights::TrafficLightsHook};

use gpui::Entity;
use gpui::{
    div, prelude::*, px, rgb, size, AnyElement, App, Application, Bounds, Context, FocusHandle,
    Focusable, IntoElement, Render, Window,
};
use gpui_component::input::InputState;
use gpui_component::resizable::ResizableState;
use gpui_component::Icon;
use gpui_component::Root;
use tracing::info;

pub struct NohrApp;

impl NohrApp {
    pub fn run() {
        init_logging();

        Application::new().with_assets(Assets).run(|app: &mut App| {
            gpui_component::init(app);
            let resizable = ResizableState::new(app);
            let bounds = Bounds::centered(None, size(px(1280.0), px(780.0)), app);
            let traffic_lights = TrafficLightsHook::new().center_vertically(UNIFIED_TOOLBAR_HEIGHT);
            let window_options = window::unified_window_options(bounds, &traffic_lights);

            app.open_window(window_options, |window, cx| {
                let search_input = cx.new(|cx| InputState::new(window, cx));
                let focus_handle = cx.focus_handle();

                // Create page instances
                let explorer = cx.new(|cx| {
                    ExplorerPage::new(resizable.clone(), search_input.clone(), cx.focus_handle())
                });
                let search = cx.new(|cx| SearchPage::new(resizable.clone(), window, cx));
                let git = cx.new(|_cx| GitPage::new());
                let s3 = cx.new(|_cx| S3Page::new());
                let extensions = cx.new(|_cx| ExtensionsPage::new());
                let settings = cx.new(|_cx| SettingsPage::new());

                let view = cx.new(|_cx| RootView {
                    current_page: PageKind::Explorer,
                    focus_handle,
                    explorer,
                    search,
                    git,
                    s3,
                    extensions,
                    settings,
                });

                cx.new(|cx| Root::new(view.into(), window, cx))
            })
            .expect("open window");
        });
    }
}

pub struct RootView {
    current_page: PageKind,
    focus_handle: FocusHandle,
    // Page entities
    explorer: Entity<ExplorerPage>,
    search: Entity<SearchPage>,
    git: Entity<GitPage>,
    s3: Entity<S3Page>,
    extensions: Entity<ExtensionsPage>,
    settings: Entity<SettingsPage>,
}

impl RootView {
    pub fn set_page(&mut self, page: PageKind, cx: &mut Context<Self>) {
        if self.current_page != page {
            self.current_page = page;
            cx.notify();
        }
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
                footer(FooterProps::default(), cx),
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

    fn render_navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
    ) -> impl IntoElement {
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
            PageKind::Search => self.search.clone().into_any_element(),
            PageKind::Git => self.git.clone().into_any_element(),
            PageKind::S3 => self.s3.clone().into_any_element(),
            PageKind::Extensions => self.extensions.clone().into_any_element(),
            PageKind::Settings => self.settings.clone().into_any_element(),
        }
    }
}
