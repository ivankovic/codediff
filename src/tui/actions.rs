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

use strum::Display;

use crate::diff::DiffMode;
use crate::diff::text::RangeMatch;
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
}

#[derive(Debug, Clone, PartialEq, Display)]
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
    /// Both before/after files are known; kick off the (background) diff computation.
    StartDiff(PathBuf, PathBuf),
    /// The background diff computation finished successfully.
    DiffReady(DiffSessionData),
    /// The background diff computation failed.
    DiffFailed(String),
    /// The user picked a color theme in the theme dialog.
    ThemeSelected(OverlayTheme),
    /// The background diff computation's phase-1-5 residual was too large for `DiffMode::Fast`
    /// to auto-resolve silently (`PendingDiff::looks_expensive()` was true) - prompts the user to
    /// pick a `DiffMode`. The counts are just enough context to render "this diff looks big"; the
    /// actual answer travels back out-of-band via `App::pending_diff_mode_tx`, not on this
    /// action, since a `oneshot::Sender` isn't `Debug`/`PartialEq`/`Clone`.
    DiffModeChoiceNeeded {
        unmatched_before: usize,
        unmatched_after: usize,
    },
    /// The user answered the `SelectDiffMode` prompt.
    DiffModeSelected(DiffMode),
}
