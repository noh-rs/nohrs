//! Modal dialogs for explorer file operations: the permanent-delete confirmation
//! and the paste name-conflict resolver (`docs/explorer-essentials.md` §1.1,
//! §1.2). These render on `gpui_component`'s `ContextModal` layer (mounted in the
//! window root) and are driven entirely by the operation state held on
//! [`ExplorerPane`]; the filesystem logic lives in `super::super::file_ops`.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::dialog::DialogButtonProps;
use nohrs_core::telemetry::LogErr as _;
use nohrs_services::fs::ops::ConflictResolution;

use crate::explorer::ExplorerPane;
use crate::explorer::file_ops::file_name_of;

impl ExplorerPane {
    /// Opens a confirmation dialog before permanently deleting the selection
    /// (§1.1 — permanent delete is the one operation undo cannot reverse, so it
    /// is gated behind a confirm).
    pub(crate) fn request_permanent_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        let title = if count == 1 {
            let name =
                file_name_of(std::path::Path::new(&paths[0])).unwrap_or_else(|| paths[0].clone());
            format!("Permanently delete \u{201c}{name}\u{201d}?")
        } else {
            format!("Permanently delete {count} items?")
        };
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let weak = weak.clone();
            let paths = paths.clone();
            dialog
                .confirm()
                .title(title.clone())
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .child("This can't be undone.")
                .on_ok(move |_, _window, cx| {
                    let paths = paths.clone();
                    weak.update(cx, |pane, cx| pane.delete_permanent_paths(paths, cx))
                        .log_err();
                    true
                })
        });
    }

    /// Opens the paste name-conflict dialog, resolving one collision at a time
    /// against the pane's [`super::super::file_ops::PastePlan`] (§1.2).
    pub(crate) fn open_conflict_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let weak = weak.clone();
            let Some(pane) = weak.upgrade() else {
                return dialog;
            };
            let Some((name, dest_name, remaining, apply_to_all)) = pane.read_with(cx, |pane, _| {
                let plan = pane.paste_plan.as_ref()?;
                let current = plan.pending.front()?;
                let name = file_name_of(current).unwrap_or_default();
                let dest_name = file_name_of(&plan.dest_dir)
                    .unwrap_or_else(|| plan.dest_dir.display().to_string());
                Some((name, dest_name, plan.pending.len() - 1, plan.apply_to_all))
            }) else {
                return dialog;
            };

            let checkbox_weak = weak.clone();
            let close_weak = weak.clone();
            dialog
                .title(format!(
                    "\u{201c}{name}\u{201d} already exists in \u{201c}{dest_name}\u{201d}"
                ))
                // Dismissing without choosing has to abandon the plan exactly as
                // Cancel does, or the pending queue survives and the next paste
                // resumes into this one.
                .on_close(move |_, _window, cx| {
                    close_weak
                        .update(cx, |pane, cx| pane.cancel_paste(cx))
                        .log_err();
                })
                // Enter reaches this dialog as `Confirm`. With a custom footer
                // and no `on_ok`, gpui-component 0.5.1 takes its `else if
                // has_footer` branch and closes the dialog *without* running
                // `on_close` — leaking the plan the same way the close button
                // used to. Claiming `on_ok` puts Enter back on the path that
                // runs `on_close`, so it abandons the paste like Escape does
                // rather than silently picking one of the four outcomes.
                .on_ok(|_, _window, _cx| true)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Choose how to resolve the name conflict.")
                        .when(remaining > 0, |this| {
                            this.child(
                                Checkbox::new("apply-to-all")
                                    .label(format!(
                                        "Apply to all remaining conflicts ({remaining} more)"
                                    ))
                                    .checked(apply_to_all)
                                    .on_click(move |checked, _window, cx| {
                                        checkbox_weak
                                            .update(cx, |pane, cx| {
                                                pane.set_apply_to_all(*checked, cx)
                                            })
                                            .log_err();
                                    }),
                            )
                        }),
                )
                // Cancel leads, and Rename — the only choice that loses nothing —
                // takes the trailing primary slot. Overwrite sits inboard of it:
                // the destructive action must not occupy the position the eye
                // (and a stray Return) treats as the default.
                .footer(move |_ok, _cancel, _window, _cx| {
                    vec![
                        cancel_button(&weak),
                        overwrite_button(&weak),
                        conflict_button(&weak, "skip", "Skip", ConflictResolution::Skip),
                        conflict_button(&weak, "rename", "Rename", ConflictResolution::Rename)
                            .primary(),
                    ]
                })
        });
    }
}

// Builds one of the conflict dialog's resolution buttons.
fn conflict_button(
    weak: &WeakEntity<ExplorerPane>,
    id: &'static str,
    label: &'static str,
    choice: ConflictResolution,
) -> Button {
    let weak = weak.clone();
    Button::new(id).label(label).on_click(move |_, window, cx| {
        let Some(pane) = weak.upgrade() else {
            return;
        };
        let done = pane.update(cx, |pane, cx| pane.resolve_current_conflict(choice, cx));
        if done {
            window.close_dialog(cx);
            pane.update(cx, |pane, cx| pane.execute_paste_plan(cx));
        }
    })
}

fn overwrite_button(weak: &WeakEntity<ExplorerPane>) -> Button {
    // Danger-coloured, and placed away from the trailing default slot (§1.2).
    conflict_button(
        weak,
        "overwrite",
        "Overwrite",
        ConflictResolution::Overwrite,
    )
    .danger()
}

fn cancel_button(weak: &WeakEntity<ExplorerPane>) -> Button {
    let weak = weak.clone();
    Button::new("cancel-paste")
        .label("Cancel")
        .on_click(move |_, window, cx| {
            window.close_dialog(cx);
            weak.update(cx, |pane, cx| pane.cancel_paste(cx)).log_err();
        })
}
