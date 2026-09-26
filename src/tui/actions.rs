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

use crate::diff::text::{RangeMatch, RenderOptions};
use crate::review::ReviewTarget;
use crate::tui::theme::OverlayTheme;

/// One entry in a directory listing shown by the file dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DirEntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Everything the diff viewer needs to display a completed diff. Holds the file contents, not
/// just paths, so the UI thread never does a blocking read after the background diff finishes.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffSessionData {
    pub before_path: PathBuf,
    pub after_path: PathBuf,
    pub before_contents: String,
    pub after_contents: String,
    pub before_ranges: Vec<RangeMatch>,
    pub after_ranges: Vec<RangeMatch>,
    /// Whether every real change touches only comment nodes. Computed while the AST still
    /// exists; it cannot be recovered from the ranges alone.
    pub comment_only: bool,
    /// Either side has no grammar, so the ranges come from `plain_text_line_diff`. Only the
    /// footer's `[plain text]` label reads it; everything else works on ranges from either source.
    pub plain_text_fallback: bool,
}

/// The result of one background diff computation - the payload of `Action::DiffComputed`.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffOutcome {
    Ready(DiffSessionData),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    /// Ctrl-Z. Deferred rather than handled where raised, because suspending needs `&mut UI` to
    /// release and re-acquire the terminal.
    Suspend,
    /// A recoverable failure, shown in the one-line `App::last_error` banner.
    Error(String),
    /// A directory listing for the file dialog finished loading.
    DirectoryListed(PathBuf, Vec<DirEntryInfo>),
    /// The user confirmed a file selection in the file dialog.
    FileSelected(PathBuf),
    /// The user cancelled the file dialog.
    DialogCancelled,
    /// A file picked in the git review picker. `index` is its position in the change set, so
    /// `]`/`[` know where to step from.
    ReviewFileSelected {
        target: ReviewTarget,
        index: usize,
    },
    /// Start the background diff of these two files.
    StartDiff(PathBuf, PathBuf),
    /// A background diff result, tagged with the generation `App::start_diff` captured. A stale
    /// result is dropped, which is how Esc cancels a diff whose thread cannot be killed. A fresh
    /// one is re-dispatched as `DiffReady`/`DiffFailed`, which components consume.
    DiffComputed {
        generation: u64,
        outcome: DiffOutcome,
    },
    DiffReady(DiffSessionData),
    DiffFailed(String),
    /// Enter in the theme dialog: apply and persist.
    ThemeSelected(OverlayTheme),
    /// The theme dialog's selection moved: apply to the viewer as a preview, without persisting.
    ThemePreviewed(OverlayTheme),
    /// Live preview of a syntax theme; like `ThemePreviewed`, persisted only when the dialog is
    /// accepted (by the dialog itself, since no action carries the value that far).
    SyntaxThemePreviewed(String),
    /// Jump the focused panel's cursor to the nearest match and highlight every match.
    SearchSubmitted(String),
    /// Recompute the live match count and preview highlights, without moving the cursor.
    SearchQueryChanged(String),
    /// A 1-indexed line number, already parsed by the prompt.
    JumpToLineSubmitted(usize),
    /// A render option changed: applied and persisted at once, so the diff behind the panel shows
    /// its effect. Closing the panel keeps it; nothing is restored.
    RenderOptionsChanged(RenderOptions),
    /// Enter or Esc in the render-options panel. Carries nothing: `RenderOptionsChanged` has
    /// already applied and persisted every change.
    RenderOptionsAccepted,
}
