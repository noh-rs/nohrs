//! Explorer file operations: delete (trash / permanent), copy / cut / paste with
//! conflict resolution, inline rename, and new folder
//! (`docs/explorer-essentials.md` §1).
//!
//! Every filesystem mutation is routed through `nohrs_services::fs::ops` rather
//! than `std::fs` (§8). Potentially slow work (recursive copy, deleting trees,
//! trashing many items) runs on the background executor; the cheap single-syscall
//! operations (rename, mkdir) run inline for immediate feedback. Errors are
//! surfaced to the footer status bar — the aggregated error dialog and progress
//! UI are tracked separately in #188.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{InputEvent, InputState};
use nohrs_core::errors::Result;
use nohrs_services::fs::ops::{self, ConflictResolution};

use super::state::ExplorerPane;
use super::types::StatusLevel;

/// Whether a clipboard entry should be duplicated (copy) or relocated (cut) when
/// pasted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipMode {
    Copy,
    Cut,
}

/// Process-wide explorer clipboard, shared across panes and windows so a copy in
/// one pane can be pasted in another. The paths are mirrored to the system
/// clipboard as text for interop with external apps, but this global stays the
/// source of truth for paste because the system clipboard cannot carry the
/// copy/cut distinction across platforms.
#[derive(Clone, Default)]
pub(crate) struct ExplorerClipboard {
    entry: Option<(ClipMode, Vec<PathBuf>)>,
}

impl Global for ExplorerClipboard {}

/// State of an in-progress inline rename of the listing row at `index` (an index
/// into `filtered_entries`).
pub(crate) struct RenameState {
    /// Row currently showing the rename input.
    pub index: usize,
    /// Full path of the entry being renamed.
    pub original_path: PathBuf,
    /// The text field the user edits the name in.
    pub input: Entity<InputState>,
    // Kept alive so the input's `PressEnter` / `Blur` events drive commit; dropped
    // (deregistering the handler) when the rename ends.
    _subscription: Subscription,
}

/// A planned paste whose destination name conflicts are being resolved one at a
/// time through the conflict dialog. Building it only stats the destinations; the
/// actual copy / move runs once every conflict has a resolution.
pub(crate) struct PastePlan {
    mode: ClipMode,
    dest_dir: PathBuf,
    // Sources with no destination collision: copied / moved as-is.
    clear: Vec<PathBuf>,
    // Conflicting sources still awaiting a user decision (the front is shown).
    pending: VecDeque<PathBuf>,
    // Conflicting sources the user has already decided on.
    resolved: Vec<(PathBuf, ConflictResolution)>,
    // When ticked, the next choice in the dialog applies to every remaining
    // `pending` entry at once (§1.2 "Apply to all").
    apply_to_all: bool,
}

// The display name (final path component) of `path`, if it has one.
fn file_name_of(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

// Performs a single copy or move from `src` to `dst` according to `mode`.
fn apply_one(mode: ClipMode, src: &Path, dst: &Path) -> Result<()> {
    match mode {
        ClipMode::Copy => ops::copy_path(src, dst),
        ClipMode::Cut => ops::move_path(src, dst).map(|_| ()),
    }
}

impl ExplorerPane {
    // Runs `op` (which performs the filesystem work and returns one message per
    // failure) on the background executor, then reloads the listing and reports
    // the outcome in the status bar. `total` and `success_label` describe the
    // batch for the success / partial-failure messages.
    fn run_fs_op<F>(&mut self, total: usize, success_label: String, cx: &mut Context<Self>, op: F)
    where
        F: FnOnce() -> Vec<String> + Send + 'static,
    {
        let task = cx.background_spawn(async move { op() });
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let errors = task.await;
                this.update(&mut cx, |pane, cx| {
                    pane.reload();
                    if errors.is_empty() {
                        pane.set_status(StatusLevel::Info, success_label);
                    } else {
                        for error in &errors {
                            tracing::error!("explorer file operation failed: {error}");
                        }
                        pane.set_status(
                            StatusLevel::Error,
                            format!("{} of {total} failed", errors.len()),
                        );
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    // ---- Delete (§1.1) ----

    /// Moves the selected entries to the OS trash.
    pub(crate) fn trash_selection(&mut self, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let total = paths.len();
        let label = format!("{total} item(s) moved to trash");
        self.run_fs_op(total, label, cx, move || {
            let mut errors = Vec::new();
            for path in paths {
                if let Err(error) = ops::trash_path(Path::new(&path)) {
                    errors.push(format!("{path}: {error}"));
                }
            }
            errors
        });
    }

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
            let name = file_name_of(Path::new(&paths[0])).unwrap_or_else(|| paths[0].clone());
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
                        .ok();
                    true
                })
        });
    }

    pub(crate) fn delete_permanent_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let total = paths.len();
        let label = format!("{total} item(s) deleted");
        self.run_fs_op(total, label, cx, move || {
            let mut errors = Vec::new();
            for path in paths {
                if let Err(error) = ops::delete_permanent(Path::new(&path)) {
                    errors.push(format!("{path}: {error}"));
                }
            }
            errors
        });
    }

    // ---- Copy / cut / paste (§1.2) ----

    /// Marks the selection for copy on the next paste.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        self.set_clipboard(ClipMode::Copy, cx);
    }

    /// Marks the selection for move on the next paste.
    pub(crate) fn cut_selection(&mut self, cx: &mut Context<Self>) {
        self.set_clipboard(ClipMode::Cut, cx);
    }

    fn set_clipboard(&mut self, mode: ClipMode, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let buffers = paths.iter().map(PathBuf::from).collect();
        cx.set_global(ExplorerClipboard {
            entry: Some((mode, buffers)),
        });
        cx.write_to_clipboard(ClipboardItem::new_string(paths.join("\n")));
        let verb = match mode {
            ClipMode::Copy => "copied",
            ClipMode::Cut => "cut",
        };
        self.set_status(StatusLevel::Info, format!("{} item(s) {verb}", paths.len()));
        cx.notify();
    }

    fn read_clipboard(&self, cx: &App) -> Option<(ClipMode, Vec<PathBuf>)> {
        if let Some((mode, paths)) = cx
            .try_global::<ExplorerClipboard>()
            .and_then(|clip| clip.entry.clone())
        {
            return Some((mode, paths));
        }
        // Fall back to the system clipboard as newline-separated path text (e.g.
        // a path copied from elsewhere), treated as a copy.
        let text = cx.read_from_clipboard()?.text()?;
        let paths: Vec<PathBuf> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect();
        (!paths.is_empty()).then_some((ClipMode::Copy, paths))
    }

    /// Pastes the clipboard contents into the current directory, prompting for
    /// each name collision (§1.2).
    pub(crate) fn paste_into_cwd(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_pending = self.prepare_paste(cx);
        if self.paste_plan.is_none() {
            return;
        }
        if has_pending {
            self.open_conflict_dialog(window, cx);
        } else {
            self.execute_paste_plan(cx);
        }
    }

    // Builds the paste plan from the clipboard into `self.paste_plan`, splitting
    // sources into collision-free and conflicting sets without touching the UI.
    // Returns whether any conflict needs the dialog. Separated from
    // `paste_into_cwd` so it can be exercised without opening a dialog.
    pub(crate) fn prepare_paste(&mut self, cx: &mut Context<Self>) -> bool {
        self.paste_plan = None;
        let Some((mode, sources)) = self.read_clipboard(cx) else {
            return false;
        };
        let dest_dir = PathBuf::from(&self.cwd);
        let mut clear = Vec::new();
        let mut pending = VecDeque::new();
        let mut resolved = Vec::new();
        for src in sources {
            let Some(name) = src.file_name() else {
                continue;
            };
            let dst = dest_dir.join(name);
            if dst == src {
                // Pasting an item back into its own directory: a copy becomes a
                // numbered duplicate; a cut onto itself is a no-op.
                if mode == ClipMode::Copy {
                    resolved.push((src, ConflictResolution::Rename));
                }
                continue;
            }
            if ops::would_conflict(&dst) {
                pending.push_back(src);
            } else {
                clear.push(src);
            }
        }
        let has_pending = !pending.is_empty();
        self.paste_plan = Some(PastePlan {
            mode,
            dest_dir,
            clear,
            pending,
            resolved,
            apply_to_all: false,
        });
        has_pending
    }

    /// Records the user's choice for the conflict currently shown, advancing to
    /// the next one. Returns `true` when no conflicts remain to resolve.
    pub(crate) fn resolve_current_conflict(
        &mut self,
        choice: ConflictResolution,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(plan) = self.paste_plan.as_mut() else {
            return true;
        };
        if plan.apply_to_all {
            while let Some(src) = plan.pending.pop_front() {
                plan.resolved.push((src, choice));
            }
        } else if let Some(src) = plan.pending.pop_front() {
            plan.resolved.push((src, choice));
        }
        let done = plan.pending.is_empty();
        cx.notify();
        done
    }

    /// Toggles the conflict dialog's "apply to all" checkbox.
    pub(crate) fn set_apply_to_all(&mut self, value: bool, cx: &mut Context<Self>) {
        if let Some(plan) = self.paste_plan.as_mut() {
            plan.apply_to_all = value;
            cx.notify();
        }
    }

    /// Abandons an in-progress paste (the conflict dialog's Cancel).
    pub(crate) fn cancel_paste(&mut self, cx: &mut Context<Self>) {
        if self.paste_plan.take().is_some() {
            cx.notify();
        }
    }

    /// Executes the resolved paste plan on the background executor.
    pub(crate) fn execute_paste_plan(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.paste_plan.take() else {
            return;
        };
        let PastePlan {
            mode,
            dest_dir,
            clear,
            resolved,
            ..
        } = plan;
        let total = clear.len() + resolved.len();
        if total == 0 {
            return;
        }
        // A cut consumes the clipboard: clear it now so a second paste does not
        // try to move the (already relocated) sources again.
        if mode == ClipMode::Cut {
            cx.set_global(ExplorerClipboard::default());
        }
        let label = format!(
            "{total} item(s) {}",
            if mode == ClipMode::Cut {
                "moved"
            } else {
                "pasted"
            }
        );
        self.run_fs_op(total, label, cx, move || {
            let mut errors = Vec::new();
            for src in clear {
                if let Some(name) = src.file_name() {
                    let dst = dest_dir.join(name);
                    if let Err(error) = apply_one(mode, &src, &dst) {
                        errors.push(format!("{}: {error}", src.display()));
                    }
                }
            }
            for (src, resolution) in resolved {
                let Some(name) = file_name_of(&src) else {
                    continue;
                };
                let result = match resolution {
                    ConflictResolution::Skip => Ok(()),
                    ConflictResolution::Rename => {
                        let unique = ops::unique_name(&dest_dir, &name);
                        apply_one(mode, &src, &dest_dir.join(unique))
                    }
                    ConflictResolution::Overwrite => {
                        let dst = dest_dir.join(&name);
                        // Remove the existing destination first so directories are
                        // replaced rather than merged.
                        if let Err(error) = ops::delete_permanent(&dst) {
                            errors.push(format!("{}: {error}", dst.display()));
                            continue;
                        }
                        apply_one(mode, &src, &dst)
                    }
                };
                if let Err(error) = result {
                    errors.push(format!("{}: {error}", src.display()));
                }
            }
            errors
        });
    }

    fn open_conflict_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            dialog
                .title(format!(
                    "\u{201c}{name}\u{201d} already exists in \u{201c}{dest_name}\u{201d}"
                ))
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
                                            .ok();
                                    }),
                            )
                        }),
                )
                .footer(move |_ok, _cancel, _window, _cx| {
                    vec![
                        conflict_button(&weak, "skip", "Skip", ConflictResolution::Skip),
                        conflict_button(&weak, "rename", "Rename", ConflictResolution::Rename),
                        cancel_button(&weak),
                        overwrite_button(&weak),
                    ]
                })
        });
    }

    // ---- Rename + new folder (§1, §6) ----

    /// Starts an inline rename of the row at `index`, focusing a text field
    /// seeded with the current name.
    pub(crate) fn begin_rename(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.filtered_entries.get(index) else {
            return;
        };
        let original_path = PathBuf::from(&entry.path);
        let name = entry.name.clone();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(name, window, cx);
            state.focus(window, cx);
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _input, event: &InputEvent, window, cx| {
                match event {
                    // Commit on Enter, and on Blur so clicking away also saves
                    // (Finder-style). Enter tears the field down first, so the
                    // trailing Blur finds no active rename and is a no-op.
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.commit_rename(window, cx)
                    }
                    _ => {}
                }
            },
        );
        self.renaming = Some(RenameState {
            index,
            original_path,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Commits the in-progress rename, resolving a name collision by numbering
    /// (§1.2 "Rename"). An empty or unchanged name cancels.
    pub(crate) fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.renaming.take() else {
            return;
        };
        // The rename field had keyboard focus; return it to the listing so the
        // user can keep navigating and acting on entries without re-clicking.
        cx.focus_self(window);
        let new_name = state.input.read(cx).value().trim().to_string();
        let original_path = state.original_path;
        let original_name = file_name_of(&original_path).unwrap_or_default();
        if new_name.is_empty() || new_name == original_name {
            cx.notify();
            return;
        }
        let final_name = match original_path.parent() {
            Some(parent) if ops::would_conflict(&parent.join(&new_name)) => {
                ops::unique_name(parent, &new_name)
            }
            _ => new_name,
        };
        match ops::rename_in_place(&original_path, &final_name) {
            Ok(_) => {
                self.reload();
                self.set_status(StatusLevel::Info, format!("Renamed to {final_name}"));
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, format!("Rename failed: {error}"));
            }
        }
        cx.notify();
    }

    /// Creates a new folder in the current directory and immediately starts an
    /// inline rename of it so the user can name it (§1, §6).
    pub(crate) fn create_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = PathBuf::from(&self.cwd);
        let name = ops::unique_name(&parent, "New Folder");
        match ops::create_dir(&parent, &name) {
            Ok(path) => {
                self.reload();
                let created = path.to_string_lossy();
                if let Some(index) = self
                    .filtered_entries
                    .iter()
                    .position(|entry| entry.path == created)
                {
                    self.select_single(index);
                    self.begin_rename(index, window, cx);
                }
                cx.notify();
            }
            Err(error) => {
                self.set_status(
                    StatusLevel::Error,
                    format!("Couldn't create folder: {error}"),
                );
                cx.notify();
            }
        }
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
    // Right-most and danger-coloured to keep it away from the safe defaults
    // (§1.2 mock).
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
            weak.update(cx, |pane, cx| pane.cancel_paste(cx)).ok();
        })
}
