/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::path::PathBuf;

use crate::diff::DiffMode;
use crate::diff::text::RangeMatch;
use crate::tui::components::diff_viewer::Panel;
use crate::tui::theme::OverlayTheme;

/// One entry in a directory listing shown by the file dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DirEntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Everything the diff viewer needs to display a completed before/after diff.
///
/// Holds the already-read file contents (not just paths) so the UI thread never has to do a
/// blocking filesystem read after the background diff computation completes.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffSessionData {
    pub before_path: PathBuf,
    pub after_path: PathBuf,
    pub before_contents: String,
    pub after_contents: String,
    pub before_ranges: Vec<RangeMatch>,
    pub after_ranges: Vec<RangeMatch>,
    /// Whether every real change between before/after touches only comment nodes (see
    /// `diff::text::is_comment_only_diff`). Computed once, while the AST is still available
    /// (`tui::app::assemble_diff_session_data`) - by the time `DiffSessionData` exists, the AST
    /// itself is gone, so this can't be recomputed later from `before_ranges`/`after_ranges` alone
    /// the way `diff::text::summarize_diff`'s other cases can.
    pub comment_only: bool,
    /// Which `DiffMode` actually produced this result - `Fast` unless the user re-ran the pair
    /// through full analysis with the `x` key (or `--exact` in batch mode). Meaningless when
    /// `plain_text_fallback` is set (no AST algorithm ran at all), same as `comment_only` in that
    /// case. Shown in `app.rs`'s footer so a user can't forget which one they're looking at.
    pub mode: DiffMode,
    /// Set when either side has no tree-sitter grammar (e.g. an extension-less `Makefile`), so
    /// `before_ranges`/`after_ranges` came from `diff::text::plain_text_line_diff` (a plain
    /// line-level Myers diff) instead of the AST-aware pipeline - see `app::compute_diff`'s
    /// branch. Everything downstream (overlay rendering, `headless`, `json_output`,
    /// `change_counts`, `DiffSummary`) already works on a plain `RangeMatch` list regardless of
    /// where it came from; this field exists purely so `app.rs`'s footer can show `[plain text]`
    /// instead of a `DiffMode` label that would otherwise misleadingly imply a structural diff ran.
    pub plain_text_fallback: bool,
}

/// The result of one background diff computation - the payload of `Action::DiffComputed`.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffOutcome {
    Ready {
        data: DiffSessionData,
        /// Whether `DiffMode::Fast`'s guard silently substituted the cheaper fallback for the
        /// expensive final pass (see `compute_diff`) - surfaced in the footer as a hint that `x`
        /// (re-run exact) is worth pressing.
        fallback_used: bool,
    },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    /// A recoverable, non-fatal failure the user should be told about (e.g. a failed frame draw)
    /// - surfaced via `App::last_error`, the same one-line banner `Action::DiffFailed` uses.
    Error(String),
    /// A directory listing for the file dialog finished loading.
    DirectoryListed(PathBuf, Vec<DirEntryInfo>),
    /// The user confirmed a file selection in the file dialog.
    FileSelected(PathBuf),
    /// The user cancelled the file dialog.
    DialogCancelled,
    /// Both before/after files are known; kick off the (background) diff computation in the given
    /// mode (`Fast` for every ordinary file pick; `Exact` when the `x` key re-runs the current
    /// pair through full analysis).
    StartDiff(PathBuf, PathBuf, DiffMode),
    /// A background diff computation returned - success or failure - tagged with the generation
    /// counter `App::start_diff` captured when it launched. `App` compares it against the current
    /// generation and simply drops a stale result: this is what makes Esc-during-Diffing a real
    /// cancel (the `spawn_blocking` thread itself can't be killed, but its answer can be
    /// discarded). A fresh result is re-dispatched as `DiffReady`/`DiffFailed` below, which is
    /// what components actually consume.
    DiffComputed {
        generation: u64,
        outcome: DiffOutcome,
    },
    /// The background diff computation finished successfully.
    DiffReady(DiffSessionData),
    /// The background diff computation failed.
    DiffFailed(String),
    /// The user picked a color theme in the theme dialog (Enter) - apply it and persist it.
    ThemeSelected(OverlayTheme),
    /// The theme dialog's selection moved (Up/Down) - apply the highlighted theme to the live
    /// viewer behind the dialog as a preview, without persisting anything. Esc reverts to the
    /// last persisted choice; Enter (`ThemeSelected`) makes it stick.
    ThemePreviewed(OverlayTheme),
    /// The user confirmed a query in the search modal (the `/` key) - jump the focused panel's
    /// cursor to the nearest match and highlight every match.
    SearchSubmitted(String),
    /// The search modal's query changed while typing - `App` recomputes the focused panel's
    /// match count for the modal's live `N matches` readout and previews the highlights, without
    /// moving the cursor (that only happens on `SearchSubmitted`).
    SearchQueryChanged(String),
    /// The user confirmed a line number in the jump-to-line prompt (the `g` key) - 1-indexed,
    /// already parsed by the prompt itself.
    JumpToLineSubmitted(usize),
    /// `n`/`p` was pressed but the focused panel has no navigable changes while the *other*
    /// panel does (e.g. the "before" side of a diff that's pure insertions - there is nothing on
    /// that side to ever jump to). Dispatched by `DiffViewer::handle_key_event` instead of
    /// performing the otherwise-silent no-op, so `App` can offer to switch panels instead.
    /// `empty_panel` is the panel with no changes, for the dialog's message; `forward` is which
    /// direction (`n`/`p`) triggered this, replayed on confirmation.
    NoChangesPromptNeeded {
        forward: bool,
        empty_panel: Panel,
    },
    /// The user confirmed the `NoChangesPrompt` dialog (Enter) - switch to the other panel and
    /// jump `forward`/backward on it, same as a normal `n`/`p` jump once there.
    NoChangesPromptConfirmed {
        forward: bool,
    },
}
