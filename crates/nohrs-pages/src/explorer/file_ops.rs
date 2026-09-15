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
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use nohrs_core::errors::{Error, Result};
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
    // Bumped on every write. A paste records the generation it leaves behind so
    // that, finishing later, it can tell its own clipboard state apart from one
    // the user — or an overlapping paste — has claimed since. Comparing against
    // "is the clipboard empty" cannot: two cut pastes in flight both leave it
    // empty, and whichever finishes first would look like the owner of both.
    generation: u64,
    // Cut pastes whose background move has not finished yet. Emptying `entry` is
    // what stops a second paste from moving the same sources twice, but the
    // paths are also mirrored to the system clipboard, and an empty `entry` is
    // exactly when `read_clipboard` falls back to that text — as a *copy*,
    // racing the move it was meant to stand aside for. While this is non-zero
    // the clipboard is deliberately empty rather than unset, so there is nothing
    // to fall back to.
    cuts_in_flight: u32,
}

impl Global for ExplorerClipboard {}

impl ExplorerClipboard {
    // Replaces the clipboard contents, returning the generation stamped on this
    // write. Mutates in place rather than replacing the global so a write during
    // a cut paste does not lose the in-flight count.
    fn write(entry: Option<(ClipMode, Vec<PathBuf>)>, cx: &mut App) -> u64 {
        let clipboard = cx.default_global::<ExplorerClipboard>();
        clipboard.generation = clipboard.generation.wrapping_add(1);
        clipboard.entry = entry;
        clipboard.generation
    }

    fn generation(cx: &App) -> u64 {
        cx.try_global::<ExplorerClipboard>()
            .map_or(0, |clip| clip.generation)
    }

    fn begin_cut(cx: &mut App) {
        let clipboard = cx.default_global::<ExplorerClipboard>();
        clipboard.cuts_in_flight = clipboard.cuts_in_flight.saturating_add(1);
    }

    fn end_cut(cx: &mut App) {
        let clipboard = cx.default_global::<ExplorerClipboard>();
        clipboard.cuts_in_flight = clipboard.cuts_in_flight.saturating_sub(1);
    }

    fn cut_in_flight(cx: &App) -> bool {
        cx.try_global::<ExplorerClipboard>()
            .is_some_and(|clip| clip.cuts_in_flight > 0)
    }
}

// Puts the sources a cut-paste could not move back on the clipboard, so the user
// can retry them as a move. Without this the cleared clipboard falls through to
// the system clipboard's path text, which carries no cut/copy distinction and so
// retries a failed *move* as a *copy*.
//
// Only restores when the clipboard is still the one this paste left behind:
// anything else means newer state the user can see, which is the live one.
// Takes `&mut App` rather than the pane so a batch outliving the pane that
// started it still restores — the clipboard is process-wide, and the retry
// matters to whatever pane the user is looking at now.
fn retain_failed_cut(failed: Vec<PathBuf>, generation: u64, cx: &mut App) {
    if failed.is_empty() || ExplorerClipboard::generation(cx) != generation {
        return;
    }
    ExplorerClipboard::write(Some((ClipMode::Cut, failed)), cx);
}

// The path `name` should take inside `dir`, numbered if the filesystem already
// has that name.
//
// Goes through `ops::destination_in` rather than joining, so a name that is not
// a single plain component — anything a path in the system clipboard, or a path
// dropped on the pane, could carry — cannot aim the paste at a directory the
// user never opened.
//
// An error where the name is taken and cannot be numbered because it is not
// valid UTF-8: a reported failure beats silently writing over whatever is there.
fn free_destination(dir: &Path, name: &OsStr) -> Result<PathBuf> {
    let dst = ops::destination_in(dir, name)?;
    if !ops::would_conflict(&dst) {
        return Ok(dst);
    }
    let name = name.to_str().ok_or_else(|| {
        Error::Other(format!(
            "{} is taken and its name cannot be numbered",
            dst.display()
        ))
    })?;
    ops::destination_in(dir, OsStr::new(&ops::unique_name(dir, name)))
}

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

/// Where a rename's initial selection ends: after the name minus its extension,
/// so typing replaces what you meant and `.rs` survives. Directories select
/// whole — their dots are part of the name, not a suffix — as do files with no
/// extension.
///
/// Measured in UTF-16 units because that is what the input's text API takes.
/// Keys off `file_stem` rather than the last `.` so a dotfile stays whole:
/// `.gitignore` is all stem, where `rfind('.')` would select nothing at all.
fn rename_selection_end(name: &str, is_dir: bool) -> usize {
    let whole = name.encode_utf16().count();
    if is_dir {
        return whole;
    }
    Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().encode_utf16().count())
        .filter(|end| *end > 0)
        .unwrap_or(whole)
}

// Performs a single copy or move from `src` to `dst` according to `mode`.
//
// Claims `dst` rather than writing over it. Every destination here was chosen by
// looking at the filesystem — `free_destination`, `unique_name`, the conflict
// dialog — and between that look and this call the name can be taken by anything
// else on the machine. A replacing write would destroy what took it; refusing
// costs one reported failure.
fn apply_one(mode: ClipMode, src: &Path, dst: &Path) -> ops::ClaimResult<()> {
    match mode {
        ClipMode::Copy => ops::copy_path_no_replace(src, dst),
        ClipMode::Cut => ops::move_path_no_replace(src, dst).map(|_| ()),
    }
}

// Overwrites `dst` with `src` without risking data loss on failure: the existing
// destination is moved aside first, and only deleted once the paste succeeds;
// if the paste fails, the original is rolled back into place. Staging aside (vs.
// deleting upfront) also makes a directory a clean replace rather than a merge.
//
// The new entry is built at a path only this call knows and moved onto `dst` at
// the end, so the destination is claimed once the replacement is whole rather
// than held open for the length of a copy.
fn overwrite_apply(mode: ClipMode, src: &Path, dst: &Path) -> Result<()> {
    use nohrs_core::telemetry::LogErr as _;
    if !ops::would_conflict(dst) {
        return Ok(apply_one(mode, src, dst)?);
    }
    let backup = scratch_path(dst, "old");
    if let Err(failure) = ops::move_path_no_replace(dst, &backup) {
        drop_claimed(&failure, &backup);
        // The same stranding the staging step can suffer, one step earlier: the
        // copy to `backup` landed and only clearing `dst` gave up partway, so
        // the whole original is at a hidden scratch path and `dst` holds what
        // the removal spared. Nothing else in this function reaches `backup`
        // from here, so this message is the only record of where it went.
        if failure.leftover() == ops::Leftover::WholeDestination {
            tracing::error!(
                "{} could not be cleared after copying it aside; the whole original is at {}",
                dst.display(),
                backup.display(),
            );
        }
        return Err(failure.into());
    }
    let staged = scratch_path(dst, "new");
    let Err(error) = build_and_commit(mode, src, dst, &staged) else {
        ops::delete_permanent(&backup).log_err();
        return Ok(());
    };
    // Put the original back. Whatever took `dst` in the meantime is not ours to
    // replace, so a failure here leaves the original at `backup` and says where.
    if let Err(failure) = ops::move_path_no_replace(&backup, dst) {
        // Unless the rollback itself got as far as taking `dst`: then what is
        // there is the rollback's own fragment, and it has to go for the message
        // below — the original is at `backup` — to be the whole truth.
        drop_claimed(&failure, dst);
        tracing::error!(
            "failed to restore {} after a failed overwrite (original kept at {}): {failure}",
            dst.display(),
            backup.display(),
        );
    }
    Err(error)
}

// Builds the replacement at `staged` and takes `dst` with it, leaving nothing of
// its own behind when it cannot. `dst` is free on entry — the caller moved what
// was there to the backup.
fn build_and_commit(mode: ClipMode, src: &Path, dst: &Path, staged: &Path) -> Result<()> {
    if let Err(failure) = apply_one(mode, src, staged) {
        // Anything a *partial* failure left at `staged` is a fragment the source
        // outlived: a rename that fails moves nothing, and the copy-and-delete
        // fallback deletes the source only once the copy is whole. That is what
        // makes it provable rather than a guess from whether `src` happens to be
        // occupied now, when something else may have re-created it since — and
        // what the failure did not write there is not ours to remove.
        drop_claimed(&failure, staged);
        // One failure does take the source away, and it is the reason the
        // sentence above says "partial": a cut whose copy to `staged` landed and
        // whose removal of `src` then gave up partway. `staged` holds the whole
        // original and `src` holds whatever `remove_dir_all` spared, so the one
        // thing that must not happen is the whole copy going quietly — it is at
        // a hidden scratch path nobody would think to look at.
        if failure.leftover() == ops::Leftover::WholeDestination {
            tracing::error!(
                "{} could not be cleared after copying it aside; the whole copy is at {}",
                src.display(),
                staged.display(),
            );
        }
        return Err(failure.into());
    }
    // Past here `apply_one` returned `Ok`, which for a cut is what says the
    // source is gone and `staged` holds the only copy of it there is.
    if let Err(failure) = ops::move_path_no_replace(staged, dst) {
        // Where the filesystem has no no-replace rename this was a copy, and one
        // that failed partway left a partial `dst` that would block the rollback
        // the caller is about to run. Removing it is safe only because the
        // failure says the name was this call's: `AlreadyExists` cannot, since
        // the kernel reports a collision *at* `dst` and one below it alike, and
        // reading the first as the second deletes a stranger's directory.
        drop_claimed(&failure, dst);
        // `WholeDestination` is not a failure to undo, or to report as one. The
        // replacement reached `dst` intact — which is the whole of what the user
        // asked for — and only clearing `staged` afterwards gave up partway.
        // Rolling back from here would take a finished overwrite apart, and a
        // cut would put whatever `remove_dir_all` spared where the user expects
        // their file, next to the whole copy. What is left is litter at a
        // scratch path, so it is logged the way a trashed item's unwritten
        // ledger row is, and the operation stands.
        if failure.leftover() == ops::Leftover::WholeDestination {
            tracing::warn!(
                "{} was replaced, but the staging copy at {} could not be cleared: {failure}",
                dst.display(),
                staged.display(),
            );
            return Ok(());
        }
        match mode {
            ClipMode::Cut => restore_staged(src, staged),
            ClipMode::Copy => drop_staged(staged),
        }
        return Err(failure.into());
    }
    Ok(())
}

// Puts a cut's source back where it came from. `staged` holds the only copy of
// it, so a failure here leaves it sitting there and says where, rather than
// deleting it.
fn restore_staged(src: &Path, staged: &Path) {
    if !ops::would_conflict(staged) {
        return;
    }
    if let Err(failure) = ops::move_path_no_replace(staged, src) {
        // A restore that got as far as taking the source's name and then failed
        // left a fragment standing where the source was; the whole copy is still
        // at `staged`, and the message below only tells the truth once that
        // fragment is gone.
        drop_claimed(&failure, src);
        tracing::error!(
            "failed to put {} back after a move that could not finish (it is at {}): {failure}",
            src.display(),
            staged.display(),
        );
    }
}

// Removes a staged entry the source outlived.
fn drop_staged(staged: &Path) {
    use nohrs_core::telemetry::LogErr as _;
    if ops::would_conflict(staged) {
        ops::delete_permanent(staged).log_err();
    }
}

// Clears away what a failed no-replace write left at the path it was writing to
// — and only when that is the write's own half-written work.
//
// The decision comes from the operation rather than from what happens to occupy
// the path now, because the cases are indistinguishable afterwards and cost very
// different things. A name the write never took holds someone else's entry; a
// whole copy whose source deletion failed may be the only copy left, since
// `remove_dir_all` gives up partway through. Removing either cannot be undone.
fn drop_claimed(failure: &ops::ClaimFailure, dst: &Path) {
    use nohrs_core::telemetry::LogErr as _;
    failure.discard_partial_destination(dst).log_err();
}

// A sibling path of `dst` that does not yet exist, for one side of an overwrite
// to work at. A sibling so the final rename stays within one filesystem, and
// hidden and suffixed so it is recognizable if a crash leaves one behind.
fn scratch_path(dst: &Path, role: &str) -> PathBuf {
    let parent = dst.parent().unwrap_or_else(|| Path::new("."));
    let name = file_name_of(dst).unwrap_or_default();
    parent.join(ops::unique_name(parent, &format!(".{name}.nohrs-{role}")))
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
        self.run_fs_op_with(total, success_label, cx, move || (op(), ()), |(), _| {});
    }

    // [`run_fs_op`](Self::run_fs_op) for batches that carry an outcome of their
    // own beyond the failure messages. `on_complete` takes `&mut App` rather
    // than the pane, and runs whether or not the pane outlived the batch, so an
    // outcome owning process-wide state is not dropped along with the pane that
    // started the work.
    fn run_fs_op_with<T, F, G>(
        &mut self,
        total: usize,
        success_label: String,
        cx: &mut Context<Self>,
        op: F,
        on_complete: G,
    ) where
        T: Send + 'static,
        F: FnOnce() -> (Vec<String>, T) + Send + 'static,
        G: FnOnce(T, &mut App) + 'static,
    {
        use nohrs_core::telemetry::LogErr as _;
        let task = cx.background_spawn(async move { op() });
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let (errors, outcome) = task.await;
                cx.update(|cx| on_complete(outcome, cx)).log_err();
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
        // The same ledger `noh rm` writes, on the platforms that need one: macOS
        // keeps its trash index inside Finder's private `.DS_Store` and exposes
        // nothing to read it back, so an item trashed without a record here can
        // never be restored — not by `noh trash restore`, not by us. Where that
        // record cannot be written, the delete is refused instead: putting the
        // item in the trash would succeed, and it would be a one-way door.
        let ledger = self.trash_ledger.clone();
        if let Err(error) = ledger.to_record() {
            self.set_status(StatusLevel::Error, format!("Can't move to trash: {error}"));
            cx.notify();
            return;
        }
        let total = paths.len();
        let label = format!("{total} item(s) moved to trash");
        self.run_fs_op(total, label, cx, move || {
            let mut errors = Vec::new();
            for path in paths {
                // Re-read per item rather than captured once: `to_record`
                // borrows from `ledger`, which this closure owns.
                let record = match ledger.to_record() {
                    Ok(record) => record,
                    Err(error) => {
                        errors.push(format!("{path}: {error}"));
                        continue;
                    }
                };
                if let Err(error) = ops::trash_path(Path::new(&path), record) {
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
        ExplorerClipboard::write(Some((mode, buffers)), cx);
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
        if ExplorerClipboard::cut_in_flight(cx) {
            // Emptied by a cut paste that is still moving those very sources.
            // Falling back now would hand them straight back as a copy.
            return None;
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
        // clipboard) don't silently overwrite each other. A name is claimed
        // whether or not the destination is already occupied: a conflicting name
        // resolved as Overwrite is still one destination, so a second source
        // aiming at it has to be numbered too.
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
            let occupied = ops::would_conflict(&dst);
            if !claimed.insert(dst) {
                // Another source already targets this name; number it instead of
                // letting the later paste clobber the earlier one.
                resolved.push((src, ConflictResolution::Rename));
            } else if occupied {
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
        // A cut consumes the clipboard: clear it now so a second paste, fired
        // while this one is still running, does not try to move the sources a
        // second time. Whatever fails to move is put back afterwards by
        // `retain_failed_cut`, which uses this generation to recognize the
        // clipboard state it left behind.
        let cut_generation = (mode == ClipMode::Cut).then(|| {
            ExplorerClipboard::begin_cut(cx);
            ExplorerClipboard::write(None, cx)
        });
        let label = format!(
            "{total} item(s) {}",
            if mode == ClipMode::Cut {
                "moved"
            } else {
                "pasted"
            }
        );
        self.run_fs_op_with(
            total,
            label,
            cx,
            move || {
                let mut errors = Vec::new();
                let mut failed = Vec::new();
                for src in clear {
                    // Owned so the borrow of `src` ends before `src` is moved
                    // into `failed`.
                    let Some(name) = src.file_name().map(OsStr::to_os_string) else {
                        continue;
                    };
                    // The plan reserved this name by comparing paths lexically,
                    // which is not how the filesystem compares them: on a
                    // case-insensitive volume `foo.txt` and `FOO.txt` are one
                    // destination, and the second source here would replace the
                    // first. Asking the filesystem instead also covers a name
                    // that appeared after the plan was built.
                    let dst = match free_destination(&dest_dir, &name) {
                        Ok(dst) => dst,
                        Err(error) => {
                            errors.push(format!("{}: {error}", src.display()));
                            failed.push(src);
                            continue;
                        }
                    };
                    if let Err(error) = apply_one(mode, &src, &dst) {
                        errors.push(format!("{}: {error}", src.display()));
                        failed.push(src);
                    }
                }
                for (src, resolution) in resolved {
                    let Some(name) = src.file_name().map(OsStr::to_os_string) else {
                        continue;
                    };
                    let result = match resolution {
                        // Filtered out above; kept for exhaustiveness without panicking.
                        ConflictResolution::Skip => continue,
                        ConflictResolution::Rename => free_destination(&dest_dir, &name)
                            .and_then(|dst| apply_one(mode, &src, &dst).map_err(Error::from)),
                        ConflictResolution::Overwrite => ops::destination_in(&dest_dir, &name)
                            .and_then(|dst| overwrite_apply(mode, &src, &dst)),
                    };
                    if let Err(error) = result {
                        errors.push(format!("{}: {error}", src.display()));
                        failed.push(src);
                    }
                }
                (errors, failed)
            },
            move |failed, cx| {
                if let Some(generation) = cut_generation {
                    retain_failed_cut(failed, generation, cx);
                    ExplorerClipboard::end_cut(cx);
                }
            },
        );
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
        let selection_end = rename_selection_end(&name, entry.kind == "dir");
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            // Seed the field *and* pre-select the part being renamed in one
            // step. `set_value` always parks the caret at the end, and the
            // selected range itself is not public, so this IME entry point is
            // the only way in: inserting into the empty field lets it place the
            // selection, and `unmark_text` then drops the composition marker so
            // the result renders as ordinary selected text.
            state.replace_and_mark_text_in_range(
                Some(0..0),
                &name,
                Some(0..selection_end),
                window,
                cx,
            );
            state.unmark_text(window, cx);
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

    /// Abandons an in-progress rename without touching the filesystem, returning
    /// focus to the listing. Bound to Escape while the field is open, and used
    /// when the row being renamed goes away underneath the field — navigating to
    /// another directory, say — since the field is positioned by row index and
    /// would otherwise re-attach to whichever entry now occupies that slot.
    pub(crate) fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.discard_rename(cx) {
            cx.focus_self(window);
        }
    }

    /// [`cancel_rename`](Self::cancel_rename) for callers without a `Window`
    /// (pane-sync navigation). Returns whether a rename was in progress. Focus
    /// is left alone, so the field's own blur handling settles it.
    pub(crate) fn discard_rename(&mut self, cx: &mut Context<Self>) -> bool {
        if self.renaming.take().is_some() {
            cx.notify();
            return true;
        }
        false
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
        // Typed by the user, so it can be anything. A name that is not a single
        // plain component is left as it is, for `rename_in_place` to reject with
        // the one message for it: numbering `../notes.txt` would quietly turn a
        // rename that was going to fail into one that renames the entry to a
        // sibling of whatever `..` reached.
        let final_name = match original_path.parent() {
            Some(parent)
                if ops::destination_in(parent, OsStr::new(&new_name))
                    .is_ok_and(|dst| ops::would_conflict(&dst)) =>
            {
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

#[cfg(test)]
// Staging real entries on disk is the point of these; there is no UI thread in
// a test binary to keep responsive.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod overwrite_recovery_tests {
    use super::{ClipMode, drop_claimed, drop_staged, overwrite_apply, restore_staged};
    use nohrs_services::fs::ops;

    // A directory holding something a copy cannot read, so a no-replace copy of
    // it stops at a known point *after* it has created the destination. That is
    // the side of the claim a name taken inside the tree falls on too, which is
    // the case this recovery exists for and the one no test can stage without a
    // second process.
    #[cfg(unix)]
    fn uncopyable_tree(at: &std::path::Path) -> std::os::unix::net::UnixListener {
        std::fs::create_dir(at).unwrap();
        std::fs::write(at.join("page.txt"), "new").unwrap();
        std::os::unix::net::UnixListener::bind(at.join("ipc")).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_destination_the_write_never_took_is_left_alone() {
        // The case that makes reading `AlreadyExists` as "this is mine" so
        // expensive: the name was taken before the copy reached for it, and
        // removing what is there destroys a directory this application never
        // created, with whatever the process that did create it had written.
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("mine.txt"), "mine").unwrap();

        let theirs = dir.path().join("theirs");
        std::fs::create_dir(&theirs).unwrap();
        std::fs::write(theirs.join("theirs.txt"), "theirs").unwrap();

        let failure = ops::copy_path_no_replace(&source, &theirs).unwrap_err();
        drop_claimed(&failure, &theirs);

        assert_eq!(
            std::fs::read_to_string(theirs.join("theirs.txt")).unwrap(),
            "theirs"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_destination_the_write_did_take_is_cleared_away() {
        // And the half-written tree that *is* ours goes, because leaving it is
        // what blocks the rollback that puts the original back.
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let _socket = uncopyable_tree(&source);

        let dst = dir.path().join("dst");
        let failure = ops::copy_path_no_replace(&source, &dst).unwrap_err();
        assert!(dst.is_dir(), "the copy has to get past creating it");
        drop_claimed(&failure, &dst);

        assert!(!dst.exists());
    }

    #[cfg(unix)]
    #[test]
    fn an_overwrite_that_cannot_build_puts_the_original_back() {
        // The whole recovery end to end: the original is moved aside, the
        // replacement fails once the staging path is its own, the staging goes,
        // and the original comes back to the name the user was looking at —
        // with nothing of ours left beside it.
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report");
        let _socket = uncopyable_tree(&source);

        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let dst = work.join("report");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(dst.join("page.txt"), "original").unwrap();

        assert!(overwrite_apply(ClipMode::Copy, &source, &dst).is_err());

        assert_eq!(
            std::fs::read_to_string(dst.join("page.txt")).unwrap(),
            "original",
            "the original has to be back where it was"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&work)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name != "report")
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    #[test]
    fn a_cut_that_cannot_finish_puts_the_source_back() {
        // The source is *inside* the staging path by then: a cut moves it there
        // before the destination is taken. Deleting it — which is the right
        // thing for a copy's staging — would destroy the only copy there is.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("notes.txt");
        let staged = dir.path().join(".notes.txt.nohrs-new");
        std::fs::write(&staged, "payload").unwrap();

        restore_staged(&src, &staged);

        assert_eq!(std::fs::read_to_string(&src).unwrap(), "payload");
        assert!(!staged.exists());
    }

    #[test]
    fn a_source_re_created_under_us_does_not_cost_the_staged_one() {
        // Something else taking the source's name back is not a reason to throw
        // away what was moved out of it. The no-replace move refuses, and the
        // only copy stays where it can still be found.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("notes.txt");
        std::fs::write(&src, "someone else's").unwrap();
        let staged = dir.path().join(".notes.txt.nohrs-new");
        std::fs::write(&staged, "payload").unwrap();

        restore_staged(&src, &staged);

        assert_eq!(std::fs::read_to_string(&staged).unwrap(), "payload");
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "someone else's");
    }

    #[test]
    fn staging_the_source_outlived_is_dropped() {
        // Every way `apply_one` can fail leaves the source where it was, so what
        // is at the staging path is a fragment and goes.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("notes.txt");
        std::fs::write(&src, "payload").unwrap();
        let staged = dir.path().join(".notes.txt.nohrs-new");
        std::fs::write(&staged, "half a copy").unwrap();

        drop_staged(&staged);

        assert!(!staged.exists());
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "payload");
    }
}

#[cfg(test)]
mod rename_selection_tests {
    use super::rename_selection_end;

    #[test]
    fn selects_the_name_without_its_extension() {
        assert_eq!(rename_selection_end("notes.txt", false), 5);
        assert_eq!(rename_selection_end("Cargo.toml", false), 5);
    }

    #[test]
    fn keeps_only_the_final_extension_out_of_the_selection() {
        // "archive.tar" stays selected; typing replaces it and ".gz" survives.
        assert_eq!(rename_selection_end("archive.tar.gz", false), 11);
    }

    #[test]
    fn selects_extensionless_names_whole() {
        assert_eq!(rename_selection_end("Makefile", false), 8);
    }

    #[test]
    fn selects_dotfiles_whole() {
        // `file_stem` treats a leading dot as part of the name; keying off the
        // last `.` instead would select nothing here.
        assert_eq!(rename_selection_end(".gitignore", false), 10);
    }

    #[test]
    fn selects_directories_whole_even_with_dots() {
        assert_eq!(rename_selection_end("my.folder", true), 9);
    }

    #[test]
    fn counts_utf16_units_not_bytes() {
        // The input's text API indexes in UTF-16, so a multi-byte stem must not
        // be measured in bytes (12 here) or chars alone.
        assert_eq!(rename_selection_end("日本語.txt", false), 3);
        // Astral-plane characters take two UTF-16 units each.
        assert_eq!(rename_selection_end("🎉🎉.txt", false), 4);
    }
}
