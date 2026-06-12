use crate::theme::theme;
use gpui::{
    Action, Context, IntoElement, Pixels, Render, WindowControlArea, div, prelude::*, px, rgb,
};
use gpui_component::{
    Icon, IconName, Sizable, Size,
    button::{Button, ButtonRounded, ButtonVariant, ButtonVariants},
    menu::DropdownMenu,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Fixed height of the unified toolbar.
pub const UNIFIED_TOOLBAR_HEIGHT: Pixels = px(36.0);
const ACCOUNT_BUTTON_ID: &str = "unified-toolbar-account-button";

/// Properties for the unified toolbar's account button and menu.
#[derive(Clone)]
pub struct UnifiedToolbarProps {
    /// Display name shown in the account menu header.
    pub account_name: String,
    /// Account plan label shown beneath the name; hidden when empty.
    pub account_plan: String,
}

impl Default for UnifiedToolbarProps {
    fn default() -> Self {
        Self {
            account_name: "Guest".to_string(),
            account_plan: String::new(),
        }
    }
}

/// Build the unified toolbar element: a draggable window region plus an account button with menu.
pub fn unified_toolbar<V: Render>(
    props: UnifiedToolbarProps,
    _cx: &mut Context<V>,
) -> impl IntoElement + use<V> {
    let UnifiedToolbarProps {
        account_name,
        account_plan,
    } = props;

    let drag_region = div()
        .id("unified-toolbar-drag-region")
        .flex_1()
        .h_full()
        .window_control_area(WindowControlArea::Drag);

    let account_button = Button::new(ACCOUNT_BUTTON_ID)
        .icon(
            Icon::new(IconName::CircleUser)
                .size_5()
                .text_color(rgb(theme::FG_SECONDARY)),
        )
        .rounded(ButtonRounded::Large)
        .compact()
        .with_variant(ButtonVariant::Ghost)
        .with_size(Size::Small)
        .dropdown_menu(move |menu, _window, _cx| {
            let header_name = account_name.clone();
            let header_plan = account_plan.clone();

            let mut menu = menu
                .min_w(px(220.0))
                .menu_element_with_disabled(
                    AccountMenuAction::boxed(AccountMenuCommand::ProfileSummary),
                    true,
                    move |_, _| {
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Icon::new(IconName::CircleUser)
                                    .size_4()
                                    .text_color(rgb(theme::FG_SECONDARY)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .items_start()
                                    .gap_y(px(2.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(theme::FG))
                                            .child(header_name.clone()),
                                    )
                                    .when(!header_plan.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(theme::FG_SECONDARY))
                                                .child(header_plan.clone()),
                                        )
                                    }),
                            )
                    },
                )
                .separator();

            menu = menu
                .menu_with_icon(
                    "Settings",
                    Icon::new(IconName::Settings),
                    AccountMenuAction::boxed(AccountMenuCommand::Settings),
                )
                .menu_with_icon(
                    "Keymap",
                    Icon::new(IconName::SquareTerminal),
                    AccountMenuAction::boxed(AccountMenuCommand::Keymap),
                )
                .menu_with_icon(
                    "Themes…",
                    Icon::new(IconName::Palette),
                    AccountMenuAction::boxed(AccountMenuCommand::Themes),
                )
                .menu_with_icon(
                    "Icon Themes…",
                    Icon::new(IconName::GalleryVerticalEnd),
                    AccountMenuAction::boxed(AccountMenuCommand::IconThemes),
                )
                .menu_with_icon(
                    "Extensions",
                    Icon::new(IconName::LayoutDashboard),
                    AccountMenuAction::boxed(AccountMenuCommand::Extensions),
                )
                .separator()
                .menu_with_icon(
                    "Sign Out",
                    Icon::new(IconName::ChevronRight),
                    AccountMenuAction::boxed(AccountMenuCommand::SignOut),
                );

            menu
        });

    div()
        .id("unified-toolbar")
        .h(UNIFIED_TOOLBAR_HEIGHT)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(16.0))
        .bg(rgb(theme::BG))
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .child(drag_region)
        .child(account_button)
}

/// Commands dispatched from the account menu in the unified toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccountMenuCommand {
    /// The disabled header entry showing the account summary.
    ProfileSummary,
    /// Open the settings page.
    Settings,
    /// Open the keymap editor.
    Keymap,
    /// Open the theme picker.
    Themes,
    /// Open the icon theme picker.
    IconThemes,
    /// Open the extensions view.
    Extensions,
    /// Sign out of the current account.
    SignOut,
}

impl AccountMenuCommand {
    fn from_value(value: &Value) -> Option<Self> {
        if let Some(s) = value.as_str() {
            return Self::from_str(s);
        }
        value
            .get("command")
            .and_then(Value::as_str)
            .and_then(Self::from_str)
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "profile-summary" => Some(Self::ProfileSummary),
            "settings" => Some(Self::Settings),
            "keymap" => Some(Self::Keymap),
            "themes" => Some(Self::Themes),
            "icon-themes" => Some(Self::IconThemes),
            "extensions" => Some(Self::Extensions),
            "sign-out" => Some(Self::SignOut),
            _ => None,
        }
    }
}

/// GPUI action wrapping an [`AccountMenuCommand`] dispatched from the account menu.
#[derive(Clone)]
pub struct AccountMenuAction {
    /// The command this action carries.
    pub command: AccountMenuCommand,
}

impl AccountMenuAction {
    /// Create an action for the given command.
    pub fn new(command: AccountMenuCommand) -> Self {
        Self { command }
    }

    /// Create a boxed action for the given command, ready to dispatch.
    pub fn boxed(command: AccountMenuCommand) -> Box<dyn Action> {
        Box::new(Self::new(command))
    }
}

impl fmt::Debug for AccountMenuAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountMenuAction")
            .field("command", &self.command)
            .finish()
    }
}

impl Action for AccountMenuAction {
    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }

    fn partial_eq(&self, other: &dyn Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|action| action.command == self.command)
    }

    fn name(&self) -> &'static str {
        match self.command {
            AccountMenuCommand::ProfileSummary => "account-menu.profile-summary",
            AccountMenuCommand::Settings => "account-menu.settings",
            AccountMenuCommand::Keymap => "account-menu.keymap",
            AccountMenuCommand::Themes => "account-menu.themes",
            AccountMenuCommand::IconThemes => "account-menu.icon-themes",
            AccountMenuCommand::Extensions => "account-menu.extensions",
            AccountMenuCommand::SignOut => "account-menu.sign-out",
        }
    }

    fn name_for_type() -> &'static str
    where
        Self: Sized,
    {
        "AccountMenuAction"
    }

    fn build(value: serde_json::Value) -> gpui::Result<Box<dyn Action>>
    where
        Self: Sized,
    {
        let command =
            AccountMenuCommand::from_value(&value).unwrap_or(AccountMenuCommand::Settings);
        Ok(Box::new(Self::new(command)))
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountMenuCommand, UnifiedToolbarProps, unified_toolbar};
    use gpui::{IntoElement, Render, TestAppContext, Window};
    use serde_json::json;

    #[test]
    fn from_str_maps_known_commands_and_rejects_unknown() {
        assert_eq!(
            AccountMenuCommand::from_str("settings"),
            Some(AccountMenuCommand::Settings)
        );
        assert_eq!(
            AccountMenuCommand::from_str("sign-out"),
            Some(AccountMenuCommand::SignOut)
        );
        assert_eq!(AccountMenuCommand::from_str("nope"), None);
    }

    #[test]
    fn from_value_accepts_string_and_command_object() {
        assert_eq!(
            AccountMenuCommand::from_value(&json!("themes")),
            Some(AccountMenuCommand::Themes)
        );
        assert_eq!(
            AccountMenuCommand::from_value(&json!({ "command": "keymap" })),
            Some(AccountMenuCommand::Keymap)
        );
        assert_eq!(AccountMenuCommand::from_value(&json!(42)), None);
    }

    struct ToolbarHost {
        props: UnifiedToolbarProps,
        renders: usize,
    }

    impl Render for ToolbarHost {
        fn render(
            &mut self,
            _window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            self.renders += 1;
            unified_toolbar(self.props.clone(), cx)
        }
    }

    #[gpui::test]
    async fn toolbar_builds_drag_region_and_account_button(cx: &mut TestAppContext) {
        // The account `Button` widget reads the gpui-component `Theme` global.
        cx.update(gpui_component::init);
        let props = UnifiedToolbarProps {
            account_name: "Ada".into(),
            account_plan: "Pro".into(),
        };
        let (host, cx) = cx.add_window_view(|_window, _cx| ToolbarHost { props, renders: 0 });
        cx.run_until_parked();
        host.read_with(cx, |host, _cx| assert!(host.renders > 0));
    }
}
