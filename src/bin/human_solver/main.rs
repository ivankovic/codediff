/*  This file is part of the OmniDiff code diffing tool.
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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

//! Builds the ground-truth AST mappings the fixture tests grade omnidiff against.
//!
//! Run as `cargo run --bin human_solver -- <name>`, where `<name>` is a fixture directory under
//! `src/test/data/diffs/<dataset>/` (e.g. "rust-add-if"); without it the first case alphabetically
//! opens. It shows both ASTs side by side, lets a human mark nodes matched, deleted or inserted and
//! paint the text-range ground truth, and saves the fixture's `human_mapping.json` plus its
//! `src/test/fixtures/<dataset>/<module>.rs` stub. Nodes are addressed by path (kind + sibling
//! position), not node id, since ids are not stable across parses (`test::helper::path_for_node`).
//!
//! Keybindings: press `?`, or read `HELP_TEXT` below.
use std::io::{self, Stdout, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
};
use serde::{Deserialize, Serialize};

mod actions;
mod events;
mod flatten;
mod keylog;
mod navigate;
mod render;
mod state;
mod stubs;
use actions::*;
use events::*;
use flatten::*;
use keylog::*;
use navigate::*;
use render::*;
use state::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use stubs::*;
use tree_sitter::Node;

use omnidiff::code::language::{language_for_path, to_treesitter};
use omnidiff::code::{Code, Language};
use omnidiff::diff::text::TextDiff;
use omnidiff::diff::{ASTDiff, ASTMappingReason, NodeCache, diff_code};
#[cfg(test)]
use omnidiff::test::helper::human_mapping::rebuild_caches;
use omnidiff::test::helper::human_mapping::{
    self, Caches, GroupPairing, HumanMapping, HumanMappingEntry, HumanOperation, HumanTextEntry,
    HumanTextMapping, HumanTextOperation, HumanTextSpan, HumanTextVerdict, MarkKind, MultiMapGroup,
    NamedTextMapping, NodeStatus, disagreement_is_move_only, is_inherited_removed, path_refs,
    rebuild_caches_for_mapping, status_after, status_before, text_mapping_disagreements,
};
use omnidiff::test::helper::{
    DIFF_DATASETS, code_pair_from_dir, code_pair_from_dir_without_metadata, diffs_case_dir,
    node_for_path, path_for_node, precompute_paths, read_note, write_note,
};
use omnidiff::tui::theme::{self, OverlayPalette, OverlayTheme};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Interactively build a human ground-truth AST mapping for a fixture"
)]
struct Args {
    /// A directory under `src/test/data/diffs/<dataset>/` (e.g. "rust-add-if"). If omitted, the
    /// case open when the tool last exited reopens (see `SessionMemory`), or else the first case
    /// alphabetically.
    name: Option<String>,
    /// Print where the logged keys and the time between them went (see `keylog`), and exit.
    #[arg(long)]
    key_log_summary: bool,
}

/// The `?` help popup (`Modal::Help`): a terse sheet that fits on one screen.
const HELP_TEXT: &str = "\
Tab            switch focus between Before/After panels
Up/k, Down/j   move cursor
Left/h         collapse current node, or go to its parent
Right/l        expand current node, or go to its first child
g / G          jump to first / last visible node

m / M          match cursor nodes (M also recurses into matching children);
                 with a pending multi-map selection (see x), commits it as a
                 group instead (M asserts the matched subtree closes over
                 itself, without auto-filling descendants the way plain M does;
                 for an all-to-all selection (see X), M instead walks the
                 selected subtrees in lockstep and commits one all-to-all group
                 per position, every node down to the leaves - the subtrees
                 must agree on kind and child count everywhere, or nothing is
                 committed and the first divergence is reported)
f              repeat m until end of file or a kind mismatch needs your input:
                 there, y matches anyway, d/D or i/I mark the stopping node
                 deleted or inserted (with subtree) and f carries on, n stops
d / D          mark Before node deleted / deleted with subtree
i / I          mark After node inserted / inserted with subtree
u              unmark the focused cursor node, or remove its whole multi-map
                 group if it's a group member
v              start a range at the cursor node (again: clear it); move, and
                 the next d/D/i/I/u acts on every visible node from there to
                 the cursor - a run of inserted statements is v, j j j, I.
                 Nodes already marked are skipped, and with D/I so is anything
                 under an earlier node of the range
U / Ctrl-r     undo / redo the last change to the mapping - a mark, a group, a
                 painting, a ! reset - here and in the t view alike; nothing is
                 written until s
x              toggle the focused cursor node in/out of a pending multi-map
                 selection -- select several nodes on each side, then m/M to
                 commit them as a group where any Before node may pair with
                 any After node (leftovers on the larger side become
                 delete/insert); mixed kinds ask for confirmation first
X              flip the pending selection to all-to-all and back: every
                 Before node corresponds to every After node and nothing is
                 left over (a statement split in two, two merged into one, a
                 body duplicated). Shown as G instead of g once committed
c              clear the pending multi-map selection
a / A          align other panel to the human mapping / to omnidiff's mapping
p              re-run omnidiff's own diff in the foreground; it runs by itself in
                 the background when a case opens, and its verdict then shows
                 next to each node (`*` where it disagrees with you) with the
                 count of those in each panel's header
r              toggle showing the ASTMappingReason (which pass matched it) next
                 to each node's algo verdict
n / N          jump to next / previous mismatch (`*`) vs. omnidiff's verdict
] / [          jump to the next / previous unmarked node in this panel (wraps)
} / {          open the next / previous case as the o picker lists it - its
                 sort and filters apply, so with Unmarked filtered to yes this
                 walks the unfinished cases; unsaved changes prompt first
/              search: jump to the next leaf node whose text contains the
                 given string (plain substring, no regex)

t              text view: read the source, and paint the human text-range ground
                 truth onto it (stored beside the tree mapping, not derived from
                 it). It opens with each side's cursor on the node that side's
                 panel is on, the focused panel's side first. Tab side, hjkl/g/G move, w/b/e by word and W/B/E by
                 blank-separated word as in vim, 0/^/$ to a line's start/first
                 code character/end, v select. On a terminal narrower than
                 220 columns only the focused side is drawn; Tab shows the other. By default a selection spanning
                 several rows sweeps full lines from the anchor to the cursor, one
                 contiguous multi-line block; V toggles that to vertical -- the same
                 columns on each row, like a stack of squares, not every full line
                 swept in between. d/i paint the
                 Before/After selection deleted/inserted, m pairs BOTH sides'
                 selections as a match: a move when every span on both sides reads
                 the same, otherwise an update as a whole - so banked spans of
                 different text, several spellings extracted into one helper, are
                 one match (paint an unchanged span as its own match if you want it
                 shown as a move), u removes the range under the cursor,
                 Z marks a nothing-to-paint fixture, : jumps to a line number,
                 / searches this side's text (plain substring; Enter jumps to the
                 next match, wrapping; the last query is offered again, so / Enter
                 repeats it), Esc unselects or closes.
                 In a painting named Minimal, d/i on a multi-row full-line (V)
                 sweep is recorded as one range per row starting at that row's
                 first code character, never through the indentation -- the rule
                 invariant 6 states, kept for you instead of reported afterwards.
                 m on a full-line sweep on each side does the same and pairs the
                 rows one to one (the same number of rows on both sides, or it is
                 refused), one match per row, each a move or an update by itself.
                 A blank row in the sweep drops out; a vertical selection and a
                 Full or free-named painting are left exactly as drawn.
                 Branching a Minimal painting to one named Full (s, Enter) does
                 the opposite on the way across: every line whose every visible
                 character is painted deleted, or every one inserted, is widened
                 to start at column 0, which is what invariant 4 requires of a
                 Full painting. Only those lines -- a partly changed line, a
                 matched (move/update) line, and a branch to any other name are
                 copied unchanged, and the Minimal painting itself never moves.
                 A range that overlaps one already painted is refused at the
                 keystroke (u removes the old one first): the renderer resolves
                 an overlap by highest verdict and the scorer by list order, so
                 such a painting would look like one thing and grade as another.
                 Two ranges meeting at a line break share only the newline and
                 are fine.
                 x banks a selection so another can be made on the same side, c
                 clears the banks; banked and live ranges commit together, which
                 is how an N:M match is made -- every span on ONE side must read
                 the same, so any pairing of them says the same thing.
                 A fixture may hold several named paintings, for edits with more
                 than one defensible rendering; a check passes on ANY of them.
                 s branches the current one to another name, keeping both on
                 file (Enter copies its ranges, e starts empty; Minimal / Full /
                 Only one solution offered, none required, free-form ok); L
                 switches which one you edit; D twice in either list deletes the
                 highlighted painting, which is how a fixture painted Only one
                 solution becomes a Minimal + Full pair.
                 n / p jump BOTH panels to the next / previous hunk of the plain
                 line diff (wrapping, read off the focused side's own row), and a
                 puts the other panel on the same line number as this one and
                 scrolls it to the same place on screen -- the two ways to stop
                 hand-scrolling two independently scrolled panels
                 A puts THIS side's AST panel on the leaf under the text cursor,
                 expanding whatever is collapsed over it and focusing that panel,
                 without closing this view -- so a row an invariant names becomes
                 the node whose entry has to change. A cursor in the whitespace
                 between two tokens lands on the next leaf, and says so
                 o cycles what is drawn: your painting, omnidiff's own rendering
                 of the same pair, or only the bytes where the two disagree
                 P copies omnidiff's rendering into the current painting as a
                 starting point, so a fixture is corrected rather than painted
                 from a blank page -- only into an empty painting, so it can
                 never overwrite work (s branches this one to a new name first),
                 and it declines outright on a pair whose omnidiff ranges
                 overlap or fail to pair up, rather than seeding a painting that
                 would render and score differently
T              view the output of unix `diff -u`, with before/after line numbers
                 (t/T switch between these two views while either is open)
H              toggle hiding fully solved subtrees (unmarked nodes and their
                 ancestors always stay visible)
V              list every ground-truth invariant this case breaks: the rule's
                 number, the painting it is about, what it says, and where --
                 each site's position, the text there, and the node it falls in.
                 j/k move, g/G ends, Enter puts both trees and both text panels
                 on it (opening the text view over them, one Esc from the trees),
                 Esc closes. Read from the mapping in memory, so it sees unsaved
                 edits; the o picker's Invariant column is the same count

s              save -- or, on a sample, prompt for a name (pre-filled with
                 <language>-<repository>) and promote it
R              on a sample, prompt for a reason and reject it instead of
                 promoting it (recorded in sample.csv; no human mapping needed)
e              on a diff, enter/edit its description.md (written on Enter, empty deletes
                 it; `*` in the o picker marks cases that have one); on a sample, a
                 free-form comment (recorded in sample.csv,
                 works regardless of status; carried into the generated test stub
                 if present when later promoted)
o              open a different test case (src/test/data/diffs/) as a table:
                 Name, Dataset, Cmpl, Unmarked, Paint, Disagree, Invariant, Size.
                 j/k pick a row, h/l pick a column, s sorts by that column (again to
                 reverse), f filters on it -- substring on Name, dataset cycle on
                 Dataset, off/yes/no on the rest. Filters AND across columns.
                 Invariant counts the ground-truth invariants this case's own
                 mapping breaks, the number its invariants() test asserts on;
                 Size is the diff's changed lines, as in the O picker.
                 The scans behind Cmpl/Unmarked, Paint, Disagree, Invariant and
                 Size run on the first s or f on that column (Cmpl/Unmarked blocks
                 for ~12s and Disagree ~7s on the full corpus the first time; their
                 results are cached under ~/.local/state/omnidiff/scans/ and only
                 cases changed since are rescanned); until then those columns read
                 ?, and a ? row survives either filter direction.
                 Cursor, sort and filters persist across o, and with the case
                 last open across runs (~/.local/state/omnidiff/human_solver.json):
                 started with no name, the tool reopens where it left off.
                 Every key is appended to human_solver_keys.tsv beside it (which
                 view took it, the key, whether it edited; typed text is not
                 recorded); `human_solver --key-log-summary` shows where the
                 keys and the time between them went
O              open a sampled candidate (src/test/data/samples/) as a table:
                 Name, Lang, Bucket, Status, Size. Same keys as o -- j/k pick a
                 row, h/l pick a column, s sorts by that column (again to
                 reverse), f filters on it: substring on Name, a value cycle on
                 Lang and Bucket, sampled/SOLVED/REJECTED on Status, and
                 off/nonempty/empty on Size (an empty diff is a broken draw).
                 Bucket is the LOC stratum the sample was drawn for, ? for one
                 sampled before strata were recorded, and a ? row survives the
                 Bucket filter either way. Filters AND together across columns.
                 Cursor, sort and filters persist across O
C              open a commit from this repo's own git log, then a file it
                 changed -- before/after are that file at the commit's parent
                 and at the commit itself; s promotes into handmade/

!              start this case from scratch: clears the tree mapping, its
                 multi-map groups and every named painting at once, after a
                 confirmation. Nothing is written until s, so reopening the case
                 without saving gets it back

?              toggle this help
q / Esc        quit
";

/// Where a freshly-opened panel puts its cursor: `code`'s root node id, or `usize::MAX` for a
/// text-only case. A real `Node::id` is an address, so the sentinel cannot collide; the flat node
/// list is empty in that mode, so the cursor sits nowhere.
pub(crate) fn starting_cursor_id(code: &Code) -> usize {
    code.ast
        .as_ref()
        .map(|tree| tree.root_node().id())
        .unwrap_or(usize::MAX)
}

/// Loads and parses the before/after code for the case `name`, in whichever `DIFF_DATASETS`
/// folder it lives. Parses only this case, since it runs on every `o`-picker open.
fn load_case(name: &str) -> Result<(Code, Code)> {
    let Some(dir) = diffs_case_dir(name) else {
        let available = list_available_cases().unwrap_or_default();
        bail!(
            "No test case named '{}' found in src/test/data/diffs.\nAvailable: {}",
            name,
            available
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    };

    let (mut before, mut after) = code_pair_from_dir(&dir)
        .with_context(|| format!("Failed to load test case from {:?}", dir))?
        .ok_or_else(|| {
            anyhow!(
                "Directory '{}' exists but is missing a before/after fixture",
                name
            )
        })?;

    // A language with no tree-sitter grammar opens in text-only mode (see
    // `FrameState::before_root`): the painting is what such a fixture records. `ensure_parsed`
    // errors on such a language, so it runs only when there is a tree. It fills
    // node_to_full_hash, which `m`/`M` classify inner nodes by.
    if before.ast.is_some() {
        before
            .ensure_parsed()
            .context("Failed to compute AST metadata for before code")?;
    }
    if after.ast.is_some() {
        after
            .ensure_parsed()
            .context("Failed to compute AST metadata for after code")?;
    }

    Ok((before, after))
}

fn diffs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs")
}

fn samples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("samples")
}

/// The `DIFF_DATASETS` folder the case `name` lives under, for display; `None` if it isn't a case.
fn case_dataset(name: &str) -> Option<String> {
    diffs_case_dir(name)?
        .parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().into_owned())
}

/// Sorted names of every directory directly under `root`, parseable or not. A missing `root` is
/// empty, not an error: `samples/` exists only after `materialize_test_diffs` runs.
fn list_dir_names(root: &Path) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut names: Vec<String> = fs::read_dir(root)
        .with_context(|| format!("reading {:?}", root))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();

    Ok(names)
}

/// Just the names from [`list_available_cases`], for `scan_corpus`.
fn list_available_case_names() -> Result<Vec<String>> {
    Ok(list_available_cases()?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

/// Every case across all `DIFF_DATASETS` folders with its dataset, sorted by name. Names are unique
/// across datasets (`action_promote`'s collision check spans them all).
fn list_available_cases() -> Result<Vec<(String, &'static str)>> {
    let mut names = Vec::new();
    for dataset in DIFF_DATASETS {
        for name in list_dir_names(&diffs_root().join(dataset))? {
            names.push((name, *dataset));
        }
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(names)
}

/// The case names the `o` picker shows, in order: `options` narrowed by every filter in `view`
/// (ANDed; an unknown value survives either direction, see `FlagFilter::keeps`), sorted by
/// `view.sort` with the name as an always-ascending tiebreak, so the order is total. Unknown values
/// sort after known ones ascending (see `sort_rank`).
fn visible_diff_options(
    options: &[(String, &'static str)],
    view: &DiffPickerView,
    data: DiffPickerData<'_>,
) -> Vec<String> {
    let filters = &view.filters;
    let mut visible: Vec<&str> = options
        .iter()
        .filter(|(_, dataset)| filters.dataset.is_none_or(|wanted| *dataset == wanted))
        .filter(|(name, _)| {
            filters
                .name
                .as_ref()
                .is_none_or(|needle| name.to_lowercase().contains(needle))
        })
        .filter(|(name, _)| {
            filters
                .cmpl
                .keeps(data.unmarked_of(name).map(|count| count > 0))
        })
        .filter(|(name, _)| {
            filters
                .unmarked
                .keeps(data.unmarked_of(name).map(|count| count > 0))
        })
        .filter(|(name, _)| filters.paint.keeps(data.painted_of(name)))
        .filter(|(name, _)| {
            filters
                .disagree
                .keeps(data.disagreement_of(name).map(|bytes| bytes > 0))
        })
        .filter(|(name, _)| {
            filters
                .invariant
                .keeps(data.invariants_of(name).map(|count| count > 0))
        })
        .filter(|(name, _)| {
            filters
                .size
                .keeps(data.size_of(name).map(|lines| lines > 0))
        })
        .map(|(name, _)| name.as_str())
        .collect();

    let dataset_of = |name: &str| -> &'static str {
        options
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, dataset)| *dataset)
            .unwrap_or("")
    };

    visible.sort_by(|a, b| {
        let primary = match view.sort.column {
            DiffColumn::Name => std::cmp::Ordering::Equal,
            DiffColumn::Dataset => dataset_of(a).cmp(dataset_of(b)),
            DiffColumn::Cmpl => bool_rank(data.unmarked_of(a).map(|count| count > 0))
                .cmp(&bool_rank(data.unmarked_of(b).map(|count| count > 0))),
            DiffColumn::Unmarked => {
                sort_rank(data.unmarked_of(a)).cmp(&sort_rank(data.unmarked_of(b)))
            }
            DiffColumn::Paint => bool_rank(data.painted_of(a)).cmp(&bool_rank(data.painted_of(b))),
            DiffColumn::Disagree => {
                sort_rank(data.disagreement_of(a)).cmp(&sort_rank(data.disagreement_of(b)))
            }
            DiffColumn::Invariant => {
                sort_rank(data.invariants_of(a)).cmp(&sort_rank(data.invariants_of(b)))
            }
            DiffColumn::Size => sort_rank(data.size_of(a)).cmp(&sort_rank(data.size_of(b))),
        };
        let primary = if view.sort.descending {
            primary.reverse()
        } else {
            primary
        };
        primary.then_with(|| a.cmp(b))
    });

    visible.into_iter().map(str::to_string).collect()
}

/// Sort key for a numeric column: the leading flag puts unknowns after every known value, rather
/// than a sentinel a real measurement could collide with.
fn sort_rank(value: Option<usize>) -> (bool, usize) {
    (value.is_none(), value.unwrap_or(0))
}

/// `sort_rank` for a yes/no column: `false`, then `true`, then unknown.
fn bool_rank(value: Option<bool>) -> (bool, bool) {
    (value.is_none(), value.unwrap_or(false))
}

/// Builds the `o` picker's modal with the selection on `current_name`, or on the first visible row
/// when the filters hide it. Separate from the key handlers so it is testable without files.
fn open_diff_picker_modal(
    options: Vec<(String, &'static str)>,
    current_name: &str,
    view: DiffPickerView,
    data: DiffPickerData<'_>,
) -> Modal {
    let visible = visible_diff_options(&options, &view, data);
    let selected = visible
        .iter()
        .position(|name| name == current_name)
        .unwrap_or(0)
        .min(visible.len().saturating_sub(1));
    Modal::OpenDiffPicker {
        options,
        selected,
        view,
        name_input: None,
    }
}

/// `DiffFilters` on disk: the dataset by name. Hand-written impls rather than serde's `from`/`into`,
/// which still derive over the `&'static str` field and demand the input outlive `'static`. A
/// name no longer in `DIFF_DATASETS` reads as "all" rather than failing the whole file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct DiffFiltersRecord {
    name: Option<String>,
    dataset: Option<String>,
    cmpl: FlagFilter,
    unmarked: FlagFilter,
    paint: FlagFilter,
    disagree: FlagFilter,
    invariant: FlagFilter,
    size: FlagFilter,
}

impl From<DiffFilters> for DiffFiltersRecord {
    fn from(filters: DiffFilters) -> Self {
        DiffFiltersRecord {
            name: filters.name,
            dataset: filters.dataset.map(str::to_string),
            cmpl: filters.cmpl,
            unmarked: filters.unmarked,
            paint: filters.paint,
            disagree: filters.disagree,
            invariant: filters.invariant,
            size: filters.size,
        }
    }
}

impl Serialize for DiffFilters {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DiffFiltersRecord::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiffFilters {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        DiffFiltersRecord::deserialize(deserializer).map(DiffFilters::from)
    }
}

impl From<DiffFiltersRecord> for DiffFilters {
    fn from(record: DiffFiltersRecord) -> Self {
        DiffFilters {
            name: record.name.filter(|name| !name.is_empty()),
            dataset: record
                .dataset
                .and_then(|name| DIFF_DATASETS.iter().copied().find(|d| *d == name)),
            cmpl: record.cmpl,
            unmarked: record.unmarked,
            paint: record.paint,
            disagree: record.disagree,
            invariant: record.invariant,
            size: record.size,
        }
    }
}

/// What the tool keeps between runs: where it was, and how the `o` picker was set up. Written
/// after every case switch and on quit (`remember_session`), read by `main` when no case is named.
/// Lives outside the repository, since it is one person's place, not the corpus's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct SessionMemory {
    /// The `src/test/data/diffs/` case last open; samples and commit files are not resumable
    /// by name, so they leave this as it was.
    last_case: Option<String>,
    diff_view: DiffPickerView,
}

/// `$XDG_STATE_HOME/omnidiff/human_solver.json`, or `~/.local/state/...` without the variable;
/// `None` with neither set.
fn session_memory_path() -> Option<PathBuf> {
    let state_home = match std::env::var_os("XDG_STATE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".local/state"),
    };
    Some(state_home.join("omnidiff").join("human_solver.json"))
}

/// The memory at `path`, or the default when the file is missing or unreadable: a corrupt file
/// costs the resume, never the session.
fn load_session_memory(path: &Path) -> SessionMemory {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_session_memory(path: &Path, memory: &SessionMemory) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(memory)?)?;
    Ok(())
}

/// Writes the session memory for `app`'s current state; a failure lands on the status line, so
/// a run that could not remember itself says so instead of silently opening elsewhere next time.
fn remember_session(app: &mut App) {
    let Some(path) = session_memory_path() else {
        return;
    };
    let mut memory = load_session_memory(&path);
    if matches!(app.origin, CaseOrigin::Diffs) {
        memory.last_case = Some(app.name.clone());
    }
    memory.diff_view = app.diff_view.clone();
    if let Err(err) = save_session_memory(&path, &memory) {
        app.status = Some(format!("Could not write {}: {err:#}", path.display()));
    }
}

/// The case to open when the command line names none: `remembered` if it still exists, else the
/// first case alphabetically.
fn starting_case(remembered: Option<&str>) -> Result<String> {
    if let Some(name) = remembered
        && diffs_case_dir(name).is_some()
    {
        return Ok(name.to_string());
    }
    list_available_cases()?
        .into_iter()
        .next()
        .map(|(name, _)| name)
        .ok_or_else(|| anyhow!("No test cases found in src/test/data/diffs"))
}

/// `}` / `{`: the case after or before the open one in the `o` picker's current listing, wrapping.
/// `Ok(None)` when there is nowhere to go (the status line says why) or when unsaved changes
/// raised `ConfirmDiscardUnsaved`, which returns the target itself once answered. Runs the scans
/// the view's sort and filters read, as the picker's `s`/`f` would have.
fn neighbouring_case(app: &mut App, forward: bool) -> Result<Option<OpenTarget>> {
    let options = list_available_cases()?;
    let view = app.diff_view.clone();
    ensure_diff_column_data(app, view.sort.column);
    for column in view.filters.active_columns() {
        ensure_diff_column_data(app, column);
    }
    let visible = visible_diff_options(&options, &view, DiffPickerData::from_app(app));
    if visible.is_empty() {
        bail!("No case passes the o picker's filters");
    }
    let target = match visible.iter().position(|name| *name == app.name) {
        Some(index) if forward => &visible[(index + 1) % visible.len()],
        Some(index) => &visible[(index + visible.len() - 1) % visible.len()],
        // The open case is outside the listing (filtered out, or not a diffs case): start at
        // its edge.
        None if forward => &visible[0],
        None => &visible[visible.len() - 1],
    };
    if *target == app.name {
        app.status = Some("This is the only case in the o picker's listing".to_string());
        return Ok(None);
    }
    let target = OpenTarget::Diffs(target.clone());
    if app.dirty {
        let can_save = matches!(app.origin, CaseOrigin::Diffs);
        app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
        return Ok(None);
    }
    Ok(Some(target))
}

/// One corpus scan's results on disk, so the `o` picker's `s`/`f` and `}`/`{` do not redo
/// seconds of work every session for fixtures that have not changed. Per case, the stamp its value
/// was computed under and the value - `None` for a case the scan had no answer for, so that is
/// remembered too. Keyed on the binary as well: a scan's meaning can change with the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ScanCache<T> {
    binary: u64,
    cases: HashMap<String, (u64, Option<T>)>,
}

// By hand: the derive would demand `T: Default`, which a scan's value need not be.
impl<T> Default for ScanCache<T> {
    fn default() -> Self {
        ScanCache {
            binary: 0,
            cases: HashMap::new(),
        }
    }
}

/// `<session memory dir>/scans/<kind>.json`.
fn scan_cache_path(kind: &str) -> Option<PathBuf> {
    Some(
        session_memory_path()?
            .parent()?
            .join("scans")
            .join(format!("{kind}.json")),
    )
}

fn mtime_secs(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs(),
    )
}

/// The newest modification time among a case's files: its mapping, sources and note. Any edit
/// to the case moves it, which is all the cache needs.
fn case_stamp(name: &str) -> Option<u64> {
    let dir = diffs_case_dir(name)?;
    fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|entry| mtime_secs(&entry.path()))
        .max()
}

fn binary_stamp() -> u64 {
    std::env::current_exe()
        .ok()
        .and_then(|exe| mtime_secs(&exe))
        .unwrap_or(0)
}

/// [`scan_corpus`] through the on-disk cache: only cases whose stamp moved since they were last
/// scanned, or that were never scanned, run `scan`; the rest come from the cache, which is then
/// rewritten. A case with no stamp (its directory is gone) is scanned and not cached.
fn scan_corpus_cached<T, F>(kind: &str, names: &[String], scan: F) -> HashMap<String, T>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send,
    F: Fn(&str) -> Option<T> + Sync,
{
    scan_corpus_cached_with(scan_cache_path(kind).as_deref(), names, case_stamp, scan)
}

/// `scan_corpus_cached` with the cache file and the stamp function as parameters, for tests.
fn scan_corpus_cached_with<T, F, S>(
    cache_path: Option<&Path>,
    names: &[String],
    stamp_of: S,
    scan: F,
) -> HashMap<String, T>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send,
    F: Fn(&str) -> Option<T> + Sync,
    S: Fn(&str) -> Option<u64>,
{
    let binary = binary_stamp();
    let mut cache: ScanCache<T> = cache_path
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .filter(|cache: &ScanCache<T>| cache.binary == binary)
        .unwrap_or_default();
    cache.binary = binary;

    let stamps: Vec<(String, Option<u64>)> = names
        .iter()
        .map(|name| (name.clone(), stamp_of(name)))
        .collect();
    let stale: Vec<String> = stamps
        .iter()
        .filter(|(name, stamp)| {
            stamp.is_none_or(|stamp| {
                cache
                    .cases
                    .get(name)
                    .is_none_or(|(cached_stamp, _)| *cached_stamp != stamp)
            })
        })
        .map(|(name, _)| name.clone())
        .collect();
    let fresh = scan_corpus(&stale, scan);

    let mut result = HashMap::new();
    for (name, stamp) in &stamps {
        if stale.contains(name) {
            if let Some(value) = fresh.get(name) {
                result.insert(name.clone(), value.clone());
            }
            if let Some(stamp) = stamp {
                cache
                    .cases
                    .insert(name.clone(), (*stamp, fresh.get(name).cloned()));
            }
        } else if let Some((_, Some(value))) = cache.cases.get(name) {
            result.insert(name.clone(), value.clone());
        }
    }
    // Cases no longer in the corpus drop out of the cache.
    cache.cases.retain(|name, _| names.contains(name));

    if let Some(path) = cache_path
        && !stale.is_empty()
    {
        // Best effort: a cache that could not be written costs the next session a scan.
        let written = path
            .parent()
            .map(fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|_| fs::write(path, serde_json::to_string(&cache).unwrap_or_default()));
        let _ = written;
    }
    result
}

/// The `o` picker's next dataset filter: `DIFF_DATASETS` in order, then back to "all" (`None`).
fn next_dataset_filter(current: Option<&'static str>) -> Option<&'static str> {
    match current {
        None => Some(DIFF_DATASETS[0]),
        Some(current) => DIFF_DATASETS
            .iter()
            .position(|d| *d == current)
            .and_then(|i| DIFF_DATASETS.get(i + 1))
            .copied(),
    }
}

/// Runs `scan` over `names` on several threads, collecting the `Some` results into a map.
///
/// `scan` must not go through `handmade_test_code_pair`'s process-wide cache: it never evicts, so
/// filling it from N threads multiplies peak memory. The scan bodies here read and parse per call.
///
/// A worker panic propagates: a partial map would show the lost cases as `?`, indistinguishable
/// from "not scanned yet".
fn scan_corpus<T, F>(names: &[String], scan: F) -> std::collections::HashMap<String, T>
where
    T: Send,
    F: Fn(&str) -> Option<T> + Sync,
{
    scan_corpus_with_threads(names, default_scan_threads(), scan)
}

fn default_scan_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_SCAN_THREADS)
}

/// Ceiling on `scan_corpus`'s workers. Each holds one fixture's parsed trees (plus a diff for the
/// disagreement scan), so peak memory scales with this and nothing else bounds it; past the core
/// count more workers add memory and no speed.
const MAX_SCAN_THREADS: usize = 8;

/// `scan_corpus` with the worker count pinned, so tests can compare parallel against serial.
fn scan_corpus_with_threads<T, F>(
    names: &[String],
    threads: usize,
    scan: F,
) -> std::collections::HashMap<String, T>
where
    T: Send,
    F: Fn(&str) -> Option<T> + Sync,
{
    if threads <= 1 || names.len() <= 1 {
        return names
            .iter()
            .filter_map(|name| scan(name).map(|value| (name.clone(), value)))
            .collect();
    }

    // A shared cursor rather than a slice per thread: fixture sizes differ by orders of magnitude,
    // so an even split by count leaves workers idle behind whichever drew the giants.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let scan = &scan;
    let next = &next;
    let chunks: Vec<Vec<(String, T)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads.min(names.len()))
            .map(|_| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(name) = names.get(index) else {
                            break;
                        };
                        if let Some(value) = scan(name) {
                            found.push((name.clone(), value));
                        }
                    }
                    found
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| match handle.join() {
                Ok(found) => found,
                Err(payload) => std::panic::resume_unwind(payload),
            })
            .collect()
    });

    chunks.into_iter().flatten().collect()
}

/// Runs, once per session, the corpus scan `column` reads. Called from `s`/`f` only, never from
/// `h`/`l`: the scans take seconds, and cursor movement must not stall.
fn ensure_diff_column_data(app: &mut App, column: DiffColumn) {
    match column {
        DiffColumn::Cmpl | DiffColumn::Unmarked => {
            if app.diff_unmarked.is_none() {
                app.diff_unmarked = Some(compute_diff_unmarked());
            }
        }
        DiffColumn::Paint => {
            if app.diff_text_painted.is_none() {
                app.diff_text_painted = Some(compute_diff_text_painted());
            }
        }
        DiffColumn::Disagree => {
            if app.diff_disagreement.is_none() {
                app.diff_disagreement = Some(compute_diff_disagreement());
            }
        }
        DiffColumn::Invariant => {
            if app.diff_invariants.is_none() {
                app.diff_invariants = Some(compute_diff_invariants());
            }
        }
        DiffColumn::Size => {
            if app.diff_sizes.is_none() {
                app.diff_sizes = Some(compute_diff_sizes());
            }
        }
        DiffColumn::Name | DiffColumn::Dataset => {}
    }
}

/// How many nodes in `root`'s subtree are `NodeStatus::Unmarked` - the whole-tree counterpart of
/// `count_unmarked`, which counts only visible rows.
fn count_unmarked_nodes_in_tree(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if status_fn(node, caches) == NodeStatus::Unmarked {
            count += 1;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

/// How many nodes of both trees `name`'s human mapping leaves `NodeStatus::Unmarked`. `None` only
/// when the case's source can't be loaded, which the picker shows as `?`, never as "0 left". No
/// `human_mapping.json` yet counts every node; a text-only pair counts 0.
fn diff_case_unmarked_count(name: &str) -> Option<usize> {
    let dir = diffs_case_dir(name)?;
    // Without metadata: the count never diffs, and metadata is most of the load.
    let (before, after) = code_pair_from_dir_without_metadata(&dir).ok().flatten()?;
    let mapping = human_mapping::load_with(name, &before, &after).unwrap_or_default();
    let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return Some(0);
    };
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
    Some(
        count_unmarked_nodes_in_tree(before_root, &caches, status_before)
            + count_unmarked_nodes_in_tree(after_root, &caches, status_after),
    )
}

/// Refreshes `name`'s entry in `App::diff_unmarked`, if that scan has run. Called after a save.
fn refresh_diff_unmarked(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_unmarked
        && let Some(count) = diff_case_unmarked_count(name)
    {
        map.insert(name.to_string(), count);
    }
}

/// Builds `App::diff_unmarked` (the `Cmpl` and `Unmarked` columns) for the whole corpus. A case
/// that fails to load is absent rather than failing the scan.
fn compute_diff_unmarked() -> std::collections::HashMap<String, usize> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus_cached("unmarked", &names, diff_case_unmarked_count)
}

/// Whether `name`'s human mapping carries any text painting; `None` if the file can't be read.
///
/// A substring search rather than a JSON parse: parsing the whole corpus's mapping files costs as
/// much as the expensive scans. The quotes make the token unambiguous, since `serde_json` escapes
/// any quote inside a string value, so it can only match a key.
///
/// Keyed on presence, not emptiness: `Z` records a nothing-to-paint fixture as an empty painting,
/// which counts as painted.
fn diff_case_has_text_mapping(name: &str) -> Option<bool> {
    let path = human_mapping::mapping_path(name);
    let contents = std::fs::read_to_string(path).ok()?;
    Some(contents.contains("\"text_mappings\""))
}

/// Refreshes `name`'s entry in `App::diff_text_painted`, if that scan has run. Called after a save.
fn refresh_diff_text_painted(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_text_painted
        && let Some(painted) = diff_case_has_text_mapping(name)
    {
        map.insert(name.to_string(), painted);
    }
}

/// Builds `App::diff_text_painted` (the `Paint` column) for the whole corpus.
fn compute_diff_text_painted() -> std::collections::HashMap<String, bool> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    // Unlike the other scans, an unreadable case is recorded (as unpainted) rather than absent.
    scan_corpus(&names, |name| {
        Some(diff_case_has_text_mapping(name).unwrap_or(false))
    })
}

/// How many bytes `name`'s human tree mapping and human text painting disagree about (omnidiff's
/// own matching plays no part), excluding `disagreement_is_move_only` runs. `None` when the case
/// can't be loaded or has no painting yet, as distinct from agreeing exactly (`Some(0)`).
fn diff_case_disagreement_bytes(name: &str) -> Option<usize> {
    let dir = diffs_case_dir(name)?;
    let (before, after) = code_pair_from_dir(&dir).ok().flatten()?;
    let mapping = human_mapping::load_with(name, &before, &after).ok()?;
    let check = text_mapping_disagreements(&mapping, &before, &after)
        .ok()
        .flatten()?;
    Some(
        check
            .disagreements
            .iter()
            .filter(|d| !disagreement_is_move_only(d))
            .map(|d| d.end_byte - d.start_byte)
            .sum(),
    )
}

/// Refreshes `name`'s entry in `App::diff_disagreement`, if that scan has run. Called after a save.
fn refresh_diff_disagreement(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_disagreement {
        match diff_case_disagreement_bytes(name) {
            Some(bytes) => {
                map.insert(name.to_string(), bytes);
            }
            None => {
                map.remove(name);
            }
        }
    }
}

/// Builds `App::diff_disagreement` (the `Disagree` column) for every painted case.
fn compute_diff_disagreement() -> std::collections::HashMap<String, usize> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus_cached("disagreement", &names, diff_case_disagreement_bytes)
}

/// How many ground-truth invariants `name`'s human mapping breaks - the number its fixture's
/// `invariants()` test asserts on. `None`, not `Some(0)`, without a mapping: an unannotated case
/// has not satisfied the invariants.
fn diff_case_invariant_violations(name: &str) -> Option<usize> {
    human_mapping::invariants::ground_truth_invariant_violations(name)
        .ok()
        .map(|violations| violations.len())
}

/// Builds `App::diff_invariants` (the `Invariant` column) for the whole corpus.
fn compute_diff_invariants() -> std::collections::HashMap<String, usize> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus_cached("invariants", &names, diff_case_invariant_violations)
}

/// Refreshes `name`'s entry in `App::diff_invariants`, if that scan has run. Called after a save.
fn refresh_diff_invariants(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_invariants {
        match diff_case_invariant_violations(name) {
            Some(count) => {
                map.insert(name.to_string(), count);
            }
            None => {
                map.remove(name);
            }
        }
    }
}

/// Every case's note (`description.md`) by case name; cases without one are absent.
fn compute_diff_comments() -> std::collections::HashMap<String, String> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&names, read_note)
}

/// Refreshes `name`'s entry in `App::diff_comments`, if loaded. Called after `e`.
fn refresh_diff_comment(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_comments {
        match read_note(name) {
            Some(note) => {
                map.insert(name.to_string(), note);
            }
            None => {
                map.remove(name);
            }
        }
    }
}

/// A sample's disposition from its sample.csv `status` column; `Sampled` when undecided, including
/// a row without that column or no row at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleTriageStatus {
    Sampled,
    Promoted,
    Rejected,
}

impl SampleTriageStatus {
    fn label(self) -> &'static str {
        match self {
            SampleTriageStatus::Sampled => "sampled",
            SampleTriageStatus::Promoted => "SOLVED",
            SampleTriageStatus::Rejected => "REJECTED",
        }
    }

    /// Sort order for the `Status` column: untriaged first.
    fn order(self) -> u8 {
        match self {
            SampleTriageStatus::Sampled => 0,
            SampleTriageStatus::Promoted => 1,
            SampleTriageStatus::Rejected => 2,
        }
    }
}

/// Every sample under src/test/data/samples/ as an `O` picker row: language, repository, commit
/// and path from its `source.json`, status and LOC bucket from the sample.csv row it joins to (the
/// join `action_promote`/`action_reject` use). `size` is 0 here; the `O` handler fills it from the
/// cached `App::sample_diff_sizes`.
fn list_sample_rows() -> Result<Vec<SampleRow>> {
    let names = list_dir_names(&samples_root())?;
    let meta = sample_metadata()?;

    Ok(names
        .into_iter()
        .map(|name| {
            let source = source_json_for_sample(&name);
            let language = source
                .as_ref()
                .map(|source| source.language.clone())
                .unwrap_or_else(|| "?".to_string());
            let found = source.and_then(|source| {
                meta.get(&(
                    source.language,
                    source.repository,
                    source.commit,
                    source.path,
                ))
                .cloned()
            });
            SampleRow {
                name,
                language,
                bucket: found.as_ref().and_then(|meta| meta.size_bucket.clone()),
                status: found
                    .map(|meta| meta.status)
                    .unwrap_or(SampleTriageStatus::Sampled),
                size: 0,
            }
        })
        .collect())
}

/// A sample's `source.json` provenance, without parsing its code (unlike `load_sample`).
fn source_json_for_sample(name: &str) -> Option<SampleSource> {
    let contents = fs::read_to_string(samples_root().join(name).join("source.json")).ok()?;
    serde_json::from_str(&contents).ok()
}

/// What the `O` picker reads off a sample.csv row besides its key.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SampleMeta {
    status: SampleTriageStatus,
    /// `None` for a row written before bucket tracking, or sampled without `--stratified`.
    size_bucket: Option<String>,
}

fn sample_metadata()
-> Result<std::collections::HashMap<(String, String, String, String), SampleMeta>> {
    sample_metadata_at(&sample_csv_path())
}

/// Every sample.csv row's status and bucket, keyed by (language, repository, commit, path). Empty,
/// not an error, if `path` doesn't exist.
fn sample_metadata_at(
    path: &Path,
) -> Result<std::collections::HashMap<(String, String, String, String), SampleMeta>> {
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }

    Ok(read_sample_csv_rows(path)?
        .into_iter()
        .map(|row| {
            let status = match row.status.as_str() {
                "PROMOTED" => SampleTriageStatus::Promoted,
                "REJECTED" => SampleTriageStatus::Rejected,
                _ => SampleTriageStatus::Sampled,
            };
            let size_bucket = (!row.size_bucket.is_empty()).then_some(row.size_bucket);
            (
                (row.language, row.repository, row.commit, row.path),
                SampleMeta {
                    status,
                    size_bucket,
                },
            )
        })
        .collect())
}

/// A sample's `source.json`, written by `materialize_test_diffs`: the exact `sample.csv` row it
/// came from, since the directory name (lowercased, 8-char commit) is lossy.
#[derive(Debug, Clone, Deserialize)]
struct SampleSource {
    language: String,
    repository: String,
    commit: String,
    path: String,
    /// The research dataset (`sample_test_diffs --dataset`) this sample came from.
    #[serde(default = "legacy_dataset")]
    dataset: String,
}

/// The dataset of a `source.json` without one: such samples really came from the small checkout,
/// so this is a true value, not a placeholder.
fn legacy_dataset() -> String {
    "small".to_string()
}

/// The promote-name prompt's pre-fill: `<language>-<repository>`, lowercased, `.git` stripped -
/// the same prefix `materialize_test_diffs`'s `base_name` builds. The descriptive suffix is left
/// to the human.
fn default_promoted_name(source: &SampleSource) -> String {
    let language = source.language.to_lowercase();
    let repository = source
        .repository
        .strip_suffix(".git")
        .unwrap_or(&source.repository)
        .to_lowercase();
    format!("{language}-{repository}")
}

/// Loads and parses the before/after code for a sampled candidate under
/// `src/test/data/samples/<name>/`, along with its recorded provenance.
fn load_sample(name: &str) -> Result<(Code, Code, SampleSource)> {
    let dir = samples_root().join(name);

    let (mut before, mut after) = code_pair_from_dir(&dir)
        .with_context(|| format!("Failed to load sample from {:?}", dir))?
        .ok_or_else(|| anyhow!("No before/after fixture found in samples/{}", name))?;

    if before.ast.is_none() {
        bail!(
            "Before code for sample '{}' has no AST (unsupported or undetected language)",
            name
        );
    }
    if after.ast.is_none() {
        bail!(
            "After code for sample '{}' has no AST (unsupported or undetected language)",
            name
        );
    }
    before
        .ensure_parsed()
        .context("Failed to compute AST metadata for before code")?;
    after
        .ensure_parsed()
        .context("Failed to compute AST metadata for after code")?;

    let source_path = dir.join("source.json");
    let contents =
        fs::read_to_string(&source_path).with_context(|| format!("reading {:?}", source_path))?;
    let source: SampleSource =
        serde_json::from_str(&contents).with_context(|| format!("parsing {:?}", source_path))?;

    Ok((before, after, source))
}

/// The system `diff -u` of `before_src` against `after_src`, through temp files so it reflects
/// what is loaded whatever the case's origin.
fn run_unix_diff(before_src: &[u8], after_src: &[u8]) -> Result<String> {
    let mut before_file =
        tempfile::NamedTempFile::new().context("creating temp file for before content")?;
    before_file
        .write_all(before_src)
        .context("writing before content to temp file")?;
    let mut after_file =
        tempfile::NamedTempFile::new().context("creating temp file for after content")?;
    after_file
        .write_all(after_src)
        .context("writing after content to temp file")?;

    let output = std::process::Command::new("diff")
        .arg("-u")
        .arg("--label")
        .arg("before")
        .arg("--label")
        .arg("after")
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .context("running unix `diff` (is it installed?)")?;

    match output.status.code() {
        Some(0) => Ok("(no textual differences)".to_string()),
        Some(1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => bail!("diff failed: {}", String::from_utf8_lossy(&output.stderr)),
    }
}

/// The raw `before.<ext>.test`/`after.<ext>.test` contents of `dir`, unparsed. `None` if either
/// is missing or unreadable.
fn raw_before_after(dir: &Path) -> Option<(String, String)> {
    let mut before = None;
    let mut after = None;

    for entry in fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with("before.") && name.ends_with(".test") {
            before = fs::read_to_string(&path).ok();
        } else if name.starts_with("after.") && name.ends_with(".test") {
            after = fs::read_to_string(&path).ok();
        }
    }

    Some((before?, after?))
}

/// A sample's `changed_line_count`, the `O` picker's size sort key; 0 when it can't be measured.
fn sample_diff_line_count(name: &str) -> usize {
    changed_line_count(&samples_root().join(name)).unwrap_or(0)
}

/// Changed lines in the `diff -u` of the pair in `dir`, the size measure both pickers share. `None`
/// when the pair can't be read or `diff` can't run.
fn changed_line_count(dir: &Path) -> Option<usize> {
    let (before, after) = raw_before_after(dir)?;
    let diff = run_unix_diff(before.as_bytes(), after.as_bytes()).ok()?;
    Some(count_changed_lines(&diff))
}

/// `+`/`-` lines of a unified diff, excluding its `+++`/`---` header.
fn count_changed_lines(diff: &str) -> usize {
    diff.lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .count()
}

/// Which side a unified diff lies entirely on: `After` if it only adds lines, `Before` if it only
/// removes them, `None` for both or neither (headers excluded).
///
/// `f` uses this to resolve a kind mismatch without asking: in an add-only change the before file
/// survives intact, so the mismatching node must be an insertion on the after side (and mirrored).
fn one_sided_diff(diff: &str) -> Option<Side> {
    let mut adds = false;
    let mut removes = false;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            adds = true;
        } else if line.starts_with('-') {
            removes = true;
        }
    }
    match (adds, removes) {
        (true, false) => Some(Side::After),
        (false, true) => Some(Side::Before),
        _ => None,
    }
}

/// Changed lines in `name`'s unified diff (the `Size` column); `None`, shown as `?`, when unreadable.
fn diff_case_size(name: &str) -> Option<usize> {
    changed_line_count(&diffs_case_dir(name)?)
}

/// Builds `App::diff_sizes` (the `Size` column) for the whole corpus.
fn compute_diff_sizes() -> std::collections::HashMap<String, usize> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus_cached("sizes", &names, diff_case_size)
}

/// One column of the `o` picker's table; `h`/`l` move between them, `s`/`f` act on the current one.
///
/// `Cmpl` and `Unmarked` read one number (`App::diff_unmarked`) and filter identically; both exist
/// because `Cmpl` sorts the corpus into two halves while `Unmarked` orders it by work left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum DiffColumn {
    #[default]
    Name,
    Dataset,
    Cmpl,
    Unmarked,
    Paint,
    Disagree,
    Invariant,
    /// Changed lines of the case's `diff -u`, the same measure as the `O` picker's `Size`.
    Size,
}

impl DiffColumn {
    /// Left-to-right order, shared by the header, cursor movement and `render_open_diff_picker`.
    const ALL: [DiffColumn; 8] = [
        DiffColumn::Name,
        DiffColumn::Dataset,
        DiffColumn::Cmpl,
        DiffColumn::Unmarked,
        DiffColumn::Paint,
        DiffColumn::Disagree,
        DiffColumn::Invariant,
        DiffColumn::Size,
    ];

    fn index(self) -> usize {
        DiffColumn::ALL
            .iter()
            .position(|column| *column == self)
            .unwrap_or(0)
    }

    /// Clamped rather than wrapping: a jump from one edge of the header to the other reads as a
    /// glitch.
    fn left(self) -> Self {
        DiffColumn::ALL[self.index().saturating_sub(1)]
    }

    fn right(self) -> Self {
        DiffColumn::ALL[(self.index() + 1).min(DiffColumn::ALL.len() - 1)]
    }

    fn header(self) -> &'static str {
        match self {
            DiffColumn::Name => "Name",
            DiffColumn::Dataset => "Dataset",
            DiffColumn::Cmpl => "Cmpl",
            DiffColumn::Unmarked => "Unmarked",
            DiffColumn::Paint => "Paint",
            DiffColumn::Disagree => "Disagree",
            DiffColumn::Invariant => "Invariant",
            DiffColumn::Size => "Size",
        }
    }

    /// The (`Yes`, `No`) labels of this column's `FlagFilter`; `None` for `Name` and `Dataset`.
    fn flag_labels(self) -> Option<(&'static str, &'static str)> {
        match self {
            DiffColumn::Cmpl => Some(("incomplete only", "complete only")),
            DiffColumn::Unmarked => Some(("has unmarked", "none unmarked")),
            DiffColumn::Paint => Some(("painted only", "unpainted only")),
            DiffColumn::Disagree => Some(("disagreements only", "agreeing only")),
            DiffColumn::Invariant => Some(("breaks invariants", "invariants hold")),
            // An empty diff is a broken fixture; "No" finds them.
            DiffColumn::Size => Some(("has changed lines", "empty diffs only")),
            DiffColumn::Name | DiffColumn::Dataset => None,
        }
    }
}

/// The `f` state of one yes/no column. Cycles `Off -> Yes -> No -> Off`.
///
/// A row whose value is unknown survives either direction: unknown means "not scanned yet" or
/// "failed to load", and the picker exists to surface fixtures that need attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum FlagFilter {
    #[default]
    Off,
    Yes,
    No,
}

impl FlagFilter {
    fn next(self) -> Self {
        match self {
            FlagFilter::Off => FlagFilter::Yes,
            FlagFilter::Yes => FlagFilter::No,
            FlagFilter::No => FlagFilter::Off,
        }
    }

    /// `value` is this column's yes/no reading for one row, `None` when it isn't known.
    fn keeps(self, value: Option<bool>) -> bool {
        match (self, value) {
            (FlagFilter::Off, _) | (_, None) => true,
            (FlagFilter::Yes, Some(value)) => value,
            (FlagFilter::No, Some(value)) => !value,
        }
    }
}

/// Every column's filter; a row shows only if it passes all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiffFilters {
    /// Case-insensitive name substring, stored lowercased. Never `Some("")`, which would read as
    /// on while filtering nothing.
    name: Option<String>,
    /// Which of `DIFF_DATASETS` to show; `None` is all.
    dataset: Option<&'static str>,
    cmpl: FlagFilter,
    unmarked: FlagFilter,
    paint: FlagFilter,
    disagree: FlagFilter,
    invariant: FlagFilter,
    size: FlagFilter,
}

impl DiffFilters {
    /// The columns whose flag filter is on: the scans a listing under these filters depends on.
    fn active_columns(&self) -> Vec<DiffColumn> {
        [
            (DiffColumn::Cmpl, self.cmpl),
            (DiffColumn::Unmarked, self.unmarked),
            (DiffColumn::Paint, self.paint),
            (DiffColumn::Disagree, self.disagree),
            (DiffColumn::Invariant, self.invariant),
            (DiffColumn::Size, self.size),
        ]
        .into_iter()
        .filter(|(_, flag)| *flag != FlagFilter::Off)
        .map(|(column, _)| column)
        .collect()
    }

    fn flag_mut(&mut self, column: DiffColumn) -> Option<&mut FlagFilter> {
        match column {
            DiffColumn::Cmpl => Some(&mut self.cmpl),
            DiffColumn::Unmarked => Some(&mut self.unmarked),
            DiffColumn::Paint => Some(&mut self.paint),
            DiffColumn::Disagree => Some(&mut self.disagree),
            DiffColumn::Invariant => Some(&mut self.invariant),
            DiffColumn::Size => Some(&mut self.size),
            DiffColumn::Name | DiffColumn::Dataset => None,
        }
    }

    fn flag(&self, column: DiffColumn) -> FlagFilter {
        match column {
            DiffColumn::Cmpl => self.cmpl,
            DiffColumn::Unmarked => self.unmarked,
            DiffColumn::Paint => self.paint,
            DiffColumn::Disagree => self.disagree,
            DiffColumn::Invariant => self.invariant,
            DiffColumn::Size => self.size,
            DiffColumn::Name | DiffColumn::Dataset => FlagFilter::Off,
        }
    }

    fn is_active(&self, column: DiffColumn) -> bool {
        match column {
            DiffColumn::Name => self.name.is_some(),
            DiffColumn::Dataset => self.dataset.is_some(),
            _ => self.flag(column) != FlagFilter::Off,
        }
    }

    /// One clause per active filter, for the picker's title bar.
    fn labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        if let Some(name) = &self.name {
            labels.push(format!("name~{name}"));
        }
        if let Some(dataset) = self.dataset {
            labels.push(dataset.to_string());
        }
        for column in DiffColumn::ALL {
            // `Cmpl` and `Unmarked` are one predicate: the same direction on both is shown once
            // (as `Cmpl`), opposite directions are a real, if empty, query and show both.
            if column == DiffColumn::Unmarked && self.unmarked == self.cmpl {
                continue;
            }
            if let Some((yes, no)) = column.flag_labels() {
                match self.flag(column) {
                    FlagFilter::Off => {}
                    FlagFilter::Yes => labels.push(yes.to_string()),
                    FlagFilter::No => labels.push(no.to_string()),
                }
            }
        }
        labels
    }
}

/// The single column the `o` picker sorts by, and its direction. One column, not a stack: a hidden
/// secondary key would make two identical-looking tables order differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct DiffSort {
    column: DiffColumn,
    descending: bool,
}

impl Default for DiffSort {
    fn default() -> Self {
        DiffSort {
            column: DiffColumn::Name,
            descending: false,
        }
    }
}

impl DiffSort {
    fn toggled(self, column: DiffColumn) -> Self {
        if self.column == column {
            DiffSort {
                column,
                descending: !self.descending,
            }
        } else {
            DiffSort {
                column,
                descending: false,
            }
        }
    }

    fn arrow(self) -> &'static str {
        if self.descending { "v" } else { "^" }
    }
}

/// The `o` picker's cursor/sort/filter state, persisted on `App::diff_view` across reopenings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct DiffPickerView {
    /// The cursor column: what `s` and `f` act on.
    column: DiffColumn,
    sort: DiffSort,
    filters: DiffFilters,
}

/// The corpus scans the `o` picker's columns read, borrowed; `None` for a scan not run yet. Each
/// `*_of` accessor returns `None` for "not known", whether unscanned or absent from the scan.
#[derive(Clone, Copy, Default)]
struct DiffPickerData<'a> {
    unmarked: Option<&'a HashMap<String, usize>>,
    text_painted: Option<&'a HashMap<String, bool>>,
    disagreement: Option<&'a HashMap<String, usize>>,
    invariants: Option<&'a HashMap<String, usize>>,
    sizes: Option<&'a HashMap<String, usize>>,
}

impl<'a> DiffPickerData<'a> {
    fn from_app(app: &'a App) -> Self {
        DiffPickerData {
            unmarked: app.diff_unmarked.as_ref(),
            text_painted: app.diff_text_painted.as_ref(),
            disagreement: app.diff_disagreement.as_ref(),
            invariants: app.diff_invariants.as_ref(),
            sizes: app.diff_sizes.as_ref(),
        }
    }

    fn size_of(&self, name: &str) -> Option<usize> {
        self.sizes.and_then(|map| map.get(name)).copied()
    }

    fn unmarked_of(&self, name: &str) -> Option<usize> {
        self.unmarked.and_then(|map| map.get(name)).copied()
    }

    fn painted_of(&self, name: &str) -> Option<bool> {
        self.text_painted.and_then(|map| map.get(name)).copied()
    }

    fn disagreement_of(&self, name: &str) -> Option<usize> {
        self.disagreement.and_then(|map| map.get(name)).copied()
    }

    fn invariants_of(&self, name: &str) -> Option<usize> {
        self.invariants.and_then(|map| map.get(name)).copied()
    }
}

/// One row of the `O` picker's table; `size` is its `sample_diff_line_count`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SampleRow {
    pub(crate) name: String,
    pub(crate) language: String,
    /// The LOC stratum this sample was drawn for; `None` when not recorded, which the bucket
    /// filter never hides (the `FlagFilter::keeps` rule).
    pub(crate) bucket: Option<String>,
    pub(crate) status: SampleTriageStatus,
    pub(crate) size: usize,
}

/// One column of the `O` picker's table, driven by the same keys as `DiffColumn`. Not shared with
/// it: the two pickers have too few column shapes in common for a common type to read well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SampleColumn {
    #[default]
    Name,
    Lang,
    Bucket,
    Status,
    Size,
}

impl SampleColumn {
    /// Left-to-right order, shared by the header, cursor movement and `render_open_sample_picker`.
    const ALL: [SampleColumn; 5] = [
        SampleColumn::Name,
        SampleColumn::Lang,
        SampleColumn::Bucket,
        SampleColumn::Status,
        SampleColumn::Size,
    ];

    fn index(self) -> usize {
        SampleColumn::ALL
            .iter()
            .position(|column| *column == self)
            .unwrap_or(0)
    }

    /// Clamped rather than wrapping, as `DiffColumn::left`.
    fn left(self) -> Self {
        SampleColumn::ALL[self.index().saturating_sub(1)]
    }

    fn right(self) -> Self {
        SampleColumn::ALL[(self.index() + 1).min(SampleColumn::ALL.len() - 1)]
    }

    fn label(self) -> &'static str {
        match self {
            SampleColumn::Name => "Name",
            SampleColumn::Lang => "Lang",
            SampleColumn::Bucket => "Bucket",
            SampleColumn::Status => "Status",
            SampleColumn::Size => "Size",
        }
    }
}

/// Sort key for a LOC-bucket label: its lower bound ("30-100" -> 30, "3000+" -> 3000), since string
/// order puts "100-300" before "30-100". `None` sorts last. Parsed from the label because
/// `stats::sampling::LOC_BUCKETS` is behind the `stats` feature.
fn bucket_order(bucket: Option<&str>) -> usize {
    let Some(bucket) = bucket else {
        return usize::MAX;
    };
    bucket
        .split(['-', '+'])
        .next()
        .and_then(|lower| lower.parse().ok())
        .unwrap_or(usize::MAX)
}

/// Every column's filter; a row shows only if it passes all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SampleFilters {
    /// Case-insensitive name substring, stored lowercased.
    name: Option<String>,
    /// An exact language, cycled through those present in the list.
    language: Option<String>,
    /// An exact bucket label, cycled through those present. Never hides an unbucketed row.
    bucket: Option<String>,
    status: Option<SampleTriageStatus>,
    /// Whether the diff has any changed lines; an empty one is a broken draw to delete.
    size: FlagFilter,
}

impl SampleFilters {
    fn is_active(&self, column: SampleColumn) -> bool {
        match column {
            SampleColumn::Name => self.name.is_some(),
            SampleColumn::Lang => self.language.is_some(),
            SampleColumn::Bucket => self.bucket.is_some(),
            SampleColumn::Status => self.status.is_some(),
            SampleColumn::Size => self.size != FlagFilter::Off,
        }
    }

    fn any_active(&self) -> bool {
        SampleColumn::ALL
            .iter()
            .any(|column| self.is_active(*column))
    }

    /// The filters in force, for the picker's title bar; empty when none.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(name) = &self.name {
            parts.push(format!("name~{name}"));
        }
        if let Some(language) = &self.language {
            parts.push(format!("lang={language}"));
        }
        if let Some(bucket) = &self.bucket {
            parts.push(format!("bucket={bucket}"));
        }
        if let Some(status) = self.status {
            parts.push(format!("status={}", status.label()));
        }
        match self.size {
            FlagFilter::Off => {}
            FlagFilter::Yes => parts.push("size>0".to_string()),
            FlagFilter::No => parts.push("size=0".to_string()),
        }
        parts.join("  ")
    }

    fn keeps(&self, row: &SampleRow) -> bool {
        if let Some(needle) = &self.name
            && !row.name.to_lowercase().contains(needle)
        {
            return false;
        }
        if let Some(language) = &self.language
            && row.language != *language
        {
            return false;
        }
        if let Some(bucket) = &self.bucket
            && row.bucket.as_deref().is_some_and(|b| b != bucket)
        {
            return false;
        }
        if let Some(status) = self.status
            && row.status != status
        {
            return false;
        }
        self.size.keeps(Some(row.size > 0))
    }
}

/// The single column the `O` picker sorts by, as `DiffSort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct SampleSort {
    column: SampleColumn,
    descending: bool,
}

impl SampleSort {
    fn toggled(self, column: SampleColumn) -> Self {
        SampleSort {
            column,
            descending: self.column == column && !self.descending,
        }
    }

    fn arrow(self) -> &'static str {
        if self.descending { "v" } else { "^" }
    }
}

/// The `O` picker's cursor/sort/filter state, persisted on `App::sample_view` across reopenings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SamplePickerView {
    column: SampleColumn,
    sort: SampleSort,
    filters: SampleFilters,
}

/// The distinct values present in a column, in its sort order, for `f` to cycle through.
fn sample_language_values(rows: &[SampleRow]) -> Vec<String> {
    let mut values: Vec<String> = rows.iter().map(|row| row.language.clone()).collect();
    values.sort();
    values.dedup();
    values
}

fn sample_bucket_values(rows: &[SampleRow]) -> Vec<String> {
    let mut values: Vec<String> = rows.iter().filter_map(|row| row.bucket.clone()).collect();
    values.sort_by_key(|bucket| bucket_order(Some(bucket)));
    values.dedup();
    values
}

/// Advances a cycle-through-a-list filter: `None` (all) -> first -> ... -> last -> `None`.
fn next_value_filter(current: Option<&str>, values: &[String]) -> Option<String> {
    let Some(current) = current else {
        return values.first().cloned();
    };
    let position = values.iter().position(|value| value == current)?;
    values.get(position + 1).cloned()
}

fn next_status_filter(current: Option<SampleTriageStatus>) -> Option<SampleTriageStatus> {
    match current {
        None => Some(SampleTriageStatus::Sampled),
        Some(SampleTriageStatus::Sampled) => Some(SampleTriageStatus::Promoted),
        Some(SampleTriageStatus::Promoted) => Some(SampleTriageStatus::Rejected),
        Some(SampleTriageStatus::Rejected) => None,
    }
}

/// The rows the `O` picker shows: filtered, then sorted with the name as tiebreak. Shared by the
/// renderer and the key handler so both agree on what `selected` indexes.
fn visible_sample_rows(rows: &[SampleRow], view: &SamplePickerView) -> Vec<SampleRow> {
    let mut visible: Vec<SampleRow> = rows
        .iter()
        .filter(|row| view.filters.keeps(row))
        .cloned()
        .collect();

    match view.sort.column {
        SampleColumn::Name => visible.sort_by(|a, b| a.name.cmp(&b.name)),
        SampleColumn::Lang => {
            visible.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)))
        }
        SampleColumn::Bucket => visible.sort_by(|a, b| {
            bucket_order(a.bucket.as_deref())
                .cmp(&bucket_order(b.bucket.as_deref()))
                .then(a.name.cmp(&b.name))
        }),
        SampleColumn::Status => visible.sort_by(|a, b| {
            a.status
                .order()
                .cmp(&b.status.order())
                .then(a.name.cmp(&b.name))
        }),
        SampleColumn::Size => visible.sort_by(|a, b| a.size.cmp(&b.size).then(a.name.cmp(&b.name))),
    }
    if view.sort.descending {
        visible.reverse();
    }

    visible
}

/// Builds the `O` picker's modal with `current_name` selected if visible. `selected` indexes
/// `visible_sample_rows`, not `rows`.
fn open_sample_picker_modal(
    rows: Vec<SampleRow>,
    current_name: &str,
    view: SamplePickerView,
) -> Modal {
    let visible = visible_sample_rows(&rows, &view);
    let selected = visible
        .iter()
        .position(|row| row.name == current_name)
        .unwrap_or(0)
        .min(visible.len().saturating_sub(1));
    Modal::OpenSamplePicker {
        rows,
        selected,
        view,
        name_input: None,
    }
}

// ---------------------------------------------------------------------------------------------

/// The first 8 characters of a commit hash, for display.
fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(8)]
}

/// Every commit in this repository's history, newest first, as `(full hash, subject)`. Split on
/// `\x1f`, since a subject can contain almost anything else.
fn list_repo_commits() -> Result<Vec<(String, String)>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["log", "--pretty=format:%H%x1f%s"])
        .output()
        .context("running `git log` (is git installed?)")?;
    if !output.status.success() {
        bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (hash, summary) = line.split_once('\u{1f}')?;
            Some((hash.to_string(), summary.to_string()))
        })
        .collect())
}

/// Paths changed by commit `hash` that have a tree-sitter grammar. Empty for a merge commit, since
/// `diff-tree` shows none without `-m`/`-c`.
fn list_commit_files(hash: &str) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", hash])
        .output()
        .context("running `git diff-tree` (is git installed?)")?;
    if !output.status.success() {
        bail!(
            "git diff-tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|path| {
            language_for_path(Path::new(path)).is_some_and(|lang| to_treesitter(&lang).is_some())
        })
        .map(|path| path.to_string())
        .collect())
}

/// The content of `rev_path` (e.g. `"<hash>^:<path>"`) via `git show`. Empty, not an error, when
/// `git show` fails: the file is absent at that revision (added, deleted, or a root commit's
/// parent). Only a `git` that can't run is an error.
fn git_show(rev_path: &str) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("show")
        .arg(rev_path)
        .output()
        .context("running `git show` (is git installed?)")?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Ok(String::new())
    }
}

/// `path` at `hash^` and at `hash`, read from git. Bails if either side has no AST: unlike
/// `load_case`'s text-only mode, a browsing path that picks a file with no tree is a mis-pick.
fn load_git_commit_file(hash: &str, path: &str) -> Result<(Code, Code)> {
    let language = language_for_path(Path::new(path)).unwrap_or(Language::Unknown);

    let before_src = git_show(&format!("{hash}^:{path}"))?;
    let after_src = git_show(&format!("{hash}:{path}"))?;

    let mut before = Code::from_string(&before_src, &language);
    let mut after = Code::from_string(&after_src, &language);

    if before.ast.is_none() {
        bail!(
            "Before content for '{}' has no AST (unsupported or undetected language)",
            path
        );
    }
    if after.ast.is_none() {
        bail!(
            "After content for '{}' has no AST (unsupported or undetected language)",
            path
        );
    }
    before
        .ensure_parsed()
        .context("Failed to compute AST metadata for before code")?;
    after
        .ensure_parsed()
        .context("Failed to compute AST metadata for after code")?;

    Ok((before, after))
}

/// The promote-name pre-fill for a `C` case: `<language>-` lowercased (no repository, since it is
/// this one); empty for an unknown language.
fn default_promoted_name_for_path(path: &str) -> String {
    match language_for_path(Path::new(path)) {
        Some(language) => format!("{}-", language.to_string().to_lowercase()),
        None => String::new(),
    }
}

/// The `DIFF_DATASETS` folder promotion writes the current case into, shared by the prompt's text
/// and `action_promote` so they cannot disagree. `None` for `CaseOrigin::Diffs`, which saves in place.
fn promote_target_dataset(origin: &CaseOrigin) -> Option<&str> {
    match origin {
        CaseOrigin::Diffs => None,
        CaseOrigin::Sample(source) => Some(source.dataset.as_str()),
        // This repository's own commits are the handmade dataset's source.
        CaseOrigin::GitCommitFile { .. } => Some("handmade"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.key_log_summary {
        let path = key_log_path().context("no home directory to hold the key log")?;
        let contents = fs::read_to_string(&path).unwrap_or_default();
        print!("{}", summarize_key_log(&parse_key_log(&contents)));
        return Ok(());
    }

    // The same theme as the `omnidiff` binary. `set_custom_palette` must come first:
    // `OverlayTheme::Custom` resolves its colours from that process-global.
    theme::set_custom_palette(theme::load_custom_palette());
    let _ = OVERLAY_THEME.set(theme::load_overlay_theme());

    let memory = session_memory_path()
        .map(|path| load_session_memory(&path))
        .unwrap_or_default();
    let name = match args.name {
        Some(name) => name,
        None => starting_case(memory.last_case.as_deref())?,
    };

    let (before, after) = load_case(&name)?;
    let before_root_id = starting_cursor_id(&before);
    let after_root_id = starting_cursor_id(&after);

    let mapping = human_mapping::load_with(&name, &before, &after).unwrap_or_default();

    let mut app = App::new(
        name,
        CaseOrigin::Diffs,
        before_root_id,
        after_root_id,
        mapping,
    );
    app.diff_view = memory.diff_view;
    // Without a log file the session simply goes unrecorded.
    app.key_log = key_log_path().and_then(|path| KeyLog::open(&path).ok());

    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic_hook(info);
    }));

    let mut terminal = setup_terminal()?;
    let result = run_event_loop(&mut terminal, &mut app, before, after);
    restore_terminal()?;

    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal() -> Result<()> {
    if crossterm::terminal::is_raw_mode_enabled()? {
        execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
        disable_raw_mode()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
