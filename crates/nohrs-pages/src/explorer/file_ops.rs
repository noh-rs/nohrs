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

use gpui::*;
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
    // Read by the conflict dialog (in the `view` module) to label the prompt.
    pub(crate) dest_dir: PathBuf,
    // Sources with no destination collision: copied / moved as-is.
    clear: Vec<PathBuf>,
    // Conflicting sources still awaiting a user decision (the front is shown).
    pub(crate) pending: VecDeque<PathBuf>,
    // Conflicting sources the user has already decided on.
    resolved: Vec<(PathBuf, ConflictResolution)>,
    // When ticked, the next choice in the dialog applies to every remaining
    // `pending` entry at once (§1.2 "Apply to all").
    pub(crate) apply_to_all: bool,
}

// The display name (final path component) of `path`, if it has one.
pub(crate) fn file_name_of(path: &Path) -> Option<String> {
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

// Overwrites `dst` with `src` without risking data loss on failure: the existing
// destination is moved aside first, and only deleted once the paste succeeds;
// if the paste fails, the original is rolled back into place. Staging aside (vs.
// deleting upfront) also makes a directory a clean replace rather than a merge.
fn overwrite_apply(mode: ClipMode, src: &Path, dst: &Path) -> Result<()> {
    use nohrs_core::telemetry::LogErr as _;
    if !ops::would_conflict(dst) {
        return apply_one(mode, src, dst);
    }
    let backup = backup_path(dst);
    ops::move_path(dst, &backup)?;
    match apply_one(mode, src, dst) {
        Ok(()) => {
            ops::delete_permanent(&backup).log_err();
            Ok(())
        }
        Err(error) => {
            // The failed paste may have left a partial entry at `dst`; remove it
            // first so the original can be moved back rather than stranded in the
            // backup. If the restore itself fails, log loudly — the data still
            // exists at `backup`, but the destination is now wrong.
            if ops::would_conflict(dst) {
                ops::delete_permanent(dst).log_err();
            }
            if let Err(restore_error) = ops::move_path(&backup, dst) {
                tracing::error!(
                    "failed to restore {} after a failed overwrite (original kept at {}): {restore_error}",
                    dst.display(),
                    backup.display(),
                );
            }
            Err(error)
        }
    }
}

// A sibling path of `dst` that does not yet exist, used to stage the existing
// destination aside during an overwrite.
fn backup_path(dst: &Path) -> PathBuf {
    let parent = dst.parent().unwrap_or_else(|| Path::new("."));
    let name = file_name_of(dst).unwrap_or_default();
    parent.join(ops::unique_name(parent, &format!(".{name}.nohrs-tmp")))
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
        // Destination names already claimed by an earlier source in this same
        // batch, so two sources sharing a basename (possible via the system
        // clipboard) don't silently overwrite each other.
        let mut claimed = std::collections::HashSet::new();
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
            } else if !claimed.insert(dst) {
                // Another source already targets this name; number it instead of
                // letting the later paste clobber the earlier one.
                resolved.push((src, ConflictResolution::Rename));
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
        // Skips do nothing, so drop them before counting — otherwise the success
        // message would claim more items were pasted than actually were.
        let resolved: Vec<(PathBuf, ConflictResolution)> = resolved
            .into_iter()
            .filter(|(_, resolution)| *resolution != ConflictResolution::Skip)
            .collect();
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
                    // Filtered out above; kept for exhaustiveness without panicking.
                    ConflictResolution::Skip => continue,
                    ConflictResolution::Rename => {
                        let unique = ops::unique_name(&dest_dir, &name);
                        apply_one(mode, &src, &dest_dir.join(unique))
                    }
                    ConflictResolution::Overwrite => {
                        overwrite_apply(mode, &src, &dest_dir.join(&name))
                    }
                };
                if let Err(error) = result {
                    errors.push(format!("{}: {error}", src.display()));
                }
            }
            errors
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
