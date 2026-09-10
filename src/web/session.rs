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
//! The server-side half of the TUI's `App`: the open pair, the persisted settings, and the last
//! computed diff. Synchronous and socket-free on purpose - `server.rs` holds one of these behind
//! a mutex and calls into it between the network and the diff engine, so everything here is
//! tested the way `tui::app::App` is, with no server running.
//!
//! What is *not* here is any view state. Cursor, scroll, focus, search and open dialogs live in
//! the browser (`assets/web/model.js`), the way they live in `DiffViewer` for the TUI; the server
//! never learns where the cursor is. The one exception is `e`, which needs a path and a line.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::diff::text::RenderOptions;
use crate::review::{self, Review, ReviewTarget};
use crate::tui::actions::DiffSessionData;
use crate::tui::components::diff_viewer::SINGLE_PANEL_THRESHOLD;
use crate::tui::components::help_modal::HELP_TEXT;
use crate::tui::theme::{self, CustomPalette, OverlayTheme, PanelLayout};
use crate::tui::widgets::code_viewer::syntax_theme_names;
use crate::web::payload::{DiffPayload, HighlightPayload, diff_payload, highlight_payload};

/// The last successful diff, unfiltered - `DiffViewer::full_ranges` - so a render-options change
/// that only re-filters (every field but the two construction-time ones) needs no new diff.
#[derive(Debug, Clone)]
struct Loaded {
    data: DiffSessionData,
    large_residual: bool,
}

/// One diff to compute, handed to whoever owns a blocking thread. `generation` is what makes
/// cancellation work without killing anything: the session only accepts the result whose
/// generation is still current (`App::start_diff` has the same counter), so an abandoned or
/// superseded computation's answer is dropped when it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffJob {
    pub generation: u64,
    pub before: PathBuf,
    pub after: PathBuf,
    pub options: RenderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishOutcome {
    Stored(Box<DiffPayload>),
    /// The diff failed; the message is what the page's error banner shows.
    Failed(String),
    /// Superseded (or cancelled) while it ran - nothing to show, nothing changed.
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOptionsOutcome {
    /// A construction-time option changed and a pair is loaded: the diff has to be recomputed
    /// before the new options mean anything (`App::apply_render_options`'s `needs_reload`).
    Recompute(DiffJob),
    /// Re-filtered in place; `None` when no pair is loaded yet.
    Filtered(Option<Box<DiffPayload>>),
}

/// One overlay theme for the page's picker, its palette already resolved to hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemePayload {
    /// serde's name for the variant - what `SettingsUpdate::theme` accepts back.
    pub id: String,
    /// strum's label - what the picker shows.
    pub label: String,
    pub palette: CustomPalette,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsPayload {
    pub theme: OverlayTheme,
    pub layout: PanelLayout,
    pub node_highlight: bool,
    pub render_options: RenderOptions,
    pub syntax_theme: Option<String>,
    pub custom_palette: CustomPalette,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderOptionRow {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Presets {
    pub minimal: RenderOptions,
    pub full: RenderOptions,
}

/// Everything the page needs to draw its first frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatePayload {
    pub version: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub settings: SettingsPayload,
    pub themes: Vec<ThemePayload>,
    pub syntax_themes: Vec<String>,
    pub render_option_rows: Vec<RenderOptionRow>,
    /// `RenderOptions::MINIMAL`/`FULL`, for the `M` panel's `1`/`2` keys and the footer badge -
    /// sent rather than duplicated in JavaScript, so a preset can only mean one thing.
    pub presets: Presets,
    pub recent_pairs: Vec<[String; 2]>,
    pub help_text: &'static str,
    pub config_error: Option<String>,
    /// `DiffViewer`'s dual/single cut-over, in character cells, so the page's auto layout flips
    /// at the same width the terminal's does.
    pub single_panel_threshold: u16,
    /// `--review`: the page opens the git review picker as soon as it loads.
    pub review_on_start: bool,
}

/// A settings change from the page. Every field optional: the page sends the one that changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SettingsUpdate {
    pub theme: Option<OverlayTheme>,
    pub layout: Option<PanelLayout>,
    pub node_highlight: Option<bool>,
    pub syntax_theme: Option<String>,
    pub custom_palette: Option<CustomPalette>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirEntryPayload {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListingPayload {
    pub dir: String,
    pub entries: Vec<DirEntryPayload>,
}

/// The serde field names of `RenderOptions`, in declaration order, paired with the labels the
/// TUI's `M` panel shows. Pinned by a test against the struct's real serialization so a renamed
/// or reordered field cannot leave the page toggling the wrong row.
pub fn render_option_rows() -> Vec<RenderOptionRow> {
    const KEYS: [&str; 6] = [
        "leading_whitespace",
        "structural_punctuation",
        "whole_pair_updates",
        "paint_reindent_only_moves",
        "paint_displaced_moves",
        "paint_resized_moves",
    ];
    KEYS.iter()
        .zip(RenderOptions::FULL.options())
        .map(|(key, (label, _))| RenderOptionRow { key, label })
        .collect()
}

pub struct Session {
    before: Option<PathBuf>,
    after: Option<PathBuf>,
    render_options: RenderOptions,
    theme: OverlayTheme,
    layout: PanelLayout,
    node_highlight: bool,
    syntax_theme: Option<String>,
    recent_pairs: Vec<(PathBuf, PathBuf)>,
    loaded: Option<Loaded>,
    generation: u64,
    config_error: Option<String>,
    review_on_start: bool,
    /// Materialized blobs of reviewed changes, created on the first review open, removed when
    /// the session (and so the server) goes away.
    review_workspace: Option<review::Workspace>,
}

impl Session {
    /// Loads the persisted settings the way `App::run` does, including installing the custom
    /// palette so `OverlayTheme::Custom` resolves. `render_options` overrides the persisted ones
    /// when the command line asked for a preset (`--minimal`/`--full`).
    pub fn from_config(render_options: Option<RenderOptions>) -> Self {
        theme::set_custom_palette(theme::load_custom_palette());
        Self {
            before: None,
            after: None,
            render_options: render_options.unwrap_or_else(theme::load_render_options),
            theme: theme::load_overlay_theme(),
            layout: theme::load_panel_layout(),
            node_highlight: theme::load_node_highlight(),
            syntax_theme: theme::load_syntax_theme(),
            recent_pairs: theme::load_recent_pairs(),
            loaded: None,
            generation: 0,
            config_error: theme::config_error().map(|problem| {
                format!("config not loaded, using defaults and leaving the file alone - {problem}")
            }),
            review_on_start: false,
            review_workspace: None,
        }
    }

    /// `--review`: remembered for the state payload; the page does the opening.
    pub fn set_review_on_start(&mut self) {
        self.review_on_start = true;
    }

    /// The repository around the server's working directory - what the page's `G` lists.
    pub fn load_review(commit_limit: usize) -> anyhow::Result<Review> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        review::load(&cwd, commit_limit)
    }

    /// Materializes `target`'s two sides under this session's workspace and starts their diff,
    /// exactly as `App::open_review_position` does for the TUI.
    pub fn open_review_target(
        &mut self,
        root: &Path,
        target: &ReviewTarget,
    ) -> anyhow::Result<DiffJob> {
        if self.review_workspace.is_none() {
            self.review_workspace = Some(review::Workspace::new()?);
        }
        let workspace = self.review_workspace.as_ref().expect("created just above");
        let (before, after) = workspace.materialize(root, target)?;
        Ok(self.begin_diff(before, after))
    }

    /// The pair the command line named. Only remembered: the page asks for the diff itself once
    /// it has loaded, so a slow diff shows its "Diffing…" screen rather than a blank tab.
    pub fn set_pair(&mut self, before: PathBuf, after: PathBuf) {
        self.before = Some(before);
        self.after = Some(after);
    }

    pub fn render_options(&self) -> RenderOptions {
        self.render_options
    }

    pub fn path_for(&self, panel: &str) -> Option<&Path> {
        match panel {
            "before" => self.before.as_deref(),
            "after" => self.after.as_deref(),
            _ => None,
        }
    }

    pub fn state(&self) -> StatePayload {
        StatePayload {
            version: env!("CARGO_PKG_VERSION"),
            before: self.before.as_ref().map(|p| p.display().to_string()),
            after: self.after.as_ref().map(|p| p.display().to_string()),
            settings: SettingsPayload {
                theme: self.theme,
                layout: self.layout,
                node_highlight: self.node_highlight,
                render_options: self.render_options,
                syntax_theme: self.syntax_theme.clone(),
                custom_palette: theme::custom_palette(),
            },
            themes: OverlayTheme::iter()
                .map(|theme| ThemePayload {
                    id: serde_json::to_value(theme)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                        .unwrap_or_default(),
                    label: theme.to_string(),
                    palette: CustomPalette::from_palette(&theme.palette()),
                })
                .collect(),
            syntax_themes: syntax_theme_names(),
            render_option_rows: render_option_rows(),
            presets: Presets {
                minimal: RenderOptions::MINIMAL,
                full: RenderOptions::FULL,
            },
            recent_pairs: self
                .recent_pairs
                .iter()
                .map(|(before, after)| [before.display().to_string(), after.display().to_string()])
                .collect(),
            help_text: HELP_TEXT,
            config_error: self.config_error.clone(),
            single_panel_threshold: SINGLE_PANEL_THRESHOLD,
            review_on_start: self.review_on_start,
        }
    }

    /// Records the pair and hands back the job to run. Bumping the generation is what retires
    /// any computation still in flight for the previous pair.
    pub fn begin_diff(&mut self, before: PathBuf, after: PathBuf) -> DiffJob {
        self.before = Some(before.clone());
        self.after = Some(after.clone());
        self.generation += 1;
        DiffJob {
            generation: self.generation,
            before,
            after,
            options: self.render_options,
        }
    }

    /// Retires whatever is in flight without starting anything - the page's Esc during
    /// "Diffing…". The previous result stays loaded, exactly as in the TUI.
    pub fn cancel_diff(&mut self) {
        self.generation += 1;
    }

    /// Accepts a finished computation if it is still the current one.
    pub fn finish_diff(
        &mut self,
        generation: u64,
        outcome: Result<(DiffSessionData, bool), String>,
    ) -> FinishOutcome {
        if generation != self.generation {
            return FinishOutcome::Stale;
        }
        match outcome {
            Ok((data, large_residual)) => {
                theme::record_recent_pair(&data.before_path, &data.after_path);
                let pair = (data.before_path.clone(), data.after_path.clone());
                self.recent_pairs.retain(|existing| existing != &pair);
                self.recent_pairs.insert(0, pair);
                let loaded = Loaded {
                    data,
                    large_residual,
                };
                let payload = self.payload_for(&loaded);
                self.loaded = Some(loaded);
                FinishOutcome::Stored(Box::new(payload))
            }
            Err(message) => FinishOutcome::Failed(message),
        }
    }

    fn payload_for(&self, loaded: &Loaded) -> DiffPayload {
        diff_payload(
            &loaded.data,
            loaded.large_residual,
            self.render_options,
            self.syntax_theme.as_deref(),
        )
    }

    /// `App::apply_render_options`: persist, then either re-filter the loaded diff or ask for a
    /// new one when a construction-time option flipped.
    pub fn set_render_options(&mut self, options: RenderOptions) -> RenderOptionsOutcome {
        let previous = self.render_options;
        self.render_options = options;
        theme::save_render_options(options);
        let needs_recompute = previous.whole_pair_updates != options.whole_pair_updates
            || previous.paint_reindent_only_moves != options.paint_reindent_only_moves;
        if needs_recompute
            && let (Some(before), Some(after)) = (self.before.clone(), self.after.clone())
        {
            return RenderOptionsOutcome::Recompute(self.begin_diff(before, after));
        }
        RenderOptionsOutcome::Filtered(
            self.loaded
                .as_ref()
                .map(|loaded| Box::new(self.payload_for(loaded))),
        )
    }

    /// The loaded pair re-highlighted in `syntax_theme`, for the theme dialog's live preview.
    /// Not persisted - accepting the dialog does that through [`Self::apply_settings`].
    pub fn highlight(&self, syntax_theme: &str) -> Option<HighlightPayload> {
        self.loaded
            .as_ref()
            .map(|loaded| highlight_payload(&loaded.data, syntax_theme))
    }

    /// Persists each setting the page sent, the same `theme::save_*` calls the TUI's dialogs
    /// make. A custom palette is installed process-wide as well, so `OverlayTheme::Custom`
    /// resolves to it from then on.
    pub fn apply_settings(&mut self, update: SettingsUpdate) {
        if let Some(palette) = update.custom_palette {
            theme::set_custom_palette(palette.clone());
            theme::save_custom_palette(palette);
        }
        if let Some(theme) = update.theme {
            self.theme = theme;
            theme::save_overlay_theme(theme);
        }
        if let Some(layout) = update.layout {
            self.layout = layout;
            theme::save_panel_layout(layout);
        }
        if let Some(enabled) = update.node_highlight {
            self.node_highlight = enabled;
            theme::save_node_highlight(enabled);
        }
        if let Some(name) = update.syntax_theme {
            theme::save_syntax_theme(&name);
            self.syntax_theme = Some(name);
        }
    }
}

/// `FileDialog::request_listing`, synchronously: directories first, then case-insensitive by
/// name, `..` prepended whenever there is a parent. An unreadable directory lists as empty rather
/// than failing, as in the TUI, so the user can still back out of it.
pub fn list_directory(dir: &Path) -> ListingPayload {
    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let is_dir = entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false);
            entries.push(DirEntryPayload {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path().display().to_string(),
                is_dir,
            });
        }
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    if let Some(parent) = dir.parent() {
        entries.insert(
            0,
            DirEntryPayload {
                name: "..".to_string(),
                path: parent.display().to_string(),
                is_dir: true,
            },
        );
    }
    ListingPayload {
        dir: dir.display().to_string(),
        entries,
    }
}

/// `App::run_editor` without the terminal handover: `$VISUAL`, else `$EDITOR`, else `vi`, on
/// `path` at `line`, inheriting this process's stdio - so a terminal editor opens in the terminal
/// `codediff-web` was started from, and a GUI one opens wherever it opens. Blocks until the
/// editor exits; the caller re-diffs afterwards.
pub fn run_editor(path: &Path, line: usize) -> Result<(), String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    std::process::Command::new(&editor)
        .arg(format!("+{line}"))
        .arg(path)
        .status()
        .map(|_| ())
        .map_err(|err| format!("failed to launch editor '{editor}': {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::text::{RangeMatch, TextOperation};
    use crate::diff::text_range::TextRange;

    fn session() -> Session {
        Session::from_config(None)
    }

    fn sample_data(before: &str, after: &str) -> DiffSessionData {
        DiffSessionData {
            before_path: PathBuf::from("/tmp/x/before.rs"),
            after_path: PathBuf::from("/tmp/x/after.rs"),
            before_contents: before.to_string(),
            after_contents: after.to_string(),
            before_ranges: vec![RangeMatch {
                source: TextRange::new(0, 0, 0, 1),
                destination: TextRange::new(0, 0, 0, 0),
                operation: TextOperation::Delete,
            }],
            after_ranges: Vec::new(),
            comment_only: false,
            plain_text_fallback: false,
        }
    }

    #[test]
    fn render_option_rows_match_the_structs_own_serialization_order() {
        // The JSON *text*, not a `Value`: serde_json's object map is sorted, and what the page
        // toggles by index is the derive's declaration order, which only the text preserves.
        let text = serde_json::to_string(&RenderOptions::FULL).unwrap();
        let serialized: Vec<&str> = text.split('"').skip(1).step_by(2).collect();
        let rows: Vec<&str> = render_option_rows().iter().map(|row| row.key).collect();
        assert_eq!(rows, serialized);
        assert!(render_option_rows()[1].label.starts_with("Structural"));
    }

    #[test]
    fn the_state_names_every_theme_with_a_round_trippable_id() {
        let state = session().state();
        assert_eq!(state.themes.len(), OverlayTheme::iter().count());
        for theme in &state.themes {
            let parsed: OverlayTheme =
                serde_json::from_value(serde_json::Value::String(theme.id.clone()))
                    .unwrap_or_else(|_| panic!("{} should parse back", theme.id));
            assert_eq!(parsed.to_string(), theme.label);
        }
        assert!(
            state
                .syntax_themes
                .iter()
                .any(|name| name == "base16-ocean.dark")
        );
        assert_eq!(state.single_panel_threshold, SINGLE_PANEL_THRESHOLD);
        assert!(state.help_text.contains("Navigation"));
    }

    #[test]
    fn a_stale_result_is_dropped_and_a_current_one_stored() {
        let mut session = session();
        let first = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        let second = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        assert_eq!(second.generation, first.generation + 1);
        assert_eq!(
            session.finish_diff(first.generation, Ok((sample_data("a\n", "\n"), false))),
            FinishOutcome::Stale
        );
        assert!(session.loaded.is_none());
        match session.finish_diff(second.generation, Ok((sample_data("a\n", "\n"), true))) {
            FinishOutcome::Stored(payload) => {
                assert!(payload.large_residual);
                assert_eq!(payload.before.lines, vec!["a"]);
            }
            other => panic!("expected Stored, got {other:?}"),
        }
        assert_eq!(session.recent_pairs.len(), 1);
    }

    #[test]
    fn cancelling_retires_the_in_flight_generation_but_keeps_the_last_result() {
        let mut session = session();
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        session.finish_diff(job.generation, Ok((sample_data("a\n", "\n"), false)));
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        session.cancel_diff();
        assert_eq!(
            session.finish_diff(job.generation, Ok((sample_data("zzz\n", "\n"), false))),
            FinishOutcome::Stale
        );
        assert_eq!(session.loaded.as_ref().unwrap().data.before_contents, "a\n");
    }

    #[test]
    fn a_failed_diff_reports_its_message() {
        let mut session = session();
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        assert_eq!(
            session.finish_diff(job.generation, Err("boom".into())),
            FinishOutcome::Failed("boom".into())
        );
    }

    #[test]
    fn a_filter_only_option_change_refilters_without_a_new_diff() {
        let mut session = session();
        session.set_render_options(RenderOptions::FULL);
        assert_eq!(
            session.set_render_options(RenderOptions {
                leading_whitespace: false,
                ..RenderOptions::FULL
            }),
            RenderOptionsOutcome::Filtered(None),
            "no pair loaded yet: nothing to refilter"
        );
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        session.finish_diff(job.generation, Ok((sample_data("a\n", "\n"), false)));
        match session.set_render_options(RenderOptions {
            structural_punctuation: false,
            ..RenderOptions::FULL
        }) {
            RenderOptionsOutcome::Filtered(Some(payload)) => {
                assert!(!payload.render_options.structural_punctuation);
            }
            other => panic!("expected a refiltered payload, got {other:?}"),
        }
        assert_eq!(
            session.generation, job.generation,
            "no new computation was started"
        );
    }

    #[test]
    fn a_construction_time_option_change_asks_for_a_recompute() {
        let mut session = session();
        session.set_render_options(RenderOptions::FULL);
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        session.finish_diff(job.generation, Ok((sample_data("a\n", "\n"), false)));
        let flipped = RenderOptions {
            whole_pair_updates: true,
            ..RenderOptions::FULL
        };
        match session.set_render_options(flipped) {
            RenderOptionsOutcome::Recompute(new_job) => {
                assert_eq!(new_job.generation, job.generation + 1);
                assert_eq!(new_job.options, flipped);
            }
            other => panic!("expected Recompute, got {other:?}"),
        }
        assert_eq!(theme::load_render_options(), flipped, "persisted as well");
    }

    #[test]
    fn settings_persist_through_the_same_config_the_tui_reads() {
        let mut session = session();
        session.apply_settings(SettingsUpdate {
            theme: Some(OverlayTheme::Nord),
            layout: Some(PanelLayout::Single),
            node_highlight: Some(true),
            syntax_theme: Some("Solarized (light)".into()),
            custom_palette: None,
        });
        assert_eq!(theme::load_overlay_theme(), OverlayTheme::Nord);
        assert_eq!(theme::load_panel_layout(), PanelLayout::Single);
        assert!(theme::load_node_highlight());
        assert_eq!(
            theme::load_syntax_theme().as_deref(),
            Some("Solarized (light)")
        );
        let state = session.state();
        assert_eq!(state.settings.theme, OverlayTheme::Nord);
        assert_eq!(
            state.settings.syntax_theme.as_deref(),
            Some("Solarized (light)")
        );
    }

    #[test]
    fn a_custom_palette_is_installed_as_well_as_saved() {
        let mut session = session();
        let palette = CustomPalette {
            insert_bg: "#123456".into(),
            ..CustomPalette::default()
        };
        session.apply_settings(SettingsUpdate {
            custom_palette: Some(palette.clone()),
            ..SettingsUpdate::default()
        });
        assert_eq!(theme::custom_palette().insert_bg, "#123456");
        assert_eq!(theme::load_custom_palette().insert_bg, "#123456");
        let custom = session
            .state()
            .themes
            .into_iter()
            .find(|theme| theme.label == "Custom")
            .unwrap();
        assert_eq!(
            custom.palette.insert_bg, "#123456",
            "the picker shows the installed palette"
        );
    }

    #[test]
    fn highlight_is_none_until_a_pair_is_loaded() {
        let mut session = session();
        assert!(session.highlight("Solarized (light)").is_none());
        let job = session.begin_diff("/tmp/a".into(), "/tmp/b".into());
        session.finish_diff(job.generation, Ok((sample_data("a\n", "\n"), false)));
        let payload = session.highlight("Solarized (light)").unwrap();
        assert_eq!(payload.syntax.theme, "Solarized (light)");
        assert_eq!(payload.before.len(), 1);
    }

    #[test]
    fn listing_puts_directories_first_and_the_parent_on_top() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        std::fs::write(dir.path().join("A.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("zed")).unwrap();
        let listing = list_directory(dir.path());
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "zed", "A.txt", "b.txt"]);
        assert_eq!(
            listing.entries[0].path,
            dir.path().parent().unwrap().display().to_string()
        );
    }

    #[test]
    fn listing_an_unreadable_directory_still_offers_the_way_out() {
        let listing = list_directory(Path::new("/definitely/not/a/dir"));
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "..");
    }

    #[test]
    fn a_reviewed_file_is_materialized_and_diffed_like_any_pair() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args([
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@example.com",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(root.join("a.rs"), "fn a() { 1 }\n").unwrap();
        let review = review::load(root, 5).unwrap();
        let target = ReviewTarget {
            set: review::ChangeSet::WorkingTree,
            file: review.working_tree[0].clone(),
        };
        let mut session = session();
        assert!(!session.state().review_on_start);
        session.set_review_on_start();
        assert!(session.state().review_on_start);
        let job = session.open_review_target(&review.root, &target).unwrap();
        assert!(job.before.starts_with(std::env::temp_dir()));
        assert!(job.after.ends_with("a.rs"));
        assert_eq!(
            session.state().after.as_deref(),
            Some(job.after.to_str().unwrap())
        );
    }

    #[test]
    fn path_for_names_the_two_panels_and_nothing_else() {
        let mut session = session();
        assert!(session.path_for("before").is_none());
        session.set_pair("/tmp/a".into(), "/tmp/b".into());
        assert_eq!(session.path_for("before"), Some(Path::new("/tmp/a")));
        assert_eq!(session.path_for("after"), Some(Path::new("/tmp/b")));
        assert!(session.path_for("sideways").is_none());
        let state = session.state();
        assert_eq!(state.before.as_deref(), Some("/tmp/a"));
    }
}
