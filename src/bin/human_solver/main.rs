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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

/**
* A helper binary for building the ground-truth AST mappings used by src/test/fixtures.
*
* Run as `cargo run --bin human_solver -- <name>`, where `<name>` is the name of a directory under
* `src/test/data/diffs/` (e.g. "rust-add-if"). If `<name>` is omitted, the first available case
* (alphabetically) opens instead - press `o` to pick a different one. It opens a Ratatui TUI
* showing the TreeSitter ASTs
* of the before and after code side by side (not the source text), lets a human walk both trees
* independently and mark nodes as matching, deleted or inserted, and saves the result as
* `src/test/data/diffs/<name>/human_mapping.json`. It also creates the corresponding
* `src/test/fixtures/<name>.rs` test file (if one doesn't already exist), which simply
* calls `codediff::test::helper::human_mapping::assert_matches_human_mapping`, a generic comparison
* that parses the code again, computes codediff's own diff and checks it agrees with the human
* mapping. Nodes are addressed by path (kind + sibling position), not by TreeSitter node ID, since
* IDs are not stable across separate parses -- see `test::helper::path_for_node`.
*
* Keybindings:
*   Tab            switch focus between the Before and After panels
*   Up/k, Down/j   move the focused panel's cursor
*   Left/h         collapse the current node, or move to its parent if already collapsed/a leaf
*   Right/l        expand the current node, or move to its first child if already expanded
*   g / G          jump to the first / last visible node
*   m              mark the Before cursor node and the After cursor node as matching. If their
*                  kinds differ, asks for confirmation (codediff never maps different kinds
*                  together, so this will always show as a mismatch, but can be useful for
*                  exploration). If the kinds match and neither node has children, the operation
*                  (Identical/Update) is inferred automatically by comparing their text. If the
*                  kinds match and either has children, it's classified automatically too: Identical
*                  if both subtrees' precomputed content hashes match (byte-identical), otherwise
*                  MatchButNotIdentical -- no prompt
*   M              like `m`, but also recurses into children pairwise as long as both sides
*                  have the same number of children with the same kinds; stops recursing (without
*                  error) at the first level that diverges, leaving it for manual resolution. Every
*                  pair, top-level and descendant alike, is classified the same way as `m` (by
*                  content hash for nodes with children, by text for leaves) with no prompting. Any
*                  pair that ends up classified Identical and has children is collapsed in both
*                  panels, to keep whole-unchanged subtrees from cluttering the view
*   f              repeats what a single `m` press does, over and over -- match the cursor pair,
*                  advance both cursors to their own next unmarked node, match that pair, and so
*                  on -- as if `m` were being pressed by hand again and again. Stops exactly where
*                  a human doing that would have to stop too: once there's nothing left to pair up
*                  (end of file), or the next pair has different kinds, which raises the same
*                  confirmation `m` would (pressing `f` again afterwards resumes the sweep)
*   a              if the focused cursor node is matched (per the human mapping), move the other
*                  panel's cursor to its matched node. If that node isn't currently visible,
*                  scrolls the other panel so it's centered in the viewport (clamped at the
*                  start/end of the tree, where a true center isn't possible)
*   A              like `a`, but aligns to the node codediff's own diff (`p`) mapped the cursor
*                  node to, instead of the human mapping. Requires `p` to have been run first
*   p              run codediff's own diff algorithm and show its verdict for every node in
*                  parentheses next to the human-marked status glyph -- M matched (any operation),
*                  - deleted, + inserted, ? no verdict (e.g. the tree root) -- for a quick visual
*                  comparison against the human mapping without leaving the TUI. A trailing `*`
*                  marks a node the human has already decided on where codediff's verdict
*                  disagrees (matched to a different node, or matched vs. deleted/inserted)
*   n / N          jump the focused cursor forward/backward to the next/previous node marked with
*                  that trailing `*` (a mismatch between the human mapping and codediff's verdict).
*                  Wraps around the ends of the tree. Requires `p` to have been run first
*   /              prompt for text, then move the focused cursor to the next leaf node (in
*                  document order, wrapping around) whose own text contains it -- a plain
*                  substring match, no regex. Pre-filled with the last search, if any, so `/` then
*                  Enter repeats it from wherever the cursor landed. Leaf nodes only, not any node
*                  whose subtree's text contains it: an ancestor's own text is the concatenation of
*                  everything inside it, so a whole-subtree check would almost always match the
*                  nearest enclosing container first, not the actual token
*   t              show the raw before/after source as plain text, side by side, instead of the
*                  AST tree -- for just reading the code. j/k scroll (both sides together), T
*                  switches to the unix diff view instead, Esc closes
*   T              show the output of the system `diff -u` between the before and after content --
*                  a plain line-based diff, as a point of comparison against codediff's own
*                  AST-based diff. j/k scroll, t switches to the text view instead, Esc closes
*   H              toggle hiding fully solved subtrees in both panels: a node (and everything
*                  under it) is hidden once it and every one of its descendants has some mark
*                  (matched, deleted or inserted) -- nothing left there to review. Any node that's
*                  still unmarked stays visible, together with its whole ancestor chain, since an
*                  ancestor of an unmarked node can never itself count as fully solved. Recomputed
*                  fresh every frame, so marking or unmarking a node updates what's hidden
*                  immediately, without needing to toggle `H` again
*   d / D          mark the Before cursor node as deleted / deleted with its whole subtree
*   i / I          mark the After cursor node as inserted / inserted with its whole subtree
*   u              remove the mark directly on the focused cursor node
*   s              if the current case is a real test case (opened via `o`): save
*                  human_mapping.json and ensure the optimal_solutions test stub exists. If it's a
*                  sample (opened via `O`): prompt for a name and promote it -- pre-filled with
*                  "<language>-<repository>" (lowercased, ".git" stripped - the same prefix
*                  convention every existing promoted name already follows), so only the
*                  descriptive suffix (e.g. "-add-item") needs typing. Promoting copies the
*                  sample's before/after content into src/test/data/diffs/<name>/, saves
*                  human_mapping.json and the test stub there, and records <name> against the
*                  matching row in sample.csv. Re-prompts if the name is empty, contains anything
*                  other than letters/digits/-/_, or a diffs/ case with that name already exists
*   R              on a sample (opened via `O`), reject it instead of promoting it: prompts for a
*                  reason, then records it verbatim in the matching sample.csv row's `comment`
*                  column and sets `status` to REJECTED, leaving `promoted_to` empty and the sample
*                  directory itself untouched. Re-prompts if the reason is empty. Has no effect on
*                  a real test case or a git-commit-sourced case -- only a sample has a sample.csv
*                  row to reject
*   e              on a diff (`o`) or a sample (`O`), enter or edit a free-form comment. On a
*                  diff it is written to `description.md` in the fixture's own directory,
*                  immediately on Enter (not on `w` - that saves human_mapping.json); an empty
*                  submission deletes the file. The `o` picker marks cases that have one with a
*                  leading `*` and shows the selected case's note along the bottom.
*                  It is the *only* home for a promoted fixture's note: `diff_inventory` reads
*                  `description.md` and nothing else, and promotion moves any sample comment into
*                  the file rather than leaving a copy in sample.csv (see `action_promote` /
*                  `update_sample_csv_at`). On a sample it still edits sample.csv instead:
*                  prompts for
*                  text, pre-filled with whatever's already recorded, and records it verbatim in
*                  the matching sample.csv row's `comment` column -- unlike `R`, doesn't touch
*                  `status`, and an empty submission clears the comment rather than being rejected
*                  as invalid input. If a comment is present when the sample is later promoted
*                  (`s`), it is written both as a leading comment in the generated
*                  optimal_solutions test stub and into the new fixture's `description.md`, and
*                  the sample.csv cell is cleared. Has no effect on a real test case or a
*                  git-commit-sourced case, same as `R`
*   o              open a different test case: a table of every directory under
*                  src/test/data/diffs/{handmade,small,full,stratified}/, one row per case and one
*                  column per thing worth triaging on - Name, Dataset, Cmpl, Unmarked, Paint,
*                  Disagree (see `DiffColumn`). j/k move between rows, Enter opens, Esc cancels.
*                  h/l move a cursor between *columns* (the current one is highlighted in the
*                  header), and the two keys that act on it are the same for every column:
*                    `s`  sort by the cursor column; pressing it again on the column that already
*                         owns the sort flips ascending/descending. Only ever one column sorts -
*                         the last one `s` was pressed on - with the case name as a stable
*                         tiebreak.
*                    `f`  filter on the cursor column: a substring prompt on Name (Enter applies,
*                         empty clears, Esc cancels - while it is open every key is text, not a
*                         command), the dataset cycle on Dataset (all -> handmade -> small -> full
*                         -> stratified -> all, see DIFF_DATASETS), and an off -> yes -> no cycle
*                         on each of the other four (e.g. Paint: all, painted only, unpainted
*                         only). Filters on different columns combine as an AND: every active one
*                         must match for a row to show. A row whose value for a column isn't known
*                         - the scan behind it hasn't been run, or the case failed to load - stays
*                         visible under either direction of that column's filter (see
*                         `FlagFilter::keeps`).
*                  Cmpl/Unmarked, Paint and Disagree each need a corpus-wide scan that only runs
*                  when `s` or `f` is first pressed on them, and it blocks - roughly 12s for
*                  Cmpl/Unmarked and 7s for Disagree over 513 fixtures on a 4-core machine (see
*                  `scan_corpus`, which runs them across threads; h/l alone never triggers one),
*                  so those columns read `?` until then. Cursor column, sort and
*                  every filter persist across closing and reopening this picker (they live on
*                  `App::diff_view`, not just this modal instance), same as the `O` picker's own
*                  hide/sort state below. Every filter and sort change re-anchors the selection on
*                  the row it was already on, falling back to the first row when that row is
*                  filtered out.
*                  If the current mapping has unsaved changes, asks first whether to save (only
*                  offered for a real test case; see `s` above) or discard them before switching
*   O              like `o`, but lists sampled candidates under src/test/data/samples/ instead --
*                  see `s` above for what happens when one of these is saved. Samples already
*                  promoted (per sample.csv's `status` column) are marked " - SOLVED", and rejected
*                  ones (see `R` above) " - REJECTED"; press `H` inside this picker to hide both,
*                  or `s` inside this picker to cycle its sort
*                  order: alphabetical, reverse alphabetical, smallest text diff first, largest
*                  text diff first (by changed-line count in a raw `diff -u`, not AST size) --
*                  unlike `H`, changing sort order always jumps selection to the first (1st) entry
*                  in the new order, rather than tracking the previously selected name. Both the
*                  hide-solved state and the sort order persist across closing and reopening this
*                  picker (they live on `App`, not just this modal instance) - the next `O` opens
*                  right back where the last one left off
*   C              open a commit from this repository's own `git log` (not a research repo): j/k
*                  to move, Enter to list the files it changed, Enter again on one of those to
*                  open its before/after content (before = the file at that commit's parent,
*                  after = at the commit itself; either side is empty content, not an error, if
*                  the file didn't exist there -- e.g. it was added or deleted by the commit), Esc
*                  cancels either picker. Only files with a supported language are listed. Like a
*                  sample (`O`), `s` then prompts for a name to promote it under -- but always into
*                  `handmade/` (this *is* the handmade dataset's own source), and pre-filled with
*                  just `<language>-` (e.g. "rust-"), since there's no second repository to name
*   ?              show a popup listing every keybinding (`?` or Esc closes it)
*   q / Esc        quit
*
* After a match finalizes (including a modal answer), both panels' cursors auto-advance to their
* own next unmarked node (if one exists past the current position). After an insert or delete,
* only the panel that was actually marked (After or Before, respectively) auto-advances, since the
* other side wasn't touched. This makes stepping through a tree top to bottom mostly just holding
* down the marking key.
*/
use std::io::{self, Stdout, Write};
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
use serde::Deserialize;

mod actions;
mod events;
mod flatten;
mod navigate;
mod render;
mod state;
mod stubs;
#[allow(unused_imports)]
use actions::*;
#[allow(unused_imports)]
use events::*;
#[allow(unused_imports)]
use flatten::*;
#[allow(unused_imports)]
use navigate::*;
#[allow(unused_imports)]
use render::*;
#[allow(unused_imports)]
use state::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use stubs::*;
use tree_sitter::Node;

use codediff::code::language::{language_for_path, to_treesitter};
use codediff::code::{Code, Language};
use codediff::diff::text::TextDiff;
use codediff::diff::{ASTDiff, ASTMappingReason, NodeCache, diff_code};
use codediff::test::helper::human_mapping::{
    self, Caches, HumanMapping, HumanMappingEntry, HumanOperation, HumanTextEntry,
    HumanTextMapping, HumanTextOperation, HumanTextSpan, HumanTextVerdict, MarkKind, MultiMapGroup,
    NamedTextMapping, NodeStatus, disagreement_is_move_only, is_inherited_removed, path_refs,
    rebuild_caches_for_mapping, status_after, status_before, text_mapping_disagreements,
};
use codediff::tui::theme::{self, OverlayPalette, OverlayTheme};
// Only used by this file's own test module (`rebuild_caches_for_mapping`, imported above, is the
// one the non-test code path uses).
#[cfg(test)]
use codediff::test::helper::human_mapping::rebuild_caches;
use codediff::test::helper::{
    DIFF_DATASETS, code_pair_from_dir, diffs_case_dir, node_for_path, path_for_node,
    precompute_paths, read_note, write_note,
};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Interactively build a human ground-truth AST mapping for an optimal_solutions test case"
)]
struct Args {
    /// Name of the test case, i.e. the directory name under src/test/data/diffs/ (e.g.
    /// "rust-add-if"). Always starts with a language prefix. If omitted, the first available
    /// case (alphabetically) opens instead - press `o` to pick a different one.
    name: Option<String>,
}

/// Shown by the `?` help popup (`Modal::Help`). Kept as a plain reference sheet, separate from the
/// fuller explanations in this file's own doc comment, so it fits legibly in a single screen.
const HELP_TEXT: &str = "\
Tab            switch focus between Before/After panels
Up/k, Down/j   move cursor
Left/h         collapse current node, or go to its parent
Right/l        expand current node, or go to its first child
g / G          jump to first / last visible node

m / M          match cursor nodes (M also recurses into matching children);
                 with a pending multi-map selection (see x), commits it as a
                 group instead (M asserts the matched subtree closes over
                 itself, without auto-filling descendants the way plain M does)
f              repeat m until end of file or a kind mismatch needs your input
d / D          mark Before node deleted / deleted with subtree
i / I          mark After node inserted / inserted with subtree
u              unmark the focused cursor node, or remove its whole multi-map
                 group if it's a group member
x              toggle the focused cursor node in/out of a pending multi-map
                 selection -- select several nodes on each side, then m/M to
                 commit them as a group where any Before node may pair with
                 any After node (leftovers on the larger side become
                 delete/insert); mixed kinds ask for confirmation first
c              clear the pending multi-map selection
a / A          align other panel to the human mapping / to codediff's mapping
p              run codediff's own diff, show its verdict next to each node
r              toggle showing the ASTMappingReason (which pass matched it) next
                 to each node's algo verdict
n / N          jump to next / previous mismatch (`*`) vs. codediff's verdict
/              search: jump to the next leaf node whose text contains the
                 given string (plain substring, no regex)

t              text view: read the source, and paint the human text-range ground
                 truth onto it (stored beside the tree mapping, not derived from
                 it). Tab side, hjkl/0/$/g/G move, v select. By default a
                 selection spanning several rows is vertical -- the same columns
                 on each row, like a stack of squares, not every full line swept
                 in between; V toggles that to a full-line sweep, for a single
                 contiguous multi-line block. d/i paint the
                 Before/After selection deleted/inserted, m pairs BOTH sides'
                 selections as a match (move vs update derived from whether the
                 spans' text is identical), u removes the range under the cursor,
                 Z marks a nothing-to-paint fixture, : jumps to a line number,
                 Esc unselects or closes.
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
                 p cycles what is drawn: your painting, codediff's own rendering
                 of the same pair, or only the bytes where the two disagree
T              view the output of unix `diff -u`, with before/after line numbers
                 (t/T switch between these two views while either is open)
H              toggle hiding fully solved subtrees (unmarked nodes and their
                 ancestors always stay visible)

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
                 Name, Dataset, Cmpl, Unmarked, Paint, Disagree. j/k pick a row,
                 h/l pick a column, s sorts by that column (again to reverse),
                 f filters on it -- substring on Name, dataset cycle on Dataset,
                 off/yes/no on the rest. Filters AND together across columns.
                 The scans behind Cmpl/Unmarked, Paint and Disagree run on the
                 first s or f on that column (Cmpl/Unmarked blocks for ~12s and
                 Disagree ~7s on the full corpus); until then those
                 columns read ?, and a ? row survives either filter direction.
                 Cursor, sort and filters persist across o
O              open a sampled candidate (src/test/data/samples/); already-promoted
                 samples are marked \" - SOLVED\", rejected ones \" - REJECTED\" --
                 press H inside this picker to hide/show both, or s to cycle its
                 sort order (A-Z, Z-A, smallest/largest text diff first) -- both
                 persist across O
C              open a commit from this repo's own git log, then a file it
                 changed -- before/after are that file at the commit's parent
                 and at the commit itself; s promotes into handmade/

?              toggle this help
q / Esc        quit
";

/// Loads and parses the before/after code for a test case, by name. Parses only this one case's
/// directory (rather than every directory under src/test/data/diffs/, as a naive
/// `handmade_test_code_pairs`-style lookup would) since this runs on every `o`-picker open, not
/// just at startup. Resolves across `DIFF_DATASETS` (`handmade`/`small`/`full`) via the library's
/// own `diffs_case_dir`, same as every other per-name lookup in this file.
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

    if before.ast.is_none() {
        bail!(
            "Before code for '{}' has no AST (unsupported or undetected language)",
            name
        );
    }
    if after.ast.is_none() {
        bail!(
            "After code for '{}' has no AST (unsupported or undetected language)",
            name
        );
    }

    // Populates node_to_full_hash (among other things), which `m`/`M` use to auto-classify
    // matches on nodes with children instead of asking.
    before
        .ensure_parsed()
        .context("Failed to compute AST metadata for before code")?;
    after
        .ensure_parsed()
        .context("Failed to compute AST metadata for after code")?;

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

/// The dataset (`handmade`/`small`/`full`) an existing diffs/ case named `name` actually lives
/// under, for display purposes (the title bar, prompts) - `None` if `name` isn't a case at all.
fn case_dataset(name: &str) -> Option<String> {
    diffs_case_dir(name)?
        .parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().into_owned())
}

/// Names of every directory directly under `root` (not just the ones that successfully parse
/// into a Code pair), for the `o`/`O` open pickers.
fn list_dir_names(root: &Path) -> Result<Vec<String>> {
    // Not an error: `samples/` in particular won't exist at all until `materialize_test_diffs`
    // has been run once, and `full/` (see `DIFF_DATASETS`) is deliberately empty until the full
    // dataset is available.
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

/// Every case name across all `DIFF_DATASETS` folders (`handmade`/`small`/`full`/`stratified`) -
/// the `o` picker doesn't distinguish between them (see the title bar's `[dataset]` tag, via
/// `case_dataset`, for where a given case actually lives), since names are unique across all of
/// them by construction (`action_promote`'s collision check spans all of them too).
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

/// The case names the `o` picker actually shows, in the order it shows them: `options`
/// (`list_available_cases`'s output) narrowed by every active filter in `view`, then ordered by
/// `view.sort`.
///
/// **Filters combine as an AND** (see `DiffFilters`), and a row whose value for a column isn't
/// known survives that column's filter in either direction (see `FlagFilter::keeps` for the full
/// argument - in short, "not scanned yet" and "couldn't be measured" are not evidence a fixture
/// needs no attention).
///
/// **The order is total.** The primary key is `view.sort.column`, reversed when
/// `view.sort.descending`; the name is always the tiebreak, always ascending, so two rows that
/// tie on an unscanned column (every row ties, in that case) still come out in a stable, readable
/// order rather than whatever order `options` happened to arrive in. Unknown values sort after
/// known ones ascending, and the reversal moves them to the front - `sort_rank`/`bool_rank` carry
/// that as an explicit leading flag rather than leaving it to a sentinel value that a real
/// measurement could collide with.
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
        .map(|(name, _)| name.as_str())
        .collect();

    // `options`, not `visible`, is what carries each name's dataset; looked up per comparison
    // rather than materialized into a parallel list, since only the `Dataset` sort ever asks.
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

/// Sort key for a numeric column: the leading `false`/`true` puts every known value ahead of every
/// unknown one under an ascending sort, without pretending an unknown row measured any particular
/// number.
fn sort_rank(value: Option<usize>) -> (bool, usize) {
    (value.is_none(), value.unwrap_or(0))
}

/// `sort_rank` for a yes/no column - `false` (the condition doesn't hold) sorts before `true`, and
/// unknown after both.
fn bool_rank(value: Option<bool>) -> (bool, bool) {
    (value.is_none(), value.unwrap_or(false))
}

/// Builds the `o` picker's modal from a freshly-listed `options`, `current_name` (the case already
/// open - or, when reopening after a filter/sort change, the row that was selected - so the
/// selection follows the row it was on rather than jumping to the top), and the persisted `view`
/// (`App::diff_view`). Falls back to the first visible row when that name isn't in the filtered
/// view at all. Keeping the real logic here, rather than in the `KeyCode::Char('o')`/`'s'`/`'f'`
/// handlers, makes it unit-testable without real files under src/test/data/diffs/ - same shape as
/// `open_sample_picker_modal` for `O`, and for the same reason.
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

/// Cycles the `o` picker's dataset filter, in `DIFF_DATASETS` order, wrapping back to "all"
/// (`None`) after the last one - what `f` does on the `Dataset` column, same convention as
/// `SampleSortOrder::next`.
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

/// Runs `scan` over every case name in `names` across several threads, collecting the `Some`
/// results into a map. The shared shape of all four of the `o` picker's corpus scans - each is a
/// pure per-case function of the filesystem, so the only thing they had in common before this was
/// a `filter_map` over `list_available_cases`, and the only thing they need now is a work queue.
///
/// **Why this is safe to run concurrently.** Every scan body reaches the filesystem through
/// `code_pair_from_dir`, `human_mapping::mapping_path`/`load` or `read_note`, all of which read and
/// parse per call with no shared state. In particular none of them goes through
/// `handmade_test_code_pair`'s process-wide `Mutex<HashMap<_, Arc<(Code, Code)>>>` - that cache
/// never evicts, and filling it from N threads is exactly the shape that got a `cargo test` run
/// OOM-killed at 12-16GB (see its own doc comment). Nothing a worker parses outlives the closure
/// that made it; only the plain `T` result crosses a thread boundary.
///
/// **A worker panic propagates**, rather than being turned into a missing map entry. A dropped
/// slice of the corpus would read as `?` in the picker - indistinguishable from "not scanned yet"
/// under `FlagFilter::keeps` - so silently returning a partial map would quietly lie about which
/// cases were measured. `main` installs a panic hook that restores the terminal first, so this
/// fails the same visible way a sequential scan always did.
///
/// Deterministic despite the interleaving: results land in a `HashMap`, and every consumer orders
/// through `visible_diff_options`, which breaks every tie on the case name. All four scans were
/// measured to return byte-identical entry counts single-threaded and parallel.
///
/// Used for all four scans even though two of them are already cheap (`compute_diff_text_painted`
/// 616ms -> 292ms, `compute_diff_comments` 14ms -> 1.6ms over 513 fixtures): they are I/O-bound
/// rather than CPU-bound, so the gain is smaller, but it is a gain on the measurements above and
/// keeping one code path for all four is worth more than the handful of milliseconds either way.
fn scan_corpus<T, F>(
    names: &[(String, &'static str)],
    scan: F,
) -> std::collections::HashMap<String, T>
where
    T: Send,
    F: Fn(&str) -> Option<T> + Sync,
{
    scan_corpus_with_threads(names, default_scan_threads(), scan)
}

/// How many threads `scan_corpus` uses: the machine's parallelism, capped at
/// `MAX_SCAN_THREADS`.
fn default_scan_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_SCAN_THREADS)
}

/// Ceiling on `scan_corpus`'s worker count, on top of `available_parallelism`. Each worker holds
/// one fixture's two parsed trees (and, for the disagreement scan, an `ASTDiff` + `NodeCache` +
/// `TextDiff` on top) at a time, so peak RSS scales with this number and nothing else bounds it -
/// and this repo has been OOM-killed by exactly that kind of multiplier before (see
/// `handmade_test_code_pair`).
///
/// Measured on the unmarked scan over 513 fixtures (release, 4-core machine, 2026-09-02): 833 MB
/// at one worker, 2053 MB at four, 2978 MB at eight - so roughly 300 MB of headroom per extra
/// worker. The same run shows why the ceiling costs nothing in speed: at eight workers on four
/// cores the wall clock was 11.9s against 12.4s at four, i.e. within noise, while the disagreement
/// scan was actually *slower* oversubscribed (8.2s vs 7.5s). Past the core count this buys memory
/// pressure and no time, so eight is a bound on big machines rather than a target.
///
/// The other place this multiplies is the test suite, where nextest runs each corpus-scanning test
/// in its own process concurrently. Measured after this change: peak 1.99 GB summed across every
/// `human_solver` test process, with the suite down from 37.2s to 12.5s - well inside the headroom
/// the nextest migration established, so no per-test thread limit is needed.
const MAX_SCAN_THREADS: usize = 8;

/// `scan_corpus` with the worker count pinned - the seam the tests use to check that a parallel
/// run returns exactly what a single-threaded one does.
fn scan_corpus_with_threads<T, F>(
    names: &[(String, &'static str)],
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
            .filter_map(|(name, _)| scan(name).map(|value| (name.clone(), value)))
            .collect();
    }

    // A shared cursor rather than a fixed slice per thread: fixtures differ in size by orders of
    // magnitude (one mapping file alone is 80 MB), so an even split by count would leave most
    // workers idle waiting for whichever one drew the giants.
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
                        let Some((name, _)) = names.get(index) else {
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

/// Runs, if it hasn't already this session, whichever corpus-wide scan `column` reads - so `s` and
/// `f` on a column show a real ranking or a real filter rather than a table of `?`.
///
/// Called only from those two keys, never from `h`/`l`: the scans still take real wall-clock time
/// the first time even after `scan_corpus` parallelized them (order of ten seconds for
/// `Cmpl`/`Unmarked`, under ten for `Disagree`, both sub-second for `Paint` - see
/// `compute_diff_unmarked` for the full numbers), and stalling on plain
/// cursor movement across the header would make the picker feel broken. Pressing `s`/`f` is a
/// deliberate request for that column's data, which is exactly when paying for it is reasonable -
/// the same bargain the `H`/`X`/`Y` keys this replaces each struck on their own.
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
        // Both are read straight off `list_available_cases`' own output - nothing to scan.
        DiffColumn::Name | DiffColumn::Dataset => {}
    }
}

/// How many nodes in `root`'s subtree are still `NodeStatus::Unmarked` under `caches` - the
/// whole-tree counterpart of `count_unmarked` (which counts only the rows currently *visible* in
/// a panel, for the status line). The `o` picker's `Unmarked` column needs the count rather than
/// the "is any left?" boolean this replaced, because how much work a fixture still needs is what
/// orders a triage queue; the lost short-circuit costs an unfinished fixture a full walk, which is
/// a rounding error next to the tree-sitter parse and mapping rebuild `diff_case_unmarked_count`
/// already does per case.
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

/// How many nodes `name`'s current human-authored mapping still leaves `NodeStatus::Unmarked`,
/// across both its before and after trees - i.e. how much annotation work is left on it. `None` if
/// the case's code or mapping couldn't be loaded at all - no `human_mapping.json` yet, a directory
/// that doesn't parse as a valid case, or (rarer) source that no longer parses. That `None` is
/// carried all the way through to the picker as an unknown (`?`) rather than being flattened into
/// a number: a case that fails to load must not read as "0 left to do", and under
/// `FlagFilter::keeps` an unknown row stays visible under either direction of the filter, so a
/// broken case is surfaced rather than hidden. Pressing Enter on it in the picker still goes
/// through `load_case`'s own error handling as normal; this function doesn't change what opening
/// it does, only what the `Cmpl`/`Unmarked` columns say about it.
fn diff_case_unmarked_count(name: &str) -> Option<usize> {
    let dir = diffs_case_dir(name)?;
    let (before, after) = code_pair_from_dir(&dir).ok().flatten()?;
    let mapping = human_mapping::load(name).ok()?;
    let before_root = before.ast.as_ref()?.root_node();
    let after_root = after.ast.as_ref()?.root_node();
    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
    Some(
        count_unmarked_nodes_in_tree(before_root, &caches, status_before)
            + count_unmarked_nodes_in_tree(after_root, &caches, status_after),
    )
}

/// Refreshes just `name`'s entry in `App::diff_unmarked`, if the cache has been built at all this
/// session - called after a save, since that's the only way a case's unmarked count can change
/// mid-session, and a targeted single-fixture refresh is cheap, unlike rebuilding the whole cache
/// (see that field's own doc comment for why that's worth avoiding).
fn refresh_diff_unmarked(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_unmarked
        && let Some(count) = diff_case_unmarked_count(name)
    {
        map.insert(name.to_string(), count);
    }
}

/// Builds `App::diff_unmarked` for every case `list_available_cases` currently lists - the `o`
/// picker's `Cmpl` and `Unmarked` columns need this for the whole corpus before either can filter
/// or sort, unlike `O`'s `hide_solved` (a cheap lookup against sample.csv, no parsing involved).
///
/// **The most expensive of the four scans.** Measured over this repo's 513 fixtures (release
/// build, 4-core machine, 2026-09-02): **38.9s single-threaded, 12.4s through `scan_corpus`**
/// (peak RSS 833 MB and 2053 MB respectively), returning the same 512 entries either way - one
/// case fails to load and stays absent. The old comment here claimed "roughly 10s", which dated
/// from a ~230-fixture corpus and was never true at this size.
///
/// Almost all of the single-threaded 38.9s is per-case work with no shared state -
/// `code_pair_from_dir` (tree-sitter, both sides) plus `human_mapping::load` (the corpus' mapping
/// JSON runs to over a gigabyte, one file of it 80 MB) - which is exactly why it parallelizes
/// nearly linearly. The two tree walks are about 8s of it, which is what made counting affordable
/// in place of the short-circuiting "is any node unmarked?" predicate this replaced.
///
/// It is bearable at all only because `rebuild_caches_for_mapping` resolves every entry's path
/// through a `PathCache` rather than rescanning siblings per entry - see `rebuild_caches`'s own
/// doc comment for the very different cost that used to be.
fn compute_diff_unmarked() -> std::collections::HashMap<String, usize> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&options, diff_case_unmarked_count)
}

/// Whether `name`'s human mapping already carries a painted text-range mapping (see
/// `HumanTextMapping`) - the text-painting counterpart of `diff_case_unmarked_count`.
///
/// **A substring scan, not a JSON parse, and that is not a micro-optimization.** The corpus's 513
/// `human_mapping.json` files come to ~1.4 GB (one is ~29,600 lines on its own), and parsing them
/// all to ask whether one key is present costs on the order of the whole-corpus scans it was meant
/// to be the cheap counterpart of. Searching for the quoted key instead is a linear scan with no
/// allocation per entry, and it works: `compute_diff_text_painted` measured **616ms
/// single-threaded, 292ms through `scan_corpus`** over the full corpus (release, 2026-09-02),
/// against `compute_diff_unmarked`'s 38.9s/12.4s on the same run. It is the one column whose data
/// is effectively free.
///
/// The token includes its quotes deliberately. Every string this file stores is either a
/// tree-sitter node kind or a `kind:index` path element, and `serde_json` escapes any quote inside
/// a string value as `\"` - so a bare `"text_mapping"` can only be a JSON *key*, never content.
/// Key order doesn't matter either, unlike a tail-only read: a hand-edited file that moved the key
/// is still found.
///
/// `None` if the file can't be read, which `compute_diff_text_painted` treats as "not painted" for
/// the same fail-open reason `compute_diff_unmarked` leaves a case it cannot measure out of its map:
/// a case this can't read is exactly what the filter should surface, not hide.
///
/// Deliberately keyed on presence, not on emptiness. A fixture whose two files are identical has
/// nothing to paint, and `Z` in the text view records that as `Some` with no entries - which is
/// painted, and must not come back into the queue every session. That distinction is the entire
/// reason `HumanMapping::text_mapping` is an `Option` rather than a plain `Vec`.
fn diff_case_has_text_mapping(name: &str) -> Option<bool> {
    let path = human_mapping::mapping_path(name);
    let contents = std::fs::read_to_string(path).ok()?;
    Some(contents.contains("\"text_mappings\""))
}

/// Refreshes just `name`'s entry in `App::diff_text_painted`, for the same reason (and at the same
/// call sites) as `refresh_diff_unmarked`: saving is the only thing that can change a case's
/// painted-ness mid-session.
fn refresh_diff_text_painted(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_text_painted
        && let Some(painted) = diff_case_has_text_mapping(name)
    {
        map.insert(name.to_string(), painted);
    }
}

/// Builds `App::diff_text_painted` for every case `list_available_cases` lists - the `o` picker's
/// `X` toggle needs the whole corpus before it can filter.
///
/// Much cheaper than `compute_diff_unmarked`, which it otherwise mirrors: this scans bytes,
/// where that one parses both source files with tree-sitter and walks two trees. Still done lazily
/// on first `X` rather than eagerly on every `o`, both to match `H`'s behaviour and because 1.4 GB
/// of mapping files is not free to read however cheap the per-file test is.
fn compute_diff_text_painted() -> std::collections::HashMap<String, bool> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    // `Some(..unwrap_or(false))`, not `diff_case_has_text_mapping` directly: this map is keyed on
    // every listed case, with an unreadable one recorded as unpainted rather than left absent -
    // the fail-open direction this scan has always had, and the one place the four scans differ
    // in what they do with a `None`.
    scan_corpus(&options, |name| {
        Some(diff_case_has_text_mapping(name).unwrap_or(false))
    })
}

/// How many bytes `name`'s human tree mapping and human text painting disagree about, via
/// `text_mapping_disagreements` - the same pure ground-truth-vs-ground-truth comparison
/// `test::helper::human_mapping::exploratory_mapping_vs_painting_agreement_census` reports,
/// excluding `disagreement_is_move_only` runs (the one unavoidable rendering artifact from
/// `TextDiff::from`'s column-shift `Move` heuristic - see that function's own doc comment). `None`
/// when the case can't be loaded, or has no text painting yet at all (nothing to compare against -
/// distinct from "compared and agrees exactly", which is `Some(0)`).
fn diff_case_disagreement_bytes(name: &str) -> Option<usize> {
    let dir = diffs_case_dir(name)?;
    let (before, after) = code_pair_from_dir(&dir).ok().flatten()?;
    let mapping = human_mapping::load(name).ok()?;
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

/// Refreshes just `name`'s entry in `App::diff_disagreement`, for the same reason (and at the same
/// call sites) as `refresh_diff_unmarked`/`refresh_diff_text_painted`: saving is the only
/// thing that can change a case's disagreement score mid-session.
fn refresh_diff_disagreement(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_disagreement {
        match diff_case_disagreement_bytes(name) {
            Some(bytes) => {
                map.insert(name.to_string(), bytes);
            }
            // A case with no text painting yet has nothing to compare - stays absent from the map
            // rather than reporting a misleading 0, same as `diff_case_has_text_mapping`'s
            // presence-not-emptiness distinction.
            None => {
                map.remove(name);
            }
        }
    }
}

/// Builds `App::diff_disagreement` for every case `list_available_cases` lists that already has a
/// text painting - the `o` picker's `s` disagreement-sort and `Y` filter need this for the whole
/// corpus before they can rank/hide by it. The most expensive of the four lazy `o`-picker scans:
/// unlike `compute_diff_unmarked` (parses both sides once), this also builds a synthetic
/// `ASTDiff` from the tree mapping and renders it through `TextDiff::from` per fixture - still
/// bounded by the same corpus size, just a heavier constant per fixture.
fn compute_diff_disagreement() -> std::collections::HashMap<String, usize> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&options, diff_case_disagreement_bytes)
}

/// Every case's note, keyed by case name, for the `o` picker. Cases without one are simply
/// absent.
///
/// The cheapest of the three picker scans by a wide margin: a few hundred bytes per fixture where
/// `compute_diff_text_painted` reads 1.4 GB of JSON and `compute_diff_unmarked` parses both
/// sides with tree-sitter. Most fixtures have no `description.md` at all, so most of this is a
/// failed `stat`.
fn compute_diff_comments() -> std::collections::HashMap<String, String> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&options, read_note)
}

/// Refreshes just `name`'s entry, for the same reason as `refresh_diff_text_painted`: `e` is the
/// only thing that can change a case's note mid-session, and the row it was typed on should not
/// read as un-noted until the next restart.
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

/// A sample's disposition, as recorded in its sample.csv row's `status` column - `Sampled` if
/// nothing has been decided yet (including when the row predates that column, or no row matches
/// at all: the same "nothing decided" default `default_status`/`default_sample_status` already
/// use). Drives the `O` picker's " - SOLVED"/" - REJECTED" suffixes and what `hide_solved` hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleTriageStatus {
    Sampled,
    Promoted,
    Rejected,
}

impl SampleTriageStatus {
    /// Whether `hide_solved` should hide this sample: both a promotion and a rejection are a
    /// finished triage decision -- nothing left to review -- unlike `Sampled`.
    fn is_handled(self) -> bool {
        matches!(
            self,
            SampleTriageStatus::Promoted | SampleTriageStatus::Rejected
        )
    }
}

/// Every sample directory name under src/test/data/samples/, paired with the triage status of the
/// sample.csv row it came from. A sample's status is looked up by matching its `source.json`
/// (language, repository, commit, path) against a sample.csv row -- the same join
/// `action_promote`/`action_reject` use, so it stays correct even if a promoted diffs/ case was
/// later renamed or the sample directory has a numbered suffix.
fn list_samples_with_status() -> Result<Vec<(String, SampleTriageStatus)>> {
    let names = list_dir_names(&samples_root())?;
    let statuses = sample_triage_statuses()?;

    Ok(names
        .into_iter()
        .map(|name| {
            let status = source_json_for_sample(&name)
                .and_then(|source| {
                    statuses
                        .get(&(
                            source.language,
                            source.repository,
                            source.commit,
                            source.path,
                        ))
                        .copied()
                })
                .unwrap_or(SampleTriageStatus::Sampled);
            (name, status)
        })
        .collect())
}

/// Reads just the provenance out of a sample's `source.json`, without parsing its before/after
/// code (unlike `load_sample`) -- cheap enough to call once per sample when listing.
fn source_json_for_sample(name: &str) -> Option<SampleSource> {
    let contents = fs::read_to_string(samples_root().join(name).join("source.json")).ok()?;
    serde_json::from_str(&contents).ok()
}

fn sample_triage_statuses()
-> Result<std::collections::HashMap<(String, String, String, String), SampleTriageStatus>> {
    sample_triage_statuses_at(&sample_csv_path())
}

/// The triage status of every row in the sample.csv at `path`, keyed by (language, repository,
/// commit, path). Returns an empty map, not an error, if `path` doesn't exist.
fn sample_triage_statuses_at(
    path: &Path,
) -> Result<std::collections::HashMap<(String, String, String, String), SampleTriageStatus>> {
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
            ((row.language, row.repository, row.commit, row.path), status)
        })
        .collect())
}

/// Provenance recorded by `materialize_test_diffs` alongside each `src/test/data/samples/<name>/`
/// fixture (`source.json`): the exact `sample.csv` row it came from. Used, when the sample is
/// promoted, to find and update that row without having to reverse-engineer it from the
/// (lossy: lowercased, 8-char commit) directory name, and (`dataset`) to know which of
/// `DIFF_DATASETS` `action_promote` should place the fixture under.
#[derive(Debug, Clone, Deserialize)]
struct SampleSource {
    language: String,
    repository: String,
    commit: String,
    path: String,
    /// Which research dataset (tiny/small/full) this sample was materialized from - see
    /// `sample_test_diffs`'s `--dataset`. Defaults to `legacy_dataset()` for a `source.json`
    /// written before provenance tracking existed, so samples already on disk keep working
    /// without needing to be regenerated.
    #[serde(default = "legacy_dataset")]
    dataset: String,
}

/// Historical default for provenance that predates this field: every sample materialized before
/// `dataset` existed really was pulled from the small research checkout (the only one available
/// on this machine at the time), so this is a real fallback value, not a placeholder. Shared by
/// `SampleSource::dataset`'s serde default and `ensure_stub_test`'s dataset resolution.
fn legacy_dataset() -> String {
    "small".to_string()
}

/// A starting point for `s`'s promote-name prompt: `<language>-<repository>`, lowercased, with the
/// repository's trailing `.git` stripped - same transformation `materialize_test_diffs.rs`'s own
/// `base_name` applies to these same two fields, so the prefix a human sees here already matches
/// the convention every existing promoted name follows. Deliberately just the prefix, not a full
/// name: the descriptive suffix (what actually changed, e.g. "-add-item") still needs a human's
/// judgment, and `validate_new_case_name` still rejects this if it collides or is otherwise
/// invalid, same as before - this only saves retyping the boilerplate part.
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

/// Runs the system `diff -u` between `before_src` and `after_src`, as a point of comparison
/// against codediff's own AST-based diff. Writes both sides to temp files first (rather than
/// relying on any on-disk path the content may have originally come from) so this always reflects
/// exactly what's currently loaded, regardless of case origin.
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
        // 0 = identical, 1 = differences found -- both normal outcomes, not errors.
        Some(0) => Ok("(no textual differences)".to_string()),
        Some(1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => bail!("diff failed: {}", String::from_utf8_lossy(&output.stderr)),
    }
}

/// Reads the raw `before.<ext>.test`/`after.<ext>.test` contents of a fixture directory, without
/// parsing an AST -- cheap enough to call once per sample when computing the `O` picker's
/// text-diff-size sort keys (unlike `load_sample`, which parses both sides via tree-sitter just to
/// display them). `None` if either file is missing or unreadable.
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

/// The number of changed (`+`/`-`, excluding the `+++`/`---` header lines) lines in the unified
/// `diff -u` between a sample's raw before/after content -- the sort key for
/// `SampleSortOrder::{Smallest,Largest}DiffFirst`. A cheap, language-agnostic proxy for "how big a
/// change is this" that needs no AST, unlike every other size notion this tool otherwise works
/// with. `0` (not an error) if the sample's files can't be read or `diff` can't be run, so a
/// missing/malformed sample just sorts as if it were empty rather than breaking the picker.
fn sample_diff_line_count(name: &str) -> usize {
    let Some((before, after)) = raw_before_after(&samples_root().join(name)) else {
        return 0;
    };
    let Ok(diff) = run_unix_diff(before.as_bytes(), after.as_bytes()) else {
        return 0;
    };
    diff.lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .count()
}

/// One column of the `o` picker's table, left to right - the unit `h`/`l` move the cursor
/// between, and the thing both `s` (sort) and `f` (filter) act on. Every column supports both, so
/// there is one pair of keys to remember rather than one letter per dimension (this picker used to
/// bind `d`/`H`/`X`/`Y` for four separate filters and `s` for a fixed four-way sort cycle, which
/// did not extend to a fifth column and gave no way to sort by anything but disagreement).
///
/// `Cmpl` and `Unmarked` are two readings of one number (`App::diff_unmarked`): a yes/no glyph for
/// glancing down the column, and the count itself for ranking how much annotation a fixture still
/// needs. Their filters therefore select exactly the same rows by construction - kept as two
/// columns anyway because their *sorts* differ in the way that matters when triaging: `Cmpl`
/// splits the corpus in two, `Unmarked` orders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DiffColumn {
    #[default]
    Name,
    Dataset,
    Cmpl,
    Unmarked,
    Paint,
    Disagree,
}

impl DiffColumn {
    /// Left-to-right order, shared by the header row, the cursor movement below, and the width
    /// list in `render_open_diff_picker` - so a column can only ever be added in one place.
    const ALL: [DiffColumn; 6] = [
        DiffColumn::Name,
        DiffColumn::Dataset,
        DiffColumn::Cmpl,
        DiffColumn::Unmarked,
        DiffColumn::Paint,
        DiffColumn::Disagree,
    ];

    fn index(self) -> usize {
        DiffColumn::ALL
            .iter()
            .position(|column| *column == self)
            .unwrap_or(0)
    }

    /// Clamped at both ends rather than wrapping: the header row highlights the cursor column, so
    /// a press that jumps from one edge of the table to the other reads as a glitch, not a move.
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
        }
    }

    /// What `f` on this column reads as, per `FlagFilter` state - `None` for the two columns
    /// (`Name`, `Dataset`) that carry their own filter shape instead of a yes/no one.
    fn flag_labels(self) -> Option<(&'static str, &'static str)> {
        match self {
            DiffColumn::Cmpl => Some(("incomplete only", "complete only")),
            DiffColumn::Unmarked => Some(("has unmarked", "none unmarked")),
            DiffColumn::Paint => Some(("painted only", "unpainted only")),
            DiffColumn::Disagree => Some(("disagreements only", "agreeing only")),
            DiffColumn::Name | DiffColumn::Dataset => None,
        }
    }
}

/// The `f` state of one yes/no column: off, or narrowed to the rows where the column's condition
/// does (`Yes`) or does not (`No`) hold. Cycles `Off -> Yes -> No -> Off`.
///
/// **A row whose value is unknown survives either direction.** Every one of these columns is
/// backed by a corpus-wide scan that is only run on demand (see `App::diff_unmarked` and
/// friends), and a fixture the scan couldn't load reads as unknown too - so "unknown" covers both
/// "not measured yet" and "failed to measure", and neither is evidence the row should be dropped.
/// The picker's whole job is surfacing fixtures that need attention; silently hiding the ones it
/// could not measure would hide exactly the wrong rows. This replaces the three separate,
/// individually-argued fail-open rules the `H`/`X`/`Y` filters used to have (one treated a missing
/// entry as "needs attention", one as "unpainted", one as "agrees") with a single rule that reads
/// the same in both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Every column's filter at once. They combine as an AND: a row is shown only if it passes all of
/// them, so narrowing on two columns asks for rows matching both ("still has unmarked nodes AND
/// disagrees with its own painting"), never either.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiffFilters {
    /// Case-insensitive substring of the case name, stored already lowercased (`f` on `Name`
    /// prompts for it - see `Modal::OpenDiffPicker::name_input`). `None` when off; the prompt
    /// never stores an empty string, since that would filter nothing while still reading as on.
    name: Option<String>,
    /// Which of `DIFF_DATASETS` to show, cycled through by `f` on `Dataset` and wrapping back to
    /// `None` ("all") - the filter the old `d` key used to own.
    dataset: Option<&'static str>,
    cmpl: FlagFilter,
    unmarked: FlagFilter,
    paint: FlagFilter,
    disagree: FlagFilter,
}

impl DiffFilters {
    fn flag_mut(&mut self, column: DiffColumn) -> Option<&mut FlagFilter> {
        match column {
            DiffColumn::Cmpl => Some(&mut self.cmpl),
            DiffColumn::Unmarked => Some(&mut self.unmarked),
            DiffColumn::Paint => Some(&mut self.paint),
            DiffColumn::Disagree => Some(&mut self.disagree),
            DiffColumn::Name | DiffColumn::Dataset => None,
        }
    }

    fn flag(&self, column: DiffColumn) -> FlagFilter {
        match column {
            DiffColumn::Cmpl => self.cmpl,
            DiffColumn::Unmarked => self.unmarked,
            DiffColumn::Paint => self.paint,
            DiffColumn::Disagree => self.disagree,
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

    /// One human-readable clause per active filter, for the picker's title bar - empty when
    /// nothing is filtered.
    fn labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        if let Some(name) = &self.name {
            labels.push(format!("name~{name}"));
        }
        if let Some(dataset) = self.dataset {
            labels.push(dataset.to_string());
        }
        for column in DiffColumn::ALL {
            // `Cmpl` and `Unmarked` are one predicate read two ways, so setting both to the same
            // direction narrows nothing further - listing it twice would read as a compound
            // constraint that isn't one. The `Cmpl` wording wins; opposite directions are a real
            // (if empty) compound query and are both shown.
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

/// Which single column the `o` picker's rows are ordered by, and in which direction - `s` on the
/// cursor column takes over the sort (ascending), and `s` again on the column that already owns it
/// flips the direction. Deliberately one column, not a stack: the user asked for the last column
/// selected to be the one that sorts, and a hidden secondary key would make two identical-looking
/// tables order differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The `o` picker's whole cursor/sort/filter state, carried on `Modal::OpenDiffPicker` and
/// persisted on `App::diff_view` so it survives closing and reopening the picker - the same
/// contract the five separate `diff_*` fields this replaces each had on their own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiffPickerView {
    /// The column `h`/`l` last moved to: what `s` and `f` act on, and the one highlighted in the
    /// header row.
    column: DiffColumn,
    sort: DiffSort,
    filters: DiffFilters,
}

/// The three corpus-wide scans the `o` picker's columns read, borrowed rather than owned - `None`
/// for a scan not run yet this session (see `App::diff_unmarked`/`diff_text_painted`/
/// `diff_disagreement` for the lazy-once contract, and `FlagFilter::keeps` for why a `None` here
/// hides nothing). Bundled into one struct because filtering, sorting and rendering all need the
/// same three maps, and threading them as six positional arguments through four functions is how
/// the previous shape of this picker ended up at ten.
#[derive(Clone, Copy, Default)]
struct DiffPickerData<'a> {
    unmarked: Option<&'a HashMap<String, usize>>,
    text_painted: Option<&'a HashMap<String, bool>>,
    disagreement: Option<&'a HashMap<String, usize>>,
}

impl<'a> DiffPickerData<'a> {
    fn from_app(app: &'a App) -> Self {
        DiffPickerData {
            unmarked: app.diff_unmarked.as_ref(),
            text_painted: app.diff_text_painted.as_ref(),
            disagreement: app.diff_disagreement.as_ref(),
        }
    }

    fn unmarked_of(&self, name: &str) -> Option<usize> {
        self.unmarked.and_then(|map| map.get(name)).copied()
    }

    fn painted_of(&self, name: &str) -> Option<bool> {
        self.text_painted.and_then(|map| map.get(name)).copied()
    }

    /// `None` both when the disagreement scan hasn't run and when it ran but the case has no text
    /// painting to compare against at all - the two are the same "not known" for this picker's
    /// purposes, and `compute_diff_disagreement` already leaves the latter out of its map.
    fn disagreement_of(&self, name: &str) -> Option<usize> {
        self.disagreement.and_then(|map| map.get(name)).copied()
    }
}

/// Sort order for the `O` (open sample) picker's list, cycled by pressing `s` while it's open (see
/// `Modal::OpenSamplePicker`). `SmallestDiffFirst`/`LargestDiffFirst` rank by
/// `sample_diff_line_count` - useful for triaging samples by how much work solving one by hand is
/// likely to take, without needing to open each one first to find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleSortOrder {
    Alphabetical,
    ReverseAlphabetical,
    SmallestDiffFirst,
    LargestDiffFirst,
}

impl SampleSortOrder {
    fn next(self) -> Self {
        match self {
            SampleSortOrder::Alphabetical => SampleSortOrder::ReverseAlphabetical,
            SampleSortOrder::ReverseAlphabetical => SampleSortOrder::SmallestDiffFirst,
            SampleSortOrder::SmallestDiffFirst => SampleSortOrder::LargestDiffFirst,
            SampleSortOrder::LargestDiffFirst => SampleSortOrder::Alphabetical,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SampleSortOrder::Alphabetical => "A-Z",
            SampleSortOrder::ReverseAlphabetical => "Z-A",
            SampleSortOrder::SmallestDiffFirst => "smallest diff first",
            SampleSortOrder::LargestDiffFirst => "largest diff first",
        }
    }
}

/// Sample entries actually shown in the `O` picker: `options` filtered by `hide_solved`, then
/// ordered by `sort_order`. Shared by the render function and the key handler so both agree on
/// what index `selected` refers to by construction, rather than keeping two independently
/// maintained copies of the same filter/sort logic in sync by hand.
fn visible_sample_options(
    options: &[(String, SampleTriageStatus, usize)],
    hide_solved: bool,
    sort_order: SampleSortOrder,
) -> Vec<(String, SampleTriageStatus, usize)> {
    let mut visible: Vec<(String, SampleTriageStatus, usize)> = options
        .iter()
        .filter(|(_, status, _)| !hide_solved || !status.is_handled())
        .cloned()
        .collect();

    match sort_order {
        SampleSortOrder::Alphabetical => visible.sort_by(|a, b| a.0.cmp(&b.0)),
        SampleSortOrder::ReverseAlphabetical => visible.sort_by(|a, b| b.0.cmp(&a.0)),
        SampleSortOrder::SmallestDiffFirst => visible.sort_by_key(|(_, _, size)| *size),
        SampleSortOrder::LargestDiffFirst => {
            visible.sort_by_key(|(_, _, size)| std::cmp::Reverse(*size))
        }
    }

    visible
}

/// Builds the `O` picker's modal from a freshly-listed `options`, `current_name` (the case
/// already open, so it starts selected if it's a sample too), and the persisted `hide_solved`/
/// `sort_order` (`App::sample_hide_solved`/`sample_sort_order`). A pure function, separate from
/// the `KeyCode::Char('O')` handler that calls it, specifically so this - the part with real
/// logic to get wrong - is unit-testable without needing real files under
/// `src/test/data/samples/` the way `list_samples_with_status`/`sample_diff_line_count` do.
///
/// `selected` is computed against `visible_sample_options`'s output, not raw `options`: once
/// `hide_solved`/`sort_order` differ from `options`' own natural order (alphabetical, nothing
/// hidden), a raw-`options` position would point at the wrong row once the picker actually renders
/// the filtered/sorted list.
fn open_sample_picker_modal(
    options: Vec<(String, SampleTriageStatus, usize)>,
    current_name: &str,
    hide_solved: bool,
    sort_order: SampleSortOrder,
) -> Modal {
    let visible = visible_sample_options(&options, hide_solved, sort_order);
    let selected = visible
        .iter()
        .position(|(name, ..)| name == current_name)
        .unwrap_or(0)
        .min(visible.len().saturating_sub(1));
    Modal::OpenSamplePicker {
        options,
        selected,
        hide_solved,
        sort_order,
    }
}

// ---------------------------------------------------------------------------------------------

/// The first 8 characters of a full commit hash, for compact display - `git`'s own default abbrev
/// length. `hash` is always a full 40-character SHA here (from `list_repo_commits`'s `%H`), so this
/// never actually needs the `.min()` clamp in practice; it's there so a shorter input can't panic.
fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(8)]
}

/// Every commit in this repository's own history (not a research repo - see `diffs_root`'s
/// sibling `samples_root`, which is what `O` reads from instead), newest first, as
/// `(full hash, subject line)` - the `C` picker's options. Uses `\x1f` (unit separator) rather
/// than a visible character to split the two `git log` fields, since a commit subject can contain
/// almost anything else.
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

/// Paths changed by commit `hash`, narrowed to ones `to_treesitter` can actually parse - picking
/// an unsupported one (e.g. a `.md` or `.toml` file, both of which `language_for_path` happily
/// recognizes but codediff has no grammar for) would just fail at open time with a less helpful
/// error, so it's left out of the list entirely instead. Doesn't pass `-M` (rename detection) to
/// `git diff-tree`: a renamed-with-edits file already shows as one path here (git's default), and
/// a pure rename with no edits showing up as a delete+add pair is an acceptable rough edge for
/// this picker rather than something worth chasing. Also empty (not an error) for a merge commit:
/// `git diff-tree` shows no diff for one by default (needs `-m`/`-c`, neither passed here) - the
/// `C` picker's Enter handler folds this into the same "nothing to pick" message as a genuinely
/// empty/unsupported-only commit, which is close enough not to be worth telling apart.
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

/// The content of `rev_path` (e.g. `"<hash>:<path>"` or `"<hash>^:<path>"`) via the system
/// `git show`, in this repository. Empty, not an error, when `git show` exits non-zero: the one
/// path that matters here is a file that genuinely doesn't exist at that revision (added by the
/// commit being diffed, so it has no "before"; deleted by it, so it has no "after"; or the commit
/// being a root commit, so `<hash>^` doesn't resolve at all) - all three are valid, expected empty
/// sides, exactly like a missing side already means for `load_sample`/`load_case`. A `git` that
/// can't even run is still a real error, surfaced via `.context` on `.output()` below.
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

/// Loads the before/after content for `path` as changed by commit `hash`, read straight out of
/// git (`git_show`) rather than from any on-disk fixture - before is `path` at `hash^`, after is
/// `path` at `hash` itself. Mirrors `load_sample`'s AST checks: bails with a clear message if
/// either side's language has no AST (unsupported or undetected), same as a sample or real case
/// would.
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

/// A starting point for `s`'s promote-name prompt when the current case came from `C` rather than
/// a sample: just `<language>-` (e.g. "rust-"), lowercased - unlike `default_promoted_name`,
/// there's no second repository name to prefix with, since the source *is* this repository.
/// Empty (no dash at all) if `path`'s language can't be determined, which in practice can't
/// happen for a case that actually made it here: `load_git_commit_file` already requires a
/// language with a working `to_treesitter` mapping before this case can be opened at all.
fn default_promoted_name_for_path(path: &str) -> String {
    match language_for_path(Path::new(path)) {
        Some(language) => format!("{}-", language.to_string().to_lowercase()),
        None => String::new(),
    }
}

/// Which of `DIFF_DATASETS` `s`'s promote prompt (`Modal::PromptPromoteName`) would write the
/// current case into - shared by that prompt's own display text and `action_promote`'s actual
/// destination, so the two can never say something different (see the stale hardcoded "small" this
/// replaced: the prompt's text used to name a fixed folder regardless of `source.dataset`).
/// `None` for `CaseOrigin::Diffs`, which never raises this prompt at all (it saves directly via
/// `action_save`) - kept in the match anyway so a fourth origin can't silently fall through here.
fn promote_target_dataset(origin: &CaseOrigin) -> Option<&str> {
    match origin {
        CaseOrigin::Diffs => None,
        CaseOrigin::Sample(source) => Some(source.dataset.as_str()),
        // Always handmade: a case built by hand from this repo's own commits *is* what the
        // handmade dataset is for, unlike a sample, which carries its own recorded provenance.
        CaseOrigin::GitCommitFile { .. } => Some("handmade"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read the theme the `codediff` binary is configured with, from the same `.codediff.toml`, so
    // the two tools paint a diff identically. `set_custom_palette` first, because `OverlayTheme::
    // Custom` resolves its colours from that process-global rather than from the enum - installing
    // the theme without it would leave a custom theme rendering as Dracula.
    theme::set_custom_palette(theme::load_custom_palette());
    let _ = OVERLAY_THEME.set(theme::load_overlay_theme());

    // Loading a case requires an initial, valid AST pair, so when no name is given on the
    // command line, fall back to the first available case rather than restructuring the rest of
    // the app to tolerate no case being loaded at all - press `o` to open a different one.
    let name = match args.name {
        Some(name) => name,
        None => list_available_cases()?
            .into_iter()
            .next()
            .map(|(name, _)| name)
            .ok_or_else(|| anyhow!("No test cases found in src/test/data/diffs"))?,
    };

    let (before, after) = load_case(&name)?;
    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    let mapping = human_mapping::load(&name).unwrap_or_default();

    let mut app = App::new(
        name,
        CaseOrigin::Diffs,
        before_root_id,
        after_root_id,
        mapping,
    );

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
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn validate_new_case_name_rejects_empty() {
        assert!(validate_new_case_name("").is_err());
    }

    #[test]
    fn validate_new_case_name_rejects_leading_digit() {
        assert!(validate_new_case_name("1rust-add-if").is_err());
    }

    #[test]
    fn validate_new_case_name_rejects_unsafe_characters() {
        assert!(validate_new_case_name("rust/add-if").is_err());
        assert!(validate_new_case_name("rust add if").is_err());
        assert!(validate_new_case_name("rust.add.if").is_err());
    }

    #[test]
    fn validate_new_case_name_accepts_letters_digits_hyphen_underscore() {
        assert!(validate_new_case_name("rust-add-if_2").is_ok());
    }

    #[test]
    fn validate_new_case_name_rejects_rust_keywords() {
        assert!(validate_new_case_name("match").is_err());
        assert!(validate_new_case_name("type").is_err());
        assert!(validate_new_case_name("self").is_err());
        // A keyword as a substring of a longer name is fine -- only an exact module-name
        // collision matters.
        assert!(validate_new_case_name("matches-guard").is_ok());
    }

    #[test]
    fn stub_test_contents_has_no_comment_block_when_none_given() {
        let contents = stub_test_contents("rust-add-if", None);
        assert!(!contents.contains("//\n") && !contents.contains("    // "));
        assert!(contents.contains(
            "fn mapping() -> Result<()> {\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
        ));
    }

    #[test]
    fn stub_test_contents_has_no_comment_block_when_comment_is_empty_or_whitespace() {
        assert_eq!(
            stub_test_contents("rust-add-if", Some("")),
            stub_test_contents("rust-add-if", None)
        );
        assert_eq!(
            stub_test_contents("rust-add-if", Some("   \n  ")),
            stub_test_contents("rust-add-if", None)
        );
    }

    #[test]
    fn stub_test_contents_includes_a_wrapped_comment_block_right_before_the_assert() {
        let contents = stub_test_contents("rust-add-if", Some("A short note."));
        assert!(contents.contains(
            "fn mapping() -> Result<()> {\n    // A short note.\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
        ));
    }

    #[test]
    fn wrap_comment_lines_keeps_a_short_comment_on_one_line() {
        assert_eq!(
            wrap_comment_lines("A short note."),
            "    // A short note.\n"
        );
    }

    #[test]
    fn wrap_comment_lines_wraps_long_comments_at_word_boundaries() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen \
                     fourteen fifteen sixteen seventeen eighteen nineteen twenty";
        let wrapped = wrap_comment_lines(long);
        assert!(
            wrapped.lines().count() > 1,
            "expected a comment this long to wrap onto multiple lines"
        );
        for line in wrapped.lines() {
            assert!(
                line.len() <= 96,
                "line exceeds the 96-column wrap width: {:?} ({} chars)",
                line,
                line.len()
            );
            assert!(
                line.starts_with("    // "),
                "line missing the expected prefix: {:?}",
                line
            );
        }
        // Every word survives the wrap, in order, none dropped or duplicated - strip each line's
        // "    // " prefix first so it doesn't get counted as a word of its own.
        let rejoined: Vec<&str> = wrapped
            .lines()
            .flat_map(|line| {
                line.strip_prefix("    // ")
                    .unwrap_or(line)
                    .split_whitespace()
            })
            .collect();
        let original: Vec<&str> = long.split_whitespace().collect();
        assert_eq!(rejoined, original);
    }

    #[test]
    fn wrap_comment_lines_never_splits_a_single_word_even_if_it_exceeds_the_width() {
        let word = "x".repeat(200);
        let wrapped = wrap_comment_lines(&word);
        assert_eq!(wrapped, format!("    // {word}\n"));
    }

    #[test]
    fn is_state_preserving_key_is_true_only_for_typing_in_the_three_text_input_modals() {
        assert!(is_state_preserving_key(
            Some(&Modal::PromptSearch {
                input: String::new()
            }),
            KeyCode::Char('a')
        ));
        assert!(is_state_preserving_key(
            Some(&Modal::PromptSearch {
                input: "x".to_string()
            }),
            KeyCode::Backspace
        ));
        assert!(is_state_preserving_key(
            Some(&Modal::PromptPromoteName {
                input: String::new(),
                error: None
            }),
            KeyCode::Char('a')
        ));
        assert!(is_state_preserving_key(
            Some(&Modal::PromptRejectReason {
                input: String::new(),
                error: None
            }),
            KeyCode::Char('a')
        ));
    }

    #[test]
    fn is_state_preserving_key_is_false_for_enter_esc_on_the_same_three_modals() {
        // Enter/Esc can search-and-move-the-cursor, promote/save, reject, or close the modal --
        // all things that can change what the cached `FrameState` would report, unlike plain
        // typing.
        let search = Some(Modal::PromptSearch {
            input: "x".to_string(),
        });
        assert!(!is_state_preserving_key(search.as_ref(), KeyCode::Enter));
        assert!(!is_state_preserving_key(search.as_ref(), KeyCode::Esc));

        let promote = Some(Modal::PromptPromoteName {
            input: "x".to_string(),
            error: None,
        });
        assert!(!is_state_preserving_key(promote.as_ref(), KeyCode::Enter));
        assert!(!is_state_preserving_key(promote.as_ref(), KeyCode::Esc));

        let reject = Some(Modal::PromptRejectReason {
            input: "x".to_string(),
            error: None,
        });
        assert!(!is_state_preserving_key(reject.as_ref(), KeyCode::Enter));
        assert!(!is_state_preserving_key(reject.as_ref(), KeyCode::Esc));
    }

    #[test]
    fn is_state_preserving_key_is_false_for_every_other_modal_and_for_no_modal_at_all() {
        assert!(!is_state_preserving_key(
            Some(&Modal::Help { scroll: 0 }),
            KeyCode::Char('a')
        ));
        assert!(!is_state_preserving_key(None, KeyCode::Char('a')));
    }

    #[test]
    fn is_state_preserving_key_is_true_for_pure_navigation_and_display_keys_with_no_modal_open() {
        for code in [
            KeyCode::Up,
            KeyCode::Char('k'),
            KeyCode::Down,
            KeyCode::Char('j'),
            KeyCode::Tab,
            KeyCode::Char('g'),
            KeyCode::Char('G'),
            KeyCode::Char('n'),
            KeyCode::Char('N'),
            KeyCode::Char('x'),
            KeyCode::Char('c'),
            KeyCode::Char('p'),
            KeyCode::Char('r'),
            KeyCode::Char('t'),
            KeyCode::Char('T'),
            KeyCode::Char('/'),
            KeyCode::Char('?'),
            KeyCode::Char('q'),
            KeyCode::Esc,
        ] {
            assert!(
                is_state_preserving_key(None, code),
                "{code:?} should not force a FrameState rebuild with no modal open"
            );
        }
    }

    #[test]
    fn is_state_preserving_key_is_false_for_keys_that_can_mutate_mapping_or_collapse_state() {
        // `h`/`l`/`a`/`A` sometimes mutate a collapsed set; `m`/`M`/`f`/`d`/`D`/`i`/`I`/`u` mutate
        // the mapping directly; `H` toggles hide_solved; `s`/`R`/`o`/`O`/`C` are left on the
        // conservative (full-rebuild) path deliberately, per `is_navigation_or_display_key`'s doc
        // comment - none of these must be silently added to the fast path without also auditing
        // what they touch.
        for code in [
            KeyCode::Left,
            KeyCode::Char('h'),
            KeyCode::Right,
            KeyCode::Char('l'),
            KeyCode::Char('a'),
            KeyCode::Char('A'),
            KeyCode::Char('m'),
            KeyCode::Char('M'),
            KeyCode::Char('f'),
            KeyCode::Char('d'),
            KeyCode::Char('D'),
            KeyCode::Char('i'),
            KeyCode::Char('I'),
            KeyCode::Char('u'),
            KeyCode::Char('H'),
            KeyCode::Char('s'),
            KeyCode::Char('R'),
            KeyCode::Char('o'),
            KeyCode::Char('O'),
            KeyCode::Char('C'),
        ] {
            assert!(
                !is_state_preserving_key(None, code),
                "{code:?} must still force a FrameState rebuild with no modal open"
            );
        }
    }

    #[test]
    fn count_unmarked_counts_only_nodes_with_no_match_or_delete_mark() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let (stmt_a, _) = two_statements(root);
        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            None,
        ));

        let mut caches = Caches::default();
        let before_unmarked = count_unmarked(&flat, &caches, status_before);

        // Marking one statement's whole subtree matched must drop the unmarked count by exactly
        // the number of nodes under it, and by nothing else.
        let mut subtree_ids = Vec::new();
        collect_subtree_ids(stmt_a, &mut subtree_ids);
        mark_subtree_matched(stmt_a, &mut caches);

        let after_marking = count_unmarked(&flat, &caches, status_before);
        assert_eq!(before_unmarked - after_marking, subtree_ids.len());
    }

    #[test]
    fn render_panel_only_scans_the_visible_window_not_the_whole_flat_list() {
        // A tree deep enough that only a handful of its nodes fit in a tiny terminal; asserts that
        // `render_panel` never touches (and never renders) anything outside that window, and that
        // the header's count comes from the caller-supplied `total_unmarked`, not a fresh scan.
        let mut source = String::from("fn main() {\n");
        for i in 0..200 {
            source.push_str(&format!("    stmt_{i}();\n"));
        }
        source.push_str("}\n");
        let tree = parse_rust(&source);
        let root = tree.root_node();
        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            None,
        ));
        assert!(
            flat.len() > 200,
            "fixture should be far bigger than any plausible terminal height"
        );

        let caches = Caches::default();
        let mut panel = PanelState::new(root.id());
        // Tall enough to reach past the file's fixed preamble (source_file, function_item, fn,
        // identifier, parameters, (, ), block) down into the first statement's own leaves, but
        // nowhere near tall enough to reach the 199th one.
        let backend = ratatui::backend::TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 40, 20);

        // A deliberately wrong `total_unmarked` -- if `render_panel` still scanned the whole list
        // itself it would recompute (and show) the real count instead of trusting this value.
        terminal
            .draw(|f| {
                render_panel(
                    f,
                    area,
                    "Before",
                    &flat,
                    &mut panel,
                    &caches,
                    Side::Before,
                    source.as_bytes(),
                    true,
                    None,
                    false,
                    424242,
                    &std::collections::BTreeSet::new(),
                )
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("424242 unmarked"),
            "header should show the passed-in count verbatim: {text}"
        );
        assert!(
            text.contains("stmt_0"),
            "the first visible node should render: {text}"
        );
        assert!(
            !text.contains("stmt_199"),
            "a node far past the tiny viewport must not be scanned or rendered: {text}"
        );
    }

    #[test]
    fn sample_source_deserializes_a_legacy_source_json_missing_dataset_as_small() {
        // No "dataset" key at all - the exact shape `materialize_test_diffs` wrote before
        // provenance tracking existed, still sitting in every sample materialized before then.
        let json = r#"{"language":"Rust","repository":"repo","commit":"abc123","path":"src/a.rs"}"#;
        let source: SampleSource = serde_json::from_str(json).unwrap();
        assert_eq!(source.dataset, "small");
    }

    #[test]
    fn sample_source_deserializes_an_explicit_dataset_field() {
        let json = r#"{"language":"Rust","repository":"repo","commit":"abc123","path":"src/a.rs","dataset":"full"}"#;
        let source: SampleSource = serde_json::from_str(json).unwrap();
        assert_eq!(source.dataset, "full");
    }

    #[test]
    fn default_promoted_name_lowercases_language_and_strips_dot_git() {
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "rustdesk-rustdesk.git".to_string(),
            commit: "abc12345".to_string(),
            path: "src/lang/kz.rs".to_string(),
            dataset: "small".to_string(),
        };
        assert_eq!(default_promoted_name(&source), "rust-rustdesk-rustdesk");
    }

    #[test]
    fn default_promoted_name_leaves_a_repository_with_no_dot_git_suffix_alone() {
        let source = SampleSource {
            language: "Kotlin".to_string(),
            repository: "nextcloud-android".to_string(),
            commit: "abc12345".to_string(),
            path: "app/src/main/Foo.kt".to_string(),
            dataset: "small".to_string(),
        };
        assert_eq!(default_promoted_name(&source), "kotlin-nextcloud-android");
    }

    #[test]
    fn default_promoted_name_for_path_lowercases_the_detected_language() {
        assert_eq!(default_promoted_name_for_path("src/diff.rs"), "rust-");
        assert_eq!(default_promoted_name_for_path("scripts/tool.py"), "python-");
    }

    #[test]
    fn default_promoted_name_for_path_is_empty_for_an_undetected_language() {
        // Not a real reachable case in practice (`load_git_commit_file` already requires a
        // working `to_treesitter` mapping before a case can be opened at all), but this should
        // degrade to "no prefix", not panic, if it's ever called on something else.
        assert_eq!(default_promoted_name_for_path("README"), "");
    }

    #[test]
    fn promote_target_dataset_is_none_for_diffs_and_handmade_for_a_git_commit_file() {
        assert_eq!(promote_target_dataset(&CaseOrigin::Diffs), None);
        assert_eq!(
            promote_target_dataset(&CaseOrigin::GitCommitFile {
                path: "src/diff.rs".to_string(),
            }),
            Some("handmade")
        );
    }

    #[test]
    fn promote_target_dataset_is_the_samples_own_recorded_dataset() {
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc12345".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "full".to_string(),
        };
        assert_eq!(
            promote_target_dataset(&CaseOrigin::Sample(source)),
            Some("full")
        );
    }

    #[test]
    fn action_reject_bails_when_current_case_is_not_a_sample() {
        let app = App::new(
            "some-case".to_string(),
            CaseOrigin::Diffs,
            0,
            0,
            HumanMapping::default(),
        );
        let err = action_reject(&app, "not a real reason").unwrap_err();
        assert!(format!("{:#}", err).contains("Only a sample"));
    }

    #[test]
    fn action_reject_bails_on_an_empty_reason() {
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let app = App::new(
            "sample-name".to_string(),
            CaseOrigin::Sample(source),
            0,
            0,
            HumanMapping::default(),
        );
        // Whitespace-only trims to empty, same as a bare empty string would.
        let err = action_reject(&app, "   ").unwrap_err();
        assert!(format!("{:#}", err).contains("cannot be empty"));
    }

    #[test]
    fn short_hash_takes_the_first_eight_characters() {
        assert_eq!(
            short_hash("58a776ecdef0123456789abcdef0123456789ab"),
            "58a776ec"
        );
    }

    #[test]
    fn short_hash_does_not_panic_on_a_shorter_input() {
        assert_eq!(short_hash("abc"), "abc");
    }

    #[test]
    fn advance_to_next_search_match_finds_the_next_leaf_containing_the_query_and_wraps_around() {
        let source = "fn main() {\n    alpha();\n    beta();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            None,
        ));
        let mut panel = PanelState::new(root.id());

        let found =
            advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "beta").unwrap();
        assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "beta");
        assert_eq!(panel.cursor_id, found.id());

        // Searching again from `beta` for a query only `alpha` matches must wrap around past the
        // end of the file back to it, not report "not found" just because it's earlier in the
        // document.
        let found =
            advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "alpha").unwrap();
        assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "alpha");
    }

    #[test]
    fn advance_to_next_search_match_only_matches_leaf_nodes_not_a_containers_concatenated_text() {
        // The `block`'s own text is the concatenation of everything inside it, so it "contains"
        // both "alpha" and "beta" -- but it must never be reported as a match, only the actual
        // leaf tokens should be.
        let source = "fn main() {\n    alpha();\n    beta();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            None,
        ));
        let mut panel = PanelState::new(root.id());

        let found =
            advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "alpha").unwrap();
        assert_eq!(found.child_count(), 0);
        assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "alpha");
    }

    #[test]
    fn advance_to_next_search_match_returns_none_and_leaves_the_cursor_put_when_nothing_matches() {
        let source = "fn main() {\n    alpha();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            None,
        ));
        let mut panel = PanelState::new(root.id());
        let original_cursor = panel.cursor_id;

        let found =
            advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "nonexistent");
        assert!(found.is_none());
        assert_eq!(panel.cursor_id, original_cursor);
    }

    #[test]
    fn action_search_reports_the_matched_nodes_kind_and_moves_the_focused_panels_cursor() {
        let before_source = "fn main() {\n    alpha();\n}\n";
        let after_source = "fn main() {\n    beta();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));

        let msg = action_search(
            &mut app,
            Focus::Before,
            &before_flat,
            &after_flat,
            before_source.as_bytes(),
            after_source.as_bytes(),
            "alpha",
        )
        .unwrap();
        assert!(msg.contains("Found 'alpha'") || msg.contains("Found \"alpha\""));
        assert_eq!(
            before_flat
                .iter()
                .find(|(n, _)| n.id() == app.before.cursor_id)
                .unwrap()
                .0
                .utf8_text(before_source.as_bytes())
                .unwrap(),
            "alpha"
        );
        // The After panel's cursor must be untouched -- the search only ever moves the focused
        // panel's cursor.
        assert_eq!(app.after.cursor_id, after_root.id());
    }

    #[test]
    fn action_search_errors_when_nothing_in_the_focused_panel_matches() {
        let source = "fn main() {\n    alpha();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));

        let err = action_search(
            &mut app,
            Focus::Before,
            &flat,
            &flat,
            source.as_bytes(),
            source.as_bytes(),
            "nonexistent",
        )
        .unwrap_err();
        assert!(format!("{err}").contains("No node containing"));
    }

    #[test]
    fn handle_modal_key_prompt_search_enter_finds_a_match_and_remembers_the_query() {
        let source = "fn main() {\n    alpha();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::PromptSearch {
            input: "alpha".to_string(),
        });

        handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert_eq!(app.last_search.as_deref(), Some("alpha"));
        assert!(
            app.status.as_deref().unwrap_or("").contains("Found"),
            "expected a 'Found' status message, got {:?}",
            app.status
        );
        assert!(app.modal.is_none(), "the modal should close either way");
    }

    #[test]
    fn handle_modal_key_prompt_search_enter_on_empty_input_cancels_without_touching_last_search() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        app.last_search = Some("previous".to_string());
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::PromptSearch {
            input: "   ".to_string(),
        });

        handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert_eq!(
            app.last_search.as_deref(),
            Some("previous"),
            "an empty/whitespace-only query must not overwrite the remembered last search"
        );
        assert_eq!(app.status.as_deref(), Some("Search cancelled: empty query"));
        assert!(app.modal.is_none());
    }

    #[test]
    fn handle_modal_key_prompt_search_esc_cancels_without_searching() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::PromptSearch {
            input: "alpha".to_string(),
        });

        handle_modal_key(
            &mut app,
            KeyCode::Esc,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(app.last_search.is_none());
        assert_eq!(app.status.as_deref(), Some("Cancelled"));
        assert!(app.modal.is_none());
    }

    #[test]
    fn handle_modal_key_prompt_search_backspace_and_char_edit_the_input_and_keep_the_modal_open() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::PromptSearch {
            input: "alp".to_string(),
        });

        handle_modal_key(
            &mut app,
            KeyCode::Backspace,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        match &app.modal {
            Some(Modal::PromptSearch { input }) => assert_eq!(input, "al"),
            other => panic!("expected Modal::PromptSearch to stay open, got {other:?}"),
        }

        handle_modal_key(
            &mut app,
            KeyCode::Char('x'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        match &app.modal {
            Some(Modal::PromptSearch { input }) => assert_eq!(input, "alx"),
            other => panic!("expected Modal::PromptSearch to stay open, got {other:?}"),
        }
    }

    #[test]
    fn render_modal_prompt_search_shows_the_prefilled_query_and_instructions() {
        let backend = ratatui::backend::TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 20);
        let modal = Modal::PromptSearch {
            input: "alpha".to_string(),
        };

        terminal
            .draw(|f| {
                render_modal(
                    f,
                    area,
                    &modal,
                    "test",
                    None,
                    "",
                    "",
                    &HumanMapping::default(),
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    DiffPickerData::default(),
                    None,
                )
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("Search node text"),
            "title missing from render: {text}"
        );
        assert!(text.contains("alpha"), "prefilled query missing: {text}");
        assert!(text.contains("find next"), "instructions missing: {text}");
    }

    #[test]
    fn render_modal_prompt_promote_name_shows_the_actual_target_dataset_not_a_fixed_one() {
        // Regression guard for the bug this replaced: the prompt used to name a hardcoded
        // "small" folder no matter what `promote_dataset` (i.e. `app.origin`) actually was.
        let backend = ratatui::backend::TestBackend::new(90, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 90, 20);
        let modal = Modal::PromptPromoteName {
            input: "rust-".to_string(),
            error: None,
        };

        terminal
            .draw(|f| {
                render_modal(
                    f,
                    area,
                    &modal,
                    "test",
                    Some("handmade"),
                    "",
                    "",
                    &HumanMapping::default(),
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    DiffPickerData::default(),
                    None,
                )
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("diffs/handmade/"),
            "target dataset missing or wrong from render: {text}"
        );
        assert!(
            !text.contains("diffs/small/"),
            "stale dataset in render: {text}"
        );
    }

    #[test]
    fn run_unix_diff_reports_no_differences_for_identical_content() {
        let output = run_unix_diff(b"fn main() {}\n", b"fn main() {}\n").unwrap();
        assert_eq!(output, "(no textual differences)");
    }

    #[test]
    fn run_unix_diff_shows_added_and_removed_lines() {
        let output = run_unix_diff(b"line one\nline two\n", b"line one\nline three\n").unwrap();
        assert!(output.contains("-line two"));
        assert!(output.contains("+line three"));
        assert!(output.contains("--- before"));
        assert!(output.contains("+++ after"));
    }

    #[test]
    fn raw_before_after_reads_the_before_and_after_test_files_from_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("before.rs.test"), "fn old() {}\n").unwrap();
        fs::write(dir.path().join("after.rs.test"), "fn new() {}\n").unwrap();
        fs::write(dir.path().join("source.json"), "{}").unwrap();

        let (before, after) = raw_before_after(dir.path()).expect("both files should be found");
        assert_eq!(before, "fn old() {}\n");
        assert_eq!(after, "fn new() {}\n");
    }

    #[test]
    fn raw_before_after_is_none_when_a_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("before.rs.test"), "fn old() {}\n").unwrap();
        // No after.*.test written.

        assert!(raw_before_after(dir.path()).is_none());
    }

    #[test]
    fn sample_diff_line_count_counts_only_changed_lines_not_context_or_headers() {
        let dir = tempfile::tempdir().unwrap();
        // Same directory layout `sample_diff_line_count` reads via `samples_root().join(name)` -
        // exercised directly against a real temp directory here instead, via `raw_before_after` +
        // `run_unix_diff` (the same two calls `sample_diff_line_count` itself makes), since
        // `samples_root()` is hardcoded to this crate's own `src/test/data/samples`.
        fs::write(
            dir.path().join("before.rs.test"),
            "fn main() {\n    old();\n    same();\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("after.rs.test"),
            "fn main() {\n    new();\n    same();\n}\n",
        )
        .unwrap();

        let (before, after) = raw_before_after(dir.path()).unwrap();
        let diff = run_unix_diff(before.as_bytes(), after.as_bytes()).unwrap();
        let count = diff
            .lines()
            .filter(|line| {
                (line.starts_with('+') && !line.starts_with("+++"))
                    || (line.starts_with('-') && !line.starts_with("---"))
            })
            .count();
        assert_eq!(
            count, 2,
            "one removed + one added line, not the 2 unchanged: {diff}"
        );
    }

    #[test]
    fn sample_diff_line_count_is_zero_for_a_nonexistent_sample() {
        assert_eq!(sample_diff_line_count("does-not-exist-at-all"), 0);
    }

    #[test]
    fn sample_diff_line_count_is_nonzero_for_a_real_sample_on_disk() {
        // Full integrated path (`samples_root().join(name)`, not a crafted temp dir) against
        // whatever's actually checked in under src/test/data/samples/ - skips rather than fails if
        // none exist yet in this checkout (materialize_test_diffs hasn't run), matching
        // `list_dir_names`'s own "not an error" treatment of a missing/empty samples/ directory.
        let Ok(names) = list_dir_names(&samples_root()) else {
            return;
        };
        let Some(name) = names.first() else {
            return;
        };
        assert!(
            sample_diff_line_count(name) > 0,
            "sample '{name}' should have at least one changed line"
        );
    }

    /// Concatenates every cell's symbol in a `TestBackend`'s buffer, so a rendered frame's
    /// content can be checked with a plain `contains`.
    /// Builds a view with the cursor on `column`, everything else default.
    fn column_view(column: DiffColumn) -> DiffPickerView {
        DiffPickerView {
            column,
            ..DiffPickerView::default()
        }
    }

    /// Builds a view sorted ascending by `column`, everything else default.
    fn sort_view(column: DiffColumn) -> DiffPickerView {
        DiffPickerView {
            sort: DiffSort::default().toggled(column),
            ..DiffPickerView::default()
        }
    }

    /// Builds a view with only the `Dataset` filter set.
    fn dataset_view(dataset: Option<&'static str>) -> DiffPickerView {
        DiffPickerView {
            filters: DiffFilters {
                dataset,
                ..DiffFilters::default()
            },
            ..DiffPickerView::default()
        }
    }

    /// Builds a view with only the `Name` substring filter set.
    fn name_view(needle: &str) -> DiffPickerView {
        DiffPickerView {
            filters: DiffFilters {
                name: Some(needle.to_string()),
                ..DiffFilters::default()
            },
            ..DiffPickerView::default()
        }
    }

    /// Builds a view with one flag filter set, everything else default.
    fn flag_view(column: DiffColumn, filter: FlagFilter) -> DiffPickerView {
        let mut view = DiffPickerView::default();
        *view
            .filters
            .flag_mut(column)
            .expect("column has a flag filter") = filter;
        view
    }

    /// The `Paint` column's filter narrows in both directions, and a case the scan never reached
    /// survives *either* of them - see `FlagFilter::keeps` for why that fail-open rule is uniform
    /// across columns and directions rather than argued per filter.
    #[test]
    fn visible_diff_options_can_narrow_to_painted_or_unpainted_cases() {
        let options = vec![
            ("painted".to_string(), "handmade"),
            ("unpainted".to_string(), "handmade"),
            ("never-scanned".to_string(), "handmade"),
        ];
        let painted = std::collections::HashMap::from([
            ("painted".to_string(), true),
            ("unpainted".to_string(), false),
        ]);
        let data = DiffPickerData {
            text_painted: Some(&painted),
            ..DiffPickerData::default()
        };

        assert_eq!(
            visible_diff_options(&options, &DiffPickerView::default(), data),
            vec!["never-scanned", "painted", "unpainted"],
            "the filter off shows everything"
        );
        assert_eq!(
            visible_diff_options(
                &options,
                &flag_view(DiffColumn::Paint, FlagFilter::No),
                data
            ),
            vec!["never-scanned", "unpainted"],
            "'unpainted only' hides only cases confirmed painted"
        );
        assert_eq!(
            visible_diff_options(
                &options,
                &flag_view(DiffColumn::Paint, FlagFilter::Yes),
                data
            ),
            vec!["never-scanned", "painted"],
            "'painted only' hides only cases confirmed unpainted"
        );
    }

    /// Two columns' filters are separate queues over separate ground truths, so turning both on
    /// shows only what matches both - the AND the picker's whole compound-query story rests on.
    #[test]
    fn filters_on_different_columns_combine_as_an_and() {
        let options = vec![
            ("both".to_string(), "handmade"),
            ("tree-only".to_string(), "handmade"),
            ("text-only".to_string(), "handmade"),
            ("done".to_string(), "handmade"),
        ];
        let unmarked = std::collections::HashMap::from([
            ("both".to_string(), 3),
            ("tree-only".to_string(), 7),
            ("text-only".to_string(), 0),
            ("done".to_string(), 0),
        ]);
        let painted = std::collections::HashMap::from([
            ("both".to_string(), false),
            ("tree-only".to_string(), true),
            ("text-only".to_string(), false),
            ("done".to_string(), true),
        ]);
        let data = DiffPickerData {
            unmarked: Some(&unmarked),
            text_painted: Some(&painted),
            ..DiffPickerData::default()
        };

        let mut view = flag_view(DiffColumn::Cmpl, FlagFilter::Yes);
        view.filters.paint = FlagFilter::No;

        assert_eq!(
            visible_diff_options(&options, &view, data),
            vec!["both"],
            "only the case needing both an unmarked node and a painting survives"
        );
    }

    /// `s` on a column takes the sort over and orders by it; pressing it again flips the
    /// direction. The name is always the tiebreak, so rows that tie on the sorted column - every
    /// row, when its scan hasn't run - still come out alphabetically rather than in whatever
    /// order `options` happened to arrive in.
    #[test]
    fn visible_diff_options_sorts_by_the_selected_column_with_a_name_tiebreak() {
        let options = vec![
            ("charlie".to_string(), "handmade"),
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
            ("delta".to_string(), "handmade"),
        ];
        // `delta` is deliberately absent: an unscanned/unloadable case sorts after every known
        // one ascending, and to the front when the direction flips.
        let unmarked = std::collections::HashMap::from([
            ("charlie".to_string(), 1),
            ("alpha".to_string(), 9),
            ("bravo".to_string(), 1),
        ]);
        let data = DiffPickerData {
            unmarked: Some(&unmarked),
            ..DiffPickerData::default()
        };

        let mut view = sort_view(DiffColumn::Unmarked);
        assert_eq!(
            visible_diff_options(&options, &view, data),
            vec!["bravo", "charlie", "alpha", "delta"],
            "ascending by count, ties broken by name, unknown last"
        );

        view.sort = view.sort.toggled(DiffColumn::Unmarked);
        assert_eq!(
            visible_diff_options(&options, &view, data),
            vec!["delta", "alpha", "bravo", "charlie"],
            "pressing s again reverses the column, but ties still break by name ascending"
        );

        let name_sorted = DiffPickerView::default();
        assert_eq!(
            visible_diff_options(&options, &name_sorted, data),
            vec!["alpha", "bravo", "charlie", "delta"],
            "the default sort is the Name column, A-Z"
        );
    }

    /// The `Name` filter is a case-insensitive substring over the case name, and is the one filter
    /// that needs no corpus scan at all.
    #[test]
    fn visible_diff_options_narrows_by_a_name_substring() {
        let options = vec![
            ("rust-add-if".to_string(), "handmade"),
            ("java-add-exception".to_string(), "small"),
            ("rust-hash".to_string(), "handmade"),
        ];
        let view = name_view("rust-");

        assert_eq!(
            visible_diff_options(&options, &view, DiffPickerData::default()),
            vec!["rust-add-if", "rust-hash"]
        );
    }

    /// The picker offers this fixture's own paintings first, then whichever suggestions it lacks -
    /// so resaving the painting being edited is at the top, not buried under three constants.
    #[test]
    fn the_solution_picker_lists_existing_paintings_before_suggestions() {
        let mut app = test_app();
        app.mapping.text_mappings = vec![NamedTextMapping {
            name: "Full".to_string(),
            mapping: HumanTextMapping::default(),
        }];

        assert_eq!(
            solution_picker_names(&app.mapping),
            vec!["Full", "Minimal", "Only one solution"],
            "an existing name must not be offered twice"
        );
    }

    /// Presses one key on the main view of a case with `origin`, and hands back the App.
    ///
    /// Drives `handle_key`'s dispatch rather than `action_comment`, for the reason the solution
    /// picker's harness gives: `x` and `c` were once implemented, unit-tested through their
    /// actions, and unreachable because the key arm was never written.
    fn press_on_case(origin: CaseOrigin, name: &str, code: KeyCode) -> App {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            name.to_string(),
            origin,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        let hashes = rustc_hash::FxHashMap::default();
        handle_key(
            &mut app,
            code,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &hashes,
            &hashes,
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        app
    }

    /// `e` on a diff opens the comment prompt. It used to refuse - comments lived only in
    /// sample.csv, which a handmade fixture has no row in at all.
    #[test]
    fn e_on_a_diff_opens_the_comment_prompt() {
        let app = press_on_case(CaseOrigin::Diffs, "rust-no-change", KeyCode::Char('e'));
        match &app.modal {
            Some(Modal::PromptComment { input, .. }) => assert!(
                input.contains("identical"),
                "must pre-fill from the existing description.md, got {input:?}"
            ),
            other => panic!("expected Modal::PromptComment, got {other:?}"),
        }
    }

    /// A diff with no description.md yet still gets the prompt - just an empty one.
    #[test]
    fn e_on_an_un_noted_diff_opens_an_empty_prompt() {
        let app = press_on_case(
            CaseOrigin::Diffs,
            "rust-hello-world-added-message",
            KeyCode::Char('e'),
        );
        match &app.modal {
            Some(Modal::PromptComment { input, .. }) => assert_eq!(input, ""),
            other => panic!("expected Modal::PromptComment, got {other:?}"),
        }
    }

    /// A case that is neither a diff nor a sample - `C`'s git-commit view - still has nowhere to
    /// put a comment, and says so rather than opening a prompt that could not be saved.
    #[test]
    fn e_on_a_git_commit_case_refuses_with_a_message() {
        let app = press_on_case(
            CaseOrigin::GitCommitFile {
                path: "src/main.rs".to_string(),
            },
            "abc1234",
            KeyCode::Char('e'),
        );
        assert!(app.modal.is_none(), "no prompt should open");
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("diff (o) or sample (O)"),
            "{:?}",
            app.status
        );
    }

    /// **A promoted fixture's note lives in its own `description.md`, and nowhere else.**
    ///
    /// Stronger than the rule this replaced, which only asked that a sample.csv comment also have
    /// a file - that allowed two copies, and two copies drift: of the 19 promoted rows that
    /// carried a comment, 18 matched their `description.md` byte for byte and one had already
    /// diverged. Promotion now *moves* the note (see `update_sample_csv_at`) rather than copying
    /// it, so the correct state is no comment at all on a promoted row, and that is what this
    /// checks.
    ///
    /// A rejected or still-untriaged row keeps its comment: there is no fixture directory to put
    /// it in, and for a rejection the reason is the only record of the decision.
    #[test]
    fn no_promoted_row_carries_a_comment() {
        let rows = read_sample_csv_rows(&sample_csv_path()).expect("sample.csv");
        let duplicated: Vec<&str> = rows
            .iter()
            .filter(|row| !row.promoted_to.trim().is_empty() && !row.comment.trim().is_empty())
            .map(|row| row.promoted_to.as_str())
            .collect();
        assert!(
            duplicated.is_empty(),
            "these promoted fixtures still carry a sample.csv comment, a second home for a note \
             that belongs only in their own description.md: {duplicated:?}"
        );
    }

    /// Opens the solution picker over a fixture holding `paintings`, and presses `keys` in it.
    ///
    /// Drives `handle_modal_key` rather than `action_delete_solution`, because that is the layer
    /// where this feature has already failed once: `x` and `c` were implemented, unit-tested
    /// through their actions, and unreachable for a week because the key arm was never added.
    fn press_in_solution_picker(paintings: &[&str], keys: &[KeyCode]) -> App {
        press_in_solution_picker_editing(paintings, None, keys)
    }

    /// As above, but editing `editing` rather than whichever painting `starting_solution` picks -
    /// the only way to reach the case where the deleted painting is *not* the current one.
    fn press_in_solution_picker_editing(
        paintings: &[&str],
        editing: Option<&str>,
        keys: &[KeyCode],
    ) -> App {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = test_app();
        app.mapping.text_mappings = paintings
            .iter()
            .map(|name| NamedTextMapping {
                name: (*name).to_string(),
                mapping: HumanTextMapping::default(),
            })
            .collect();
        app.text_solution = editing
            .map(str::to_string)
            .unwrap_or_else(|| starting_solution(&app.mapping));

        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::SolutionPicker {
            names: solution_picker_names(&app.mapping),
            selected: 0,
            saving: false,
            new_name: None,
            confirm_delete: None,
            state: TextPaintState::default(),
        });

        for &code in keys {
            handle_modal_key(
                &mut app,
                code,
                &flat,
                &flat,
                root,
                root,
                &caches,
                source.as_bytes(),
                source.as_bytes(),
                &Code::from_string(source, &Language::Rust),
                &Code::from_string(source, &Language::Rust),
            );
        }
        app
    }

    fn painting_names(app: &App) -> Vec<String> {
        app.mapping
            .text_mappings
            .iter()
            .map(|named| named.name.clone())
            .collect()
    }

    /// One `D` arms, the second fires. There is no undo for a painting that may have taken an
    /// hour, so a single keystroke next to `d` (delete-range) must not be able to destroy one.
    #[test]
    fn deleting_a_painting_takes_two_presses_of_d() {
        let armed = press_in_solution_picker(&["Full", "Minimal"], &[KeyCode::Char('D')]);
        assert_eq!(
            painting_names(&armed),
            vec!["Full", "Minimal"],
            "one D must only arm the confirmation"
        );
        match &armed.modal {
            Some(Modal::SolutionPicker { confirm_delete, .. }) => {
                assert_eq!(confirm_delete.as_deref(), Some("Full"))
            }
            other => panic!("expected the picker to stay open, got {other:?}"),
        }

        let deleted = press_in_solution_picker(
            &["Full", "Minimal"],
            &[KeyCode::Char('D'), KeyCode::Char('D')],
        );
        assert_eq!(painting_names(&deleted), vec!["Minimal"]);
    }

    /// The case this exists for: a fixture painted once turns out to need a Minimal and a Full
    /// answer, so the single painting has to go before the pair can be made.
    #[test]
    fn deleting_the_last_painting_leaves_the_fixture_unpainted() {
        let app = press_in_solution_picker(
            &["Only one solution"],
            &[KeyCode::Char('D'), KeyCode::Char('D')],
        );

        assert!(
            app.mapping.text_mappings.is_empty(),
            "the fixture goes back to unpainted, not to an empty painting - those are different \
             states, and only the first is what `X` and diffs.csv should report"
        );
        assert!(app.dirty, "the deletion still has to be saved");
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("unpainted"),
            "the reader has to be told the fixture is now unpainted: {:?}",
            app.status
        );
    }

    /// The confirmation is about a painting, not about a row. Moving the cursor between the two
    /// presses must not let the second `D` land on whatever ended up highlighted instead.
    #[test]
    fn moving_the_cursor_cancels_a_pending_deletion() {
        let app = press_in_solution_picker(
            &["Full", "Minimal"],
            &[KeyCode::Char('D'), KeyCode::Char('j'), KeyCode::Char('D')],
        );

        assert_eq!(
            painting_names(&app),
            vec!["Full", "Minimal"],
            "j cleared the confirmation, so the second D only re-armed - on Minimal"
        );
        match &app.modal {
            Some(Modal::SolutionPicker { confirm_delete, .. }) => {
                assert_eq!(confirm_delete.as_deref(), Some("Minimal"))
            }
            other => panic!("expected the picker to stay open, got {other:?}"),
        }
    }

    /// Deleting a painting you are not editing must not move you. The harness's default always
    /// deletes the current one (the picker lists it first), so this is the case the other tests
    /// structurally cannot reach.
    #[test]
    fn deleting_another_painting_leaves_you_editing_your_own() {
        // Three paintings, not two: `starting_solution` returns the *first* survivor, so with two
        // it happens to return the one being edited anyway and an unguarded reset would look
        // correct. Deleting "Full" while editing "Custom" leaves "Minimal" first, so the two
        // behaviours finally differ.
        let app = press_in_solution_picker_editing(
            &["Full", "Minimal", "Custom"],
            Some("Custom"),
            &[KeyCode::Char('D'), KeyCode::Char('D')],
        );

        assert_eq!(painting_names(&app), vec!["Minimal", "Custom"]);
        assert_eq!(
            app.text_solution, "Custom",
            "deleting 'Full' has nothing to do with which painting the reader is in"
        );
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("still editing"),
            "{:?}",
            app.status
        );
    }

    /// The picker lists suggestions the fixture has not used yet alongside its real paintings.
    /// Pressing `D` on one of those is a misunderstanding, not a request, and must not silently
    /// look like it worked.
    #[test]
    fn d_on_an_unused_suggestion_deletes_nothing() {
        // "Full" exists; "Minimal" and "Only one solution" are offered but unused, so j lands on a
        // suggestion.
        let app = press_in_solution_picker(
            &["Full"],
            &[KeyCode::Char('j'), KeyCode::Char('D'), KeyCode::Char('D')],
        );

        assert_eq!(painting_names(&app), vec!["Full"]);
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("only a suggestion"),
            "saying so beats an armed confirmation for something that cannot be deleted: {:?}",
            app.status
        );
        assert!(!app.dirty, "nothing changed, so nothing needs saving");
    }

    /// The property this whole feature exists for: a fixture can hold more than one painting at
    /// once. An earlier version *moved* the ranges to the new name and dropped the old one, so
    /// however many times you saved you still ended up with exactly one.
    #[test]
    fn branching_keeps_both_paintings_on_file() {
        let (before_src, after_src) = ("gone\n", "\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 3);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );
        assert_eq!(app.text_solution, "Minimal");

        action_save_solution_as(&mut app, "Full", true);

        assert_eq!(
            app.text_solution, "Full",
            "editing continues on the new one"
        );
        assert_eq!(
            app.mapping.text_mappings.len(),
            2,
            "both paintings must survive: {:?}",
            app.mapping.text_mappings
        );
        assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
        assert_eq!(
            solution_entries(&app.mapping, "Full").len(),
            1,
            "a copy starts from what was painted"
        );
    }

    /// Two answers to one edit usually share most of their spans, so `e` (start empty) is the
    /// rarer option and Enter copies - but both have to be reachable.
    #[test]
    fn branching_empty_starts_the_new_painting_from_nothing() {
        let (before_src, after_src) = ("gone\n", "\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 3);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );

        action_save_solution_as(&mut app, "Full", false);

        assert_eq!(app.mapping.text_mappings.len(), 2);
        assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
        assert!(solution_entries(&app.mapping, "Full").is_empty());
    }

    /// Picking a name that already exists must never write: merging would leave overlapping
    /// duplicates, replacing would silently discard somebody's work, and there is no undo here.
    #[test]
    fn branching_to_an_existing_name_switches_without_overwriting_it() {
        let mut app = test_app();
        app.mapping.text_mappings = vec![
            NamedTextMapping {
                name: "Minimal".to_string(),
                mapping: HumanTextMapping::default(),
            },
            NamedTextMapping {
                name: "Full".to_string(),
                mapping: HumanTextMapping {
                    entries: vec![HumanTextEntry {
                        operation: HumanTextOperation::Delete,
                        before: vec![HumanTextSpan {
                            start_row: 0,
                            start_column: 0,
                            end_row: 0,
                            end_column: 4,
                        }],
                        after: vec![],
                    }],
                },
            },
        ];

        action_save_solution_as(&mut app, "Full", true);

        assert_eq!(app.text_solution, "Full");
        assert_eq!(app.mapping.text_mappings.len(), 2);
        assert_eq!(
            solution_entries(&app.mapping, "Full").len(),
            1,
            "the existing painting must be untouched, not merged into or replaced"
        );
        assert!(
            app.status
                .as_deref()
                .unwrap_or("")
                .contains("already exists"),
            "the reader has to be told nothing was written: {:?}",
            app.status
        );
    }

    #[test]
    fn loading_switches_which_painting_is_edited_without_touching_any() {
        let mut app = test_app();
        app.mapping.text_mappings = vec![
            NamedTextMapping {
                name: "Minimal".to_string(),
                mapping: HumanTextMapping::default(),
            },
            NamedTextMapping {
                name: "Full".to_string(),
                mapping: HumanTextMapping::default(),
            },
        ];

        action_load_solution(&mut app, "Full");

        assert_eq!(app.text_solution, "Full");
        assert_eq!(app.mapping.text_mappings.len(), 2);
        assert!(!app.dirty, "switching which one you edit changes nothing");
    }

    /// Two paintings must not bleed into each other: painting under one name and switching leaves
    /// the other empty.
    #[test]
    fn ranges_painted_under_one_name_stay_out_of_another() {
        let (before_src, after_src) = ("gone\n", "\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 3);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );

        action_load_solution(&mut app, "Full");

        assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
        assert!(solution_entries(&app.mapping, "Full").is_empty());
    }

    /// The overlay cycles through all four and back, so `p` is always the only key needed.
    #[test]
    fn the_text_overlay_cycles_human_codediff_disagreements_tree_disagreement() {
        assert_eq!(TextOverlay::default(), TextOverlay::Human);
        assert_eq!(TextOverlay::Human.next(), TextOverlay::CodeDiff);
        assert_eq!(TextOverlay::CodeDiff.next(), TextOverlay::Disagreements);
        assert_eq!(
            TextOverlay::Disagreements.next(),
            TextOverlay::TreeDisagreement
        );
        assert_eq!(TextOverlay::TreeDisagreement.next(), TextOverlay::Human);
    }

    /// codediff's own rendering comes through `TextDiff`, the projection the real TUI and the
    /// mapping site both draw - so what the solver shows is what codediff produces, not a second
    /// reading of its node mapping.
    #[test]
    fn codediff_text_spans_reports_the_changed_regions_of_a_real_diff() {
        let before = Code::from_string("fn main() {\n    foo();\n}\n", &Language::Rust);
        let after = Code::from_string("fn main() {\n    bar();\n}\n", &Language::Rust);

        let [before_spans, after_spans] = codediff_text_spans(&before, &after);

        assert!(
            !before_spans.is_empty(),
            "a renamed call must show as changed"
        );
        assert!(!after_spans.is_empty());
        // Identical text contributes nothing: the untouched `fn main() {` line is not painted.
        assert!(
            before_spans.iter().all(|(span, _)| span.start_row > 0),
            "row 0 is unchanged and should carry no span: {before_spans:?}"
        );
    }

    /// The disagreement overlay is empty exactly when the two accounts agree, which is the signal
    /// a person painting ground truth is actually looking for.
    #[test]
    fn the_disagreement_overlay_is_empty_when_the_two_accounts_match() {
        let (before_src, after_src) = ("alpha\n", "beta\n");
        let spans = [
            vec![(
                HumanTextSpan {
                    start_row: 0,
                    start_column: 0,
                    end_row: 0,
                    end_column: 5,
                },
                HumanTextVerdict::Update,
            )],
            vec![(
                HumanTextSpan {
                    start_row: 0,
                    start_column: 0,
                    end_row: 0,
                    end_column: 4,
                },
                HumanTextVerdict::Update,
            )],
        ];

        let same = overlay_disagreement_spans(&spans, &spans, before_src, after_src);
        assert!(
            same[0].is_empty() && same[1].is_empty(),
            "identical accounts must show nothing: {same:?}"
        );

        let empty = [Vec::new(), Vec::new()];
        let differing = overlay_disagreement_spans(&spans, &empty, before_src, after_src);
        assert!(
            !differing[0].is_empty(),
            "text one side calls changed and the other does not must show"
        );
    }

    /// The painted ranges use the shared overlay palette, not hardcoded ANSI colours - so a range
    /// looks the same here as the same range does in the `codediff` TUI.
    #[test]
    fn painted_ranges_use_the_shared_overlay_palette() {
        let palette = OverlayTheme::default().palette();

        assert_eq!(
            verdict_style(HumanTextVerdict::Move).bg,
            Some(palette.move_bg)
        );
        assert_eq!(
            verdict_style(HumanTextVerdict::Update).bg,
            Some(palette.update_bg)
        );
        assert_eq!(
            verdict_style(HumanTextVerdict::Delete).bg,
            Some(palette.delete_bg)
        );
        assert_eq!(
            verdict_style(HumanTextVerdict::Insert).bg,
            Some(palette.insert_bg)
        );
        assert_eq!(
            verdict_style(HumanTextVerdict::Move).fg,
            Some(palette.overlay_fg)
        );
    }

    /// A multi-row span's middle row is fully covered up to its last real character, but never
    /// past it - a human painting never means to paint a line's trailing whitespace, and least of
    /// all its newline.
    #[test]
    fn span_covers_stops_at_the_last_real_character_of_a_middle_row() {
        let span = HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 2,
            end_column: 1,
        };
        let row_len = 3; // row 1 is "foo"

        assert!(
            span_covers(span, 1, 2, row_len),
            "the last real character must still be covered"
        );
        assert!(
            !span_covers(span, 1, 3, row_len),
            "the position past the last character is the newline - must not be covered"
        );
    }

    /// A blank row caught in the middle of a multi-row span has no character of its own, so it is
    /// still reported as covered at column 0 - `render_paint_side`'s one-space fallback is what
    /// makes that visible, and it needs `span_covers` to say the row belongs to the span at all.
    #[test]
    fn span_covers_still_covers_a_blank_middle_row() {
        let span = HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 2,
            end_column: 1,
        };

        assert!(span_covers(span, 1, 0, 0));
    }

    /// The span's own end row is unaffected either way - `end_column` already says exactly where
    /// the human stopped painting.
    #[test]
    fn span_covers_uses_the_exact_end_column_on_the_last_row() {
        let span = HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        };

        assert!(span_covers(span, 0, 2, 5));
        assert!(!span_covers(span, 0, 3, 5));
    }

    /// Unset means Dracula, which is what makes these render tests deterministic rather than
    /// dependent on whatever `.codediff.toml` the machine running them happens to hold.
    #[test]
    fn the_palette_falls_back_to_the_default_theme_when_none_was_installed() {
        assert_eq!(
            overlay_palette().move_bg,
            OverlayTheme::Dracula.palette().move_bg
        );
    }

    /// A live selection is drawn in the same colour the TUI paints a cursor's counterpart: both
    /// mean "the region you are pointing at".
    #[test]
    fn a_selection_uses_the_cross_panel_highlight_colour() {
        assert_eq!(
            paint_class_style(PaintClass::Selected).bg,
            Some(OverlayTheme::default().palette().cross_highlight_bg)
        );
    }

    /// The shape this exists for: several ranges banked on each side commit as ONE match, so three
    /// occurrences before against two after is a single correspondence rather than five.
    #[test]
    fn x_banks_ranges_so_m_can_commit_an_n_to_m_match() {
        let (before_src, after_src) = ("foo\nfoo\nfoo\n", "bar\nbar\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();

        // Two banked before-ranges plus a live third; two live/banked after-ranges.
        state.pending[0] = vec![
            HumanTextSpan {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 3,
            },
            HumanTextSpan {
                start_row: 1,
                start_column: 0,
                end_row: 1,
                end_column: 3,
            },
        ];
        state.anchor[0] = Some((2, 0));
        state.cursor[0] = (2, 2);
        state.pending[1] = vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        }];
        state.anchor[1] = Some((1, 0));
        state.cursor[1] = (1, 2);

        action_paint_match(&mut app, &mut state, before_src, after_src);

        let entries = solution_entries(&app.mapping, &app.text_solution);
        assert_eq!(entries.len(), 1, "one entry, not five");
        assert_eq!(entries[0].before.len(), 3);
        assert_eq!(entries[0].after.len(), 2);
        assert_eq!(
            entries[0].verdict(before_src, after_src).unwrap(),
            HumanTextVerdict::Update,
            "foo against bar is an edit"
        );
        assert_eq!(state.pending, [vec![], vec![]], "the bank is consumed");
        assert!(
            app.status.as_deref().unwrap_or("").contains("3:2"),
            "the shape should be reported: {:?}",
            app.status
        );
    }

    /// The live selection commits along with the bank, so forgetting the final `x` before `m`
    /// doesn't silently drop a range - a loss this view has no undo for.
    #[test]
    fn the_live_selection_commits_together_with_banked_ranges() {
        let source = "foo\nfoo\n";
        let mut state = TextPaintState::default();
        state.pending[0] = vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        }];
        state.anchor[0] = Some((1, 0));
        state.cursor[0] = (1, 2);

        assert_eq!(state.committable(0, source).len(), 2);
    }

    /// A group whose spans disagree within one side is refused at the keystroke, while the
    /// selection is still on screen to fix - not silently stored to fail at save time.
    #[test]
    fn m_refuses_a_group_whose_spans_differ_within_a_side() {
        let (before_src, after_src) = ("foo\nqux\n", "bar\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.pending[0] = vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        }];
        state.anchor[0] = Some((1, 0));
        state.cursor[0] = (1, 2);
        state.anchor[1] = Some((0, 0));
        state.cursor[1] = (0, 2);

        action_paint_match(&mut app, &mut state, before_src, after_src);

        assert!(
            app.mapping.text_mappings.is_empty(),
            "nothing should have been stored"
        );
        assert!(!app.dirty);
        assert!(
            app.status
                .as_deref()
                .unwrap_or("")
                .contains("identical text"),
            "the reason has to reach the human: {:?}",
            app.status
        );
        assert_eq!(
            state.pending[0].len(),
            1,
            "the selection must survive so it can be corrected"
        );
    }

    #[test]
    fn d_paints_every_banked_range_as_one_entry() {
        let (before_src, after_src) = ("foo\nbar\n", "\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.pending[0] = vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        }];
        state.anchor[0] = Some((1, 0));
        state.cursor[0] = (1, 2);

        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );

        let entries = solution_entries(&app.mapping, &app.text_solution);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].before.len(),
            2,
            "one decision covering both ranges, not two decisions"
        );
    }

    /// Drives the text view through `handle_modal_key` rather than calling the actions directly,
    /// because the bug this pins was a missing *key binding*, not a broken action: `x` banked
    /// nothing because its match arm was never wired into the text view at all, while every test
    /// that called `action_paint_match` with a pre-filled bank kept passing.
    #[test]
    fn x_in_the_text_view_banks_the_live_selection() {
        let source = "foo\nfoo\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 2);
        app.modal = Some(Modal::TextView { state });

        handle_modal_key(
            &mut app,
            KeyCode::Char('x'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        let Some(Modal::TextView { state }) = &app.modal else {
            panic!("the text view should still be open, got {:?}", app.modal);
        };
        assert_eq!(state.pending[0].len(), 1, "x must bank the selection");
        assert!(
            state.anchor[0].is_none(),
            "and clear it, so the next v starts a fresh range"
        );
        assert!(
            app.status.as_deref().unwrap_or("").contains("Banked"),
            "got {:?}",
            app.status
        );
    }

    /// `c` is the other half of the same pair, and was missing alongside `x`.
    #[test]
    fn c_in_the_text_view_clears_both_sides_banks() {
        let source = "foo\nfoo\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        let mut state = TextPaintState::default();
        state.pending[0] = vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        }];
        state.pending[1] = state.pending[0].clone();
        app.modal = Some(Modal::TextView { state });

        handle_modal_key(
            &mut app,
            KeyCode::Char('c'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        let Some(Modal::TextView { state }) = &app.modal else {
            panic!("the text view should still be open, got {:?}", app.modal);
        };
        assert!(state.pending[0].is_empty() && state.pending[1].is_empty());
    }

    /// `diff -u` reports positions only in its hunk headers; the gutter turns counting into
    /// reading. A deleted line has no after-side number and an inserted one has no before-side
    /// number - the blank half is the point, not an omission.
    #[test]
    fn the_unix_diff_view_numbers_both_sides() {
        let backend = ratatui::backend::TestBackend::new(110, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 110, 16);
        let output = "--- a\n+++ b\n@@ -10,3 +20,3 @@\n context\n-gone\n+new\n";

        terminal
            .draw(|f| render_unix_diff_modal(f, area, output, 0))
            .unwrap();

        // Asserted per row on collapsed whitespace, not on exact column padding: the gutter's
        // field width is a layout choice, and pinning it would make this test fail for a cosmetic
        // change while saying nothing about the numbering it exists to check.
        let rows: Vec<String> = rendered_text(&terminal)
            .split('\u{2502}')
            .map(|row| row.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        let has = |wanted: &str| rows.iter().any(|row| row.contains(wanted));

        // The context line carries both counters, starting at the hunk header's positions.
        assert!(has("10 20 context"), "got: {rows:#?}");
        // A deletion advances only the before side, an insertion only the after side - so each
        // shows one number and leaves the other half of the gutter blank.
        assert!(has("11 -gone"), "got: {rows:#?}");
        assert!(has("21 +new"), "got: {rows:#?}");
    }

    /// `:` opens the jump prompt, digits accumulate, Enter moves the cursor - the whole point
    /// being a file too big to reach with `j`.
    #[test]
    fn colon_jumps_to_a_line_in_the_text_view() {
        let source = "a\nb\nc\nd\ne\nf\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::TextView {
            state: TextPaintState::default(),
        });

        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);
        for key in [KeyCode::Char(':'), KeyCode::Char('4'), KeyCode::Enter] {
            handle_modal_key(
                &mut app,
                key,
                &flat,
                &flat,
                root,
                root,
                &caches,
                source.as_bytes(),
                source.as_bytes(),
                &before,
                &after,
            );
        }

        let Some(Modal::TextView { state }) = &app.modal else {
            panic!("the text view should still be open, got {:?}", app.modal);
        };
        assert_eq!(state.cursor[0], (3, 0), "line 4 is row 3");
        assert!(state.line_prompt.is_none(), "the prompt closes on Enter");
    }

    /// Every key in the painting views preserves the cached `FrameState`, because painting writes
    /// `text_mappings` and the caches are built from `entries`. Falling through to `false` here
    /// re-walked both ASTs on every cursor keystroke, which is what made the view unusable on a
    /// large file.
    #[test]
    fn text_view_keys_do_not_invalidate_the_frame_state() {
        let text_view = Modal::TextView {
            state: TextPaintState::default(),
        };
        for key in [
            KeyCode::Char('j'),
            KeyCode::Char('v'),
            KeyCode::Char('m'),
            KeyCode::Char('x'),
            KeyCode::Esc,
        ] {
            assert!(
                is_state_preserving_key(Some(&text_view), key),
                "{key:?} should preserve the frame state"
            );
        }
    }

    /// The painting test is clamped at 100.0, not 0.0. Nothing in the corpus agrees exactly yet,
    /// so a fresh 0.0 would fail on the first run and have to be loosened - which is the "clamp
    /// moved for a reason that was not a measurement" habit the per-file layout exists to prevent.
    #[test]
    fn a_fresh_painting_test_passes_and_says_it_means_nothing_yet() {
        let block = painting_test_block("rust-add-if");

        assert!(
            block.contains(r#"assert_matches_human_painting_within_limit("rust-add-if", 100.0)"#),
            "got: {block}"
        );
        assert!(block.contains("Not measured yet"), "got: {block}");
        assert!(
            block.contains("fn painting()"),
            "the test name has to match the module's convention: {block}"
        );
        // Appended to a file that already has the licence header and the mapping test, so it
        // carries neither of its own - it starts at the blank line before its own `#[test]`.
        assert!(block.starts_with("\n#[test]"), "got: {block}");
    }

    /// A bare `App` for the text-painting action tests: they only touch `mapping`, `dirty` and
    /// `status`, so the AST panels' node ids are irrelevant and a dummy pair keeps the setup to
    /// one line.
    fn test_app() -> App {
        App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            0,
            0,
            HumanMapping::default(),
        )
    }

    fn paint_state_at(
        side: usize,
        cursor: (usize, usize),
        anchor: Option<(usize, usize)>,
    ) -> TextPaintState {
        let mut state = TextPaintState {
            side,
            ..Default::default()
        };
        state.cursor[side] = cursor;
        state.anchor[side] = anchor;
        state
    }

    /// A selection includes the character *under* the cursor. Anything else surprises a reader
    /// every time: they see a highlight covering `foo` and get a span covering `fo`.
    #[test]
    fn a_selection_includes_the_character_under_the_cursor() {
        let source = "let foo = 1;\n";
        let state = paint_state_at(0, (0, 6), Some((0, 4)));

        let spans = state.selection(0, source);
        assert_eq!(spans.len(), 1, "a same-row selection is a single span");
        let span = spans[0];

        assert_eq!(
            codediff::test::helper::human_mapping::span_text(source, span),
            Some("foo")
        );
    }

    /// Selecting backwards is the same selection - the anchor may be after the cursor.
    #[test]
    fn a_backwards_selection_normalizes_to_the_same_span() {
        let source = "let foo = 1;\n";
        let forward = paint_state_at(0, (0, 6), Some((0, 4)));
        let backward = paint_state_at(0, (0, 4), Some((0, 6)));

        // Both cover `foo`, but each includes the character under its own cursor, so the backward
        // one runs from 4 through the cursor at 4 and out to the anchor at 6 inclusive.
        assert_eq!(forward.selection(0, source), backward.selection(0, source));
    }

    /// Byte columns, not character columns - a multi-byte character before the selection must not
    /// shift it. The same trap `span_text`'s own test pins from the other side.
    #[test]
    fn a_selection_past_a_multibyte_character_lands_on_the_right_text() {
        let source = "let é = foo;\n";
        // "let é = " is 9 bytes ('é' costs two), so `foo` starts at byte 9 and its last character
        // starts at byte 11.
        let state = paint_state_at(0, (0, 11), Some((0, 9)));

        let span = state.selection(0, source)[0];

        assert_eq!(
            codediff::test::helper::human_mapping::span_text(source, span),
            Some("foo")
        );
    }

    /// A selection spanning several rows is vertical: one span per row, all sharing the same
    /// column range, rather than a single span sweeping full lines in between - the bug this
    /// feature replaces would highlight (and let `d`/`i` swallow) every untouched character
    /// between the two columns on the middle rows.
    #[test]
    fn a_multi_row_selection_is_a_stack_of_per_row_spans_not_a_line_sweep() {
        let source = "aaaXaaa\nbbbYbbb\ncccZccc\n";
        // Column 3 on row 0 through column 4 on row 2 - a one-column-wide block.
        let state = paint_state_at(0, (2, 4), Some((0, 3)));

        let spans = state.selection(0, source);

        assert_eq!(
            spans,
            vec![
                HumanTextSpan {
                    start_row: 0,
                    start_column: 3,
                    end_row: 0,
                    end_column: 5
                },
                HumanTextSpan {
                    start_row: 1,
                    start_column: 3,
                    end_row: 1,
                    end_column: 5
                },
                HumanTextSpan {
                    start_row: 2,
                    start_column: 3,
                    end_row: 2,
                    end_column: 5
                },
            ]
        );
    }

    /// A row too short to reach the selected columns contributes no span - there is nothing there
    /// to select, rather than the selection falling back to covering whatever the row does have.
    #[test]
    fn a_multi_row_selection_skips_a_row_shorter_than_the_selected_columns() {
        let source = "aaaaaa\nbb\ncccccc\n";
        let state = paint_state_at(0, (2, 4), Some((0, 4)));

        let spans = state.selection(0, source);

        assert_eq!(
            spans.iter().map(|s| s.start_row).collect::<Vec<_>>(),
            vec![0, 2],
            "row 1 ('bb') is shorter than column 4, so it contributes nothing"
        );
    }

    /// `V` swaps a selection back to the pre-vertical behaviour: one span sweeping every full row
    /// between anchor and cursor end to end, needed for a single contiguous multi-line block where
    /// `m`'s identical-text-per-side check would otherwise fail on a per-row decomposition.
    #[test]
    fn toggling_off_vertical_restores_the_full_line_sweep() {
        let source = "aaaXaaa\nbbbYbbb\ncccZccc\n";
        let mut state = paint_state_at(0, (2, 4), Some((0, 3)));
        state.vertical = false;

        let spans = state.selection(0, source);

        assert_eq!(
            spans,
            vec![HumanTextSpan {
                start_row: 0,
                start_column: 3,
                end_row: 2,
                end_column: 5
            }],
            "one span end to end, not a per-row stack"
        );
    }

    /// Reads back the style painted at one character position of one rendered row, skipping the
    /// gutter span every row starts with.
    fn style_at(lines: &[Line<'static>], row: usize, column: usize) -> Style {
        let line = &lines[row];
        let mut consumed = 0usize;
        for span in line.spans.iter().skip(1) {
            let len = span.content.chars().count();
            if column < consumed + len {
                return span.style;
            }
            consumed += len;
        }
        panic!("column {column} past the end of row {row}'s content: {line:?}");
    }

    /// The user-visible reason this toggle exists: a vertical selection leaves the untouched tail
    /// of a middle row unstyled, while the same anchor and cursor in full-line mode sweeps that
    /// tail in too. Exercises the actual render path, not just the spans `selection` computes.
    #[test]
    fn vertical_selection_leaves_a_middle_rows_tail_unstyled_but_full_line_does_not() {
        let source = "aaaXaaaaaaaa\nbbbYbbbbbbbb\ncccZcccccccc\n";
        let mut state = paint_state_at(0, (2, 4), Some((0, 3)));
        let selected_bg = Some(OverlayTheme::default().palette().cross_highlight_bg);
        // Row 1, well past the selection's column 3-4 range but still inside the line - the tail
        // a full-line sweep would highlight and a vertical selection would not.
        let tail_column = 10;

        let vertical = render_paint_side(source, &[], &state, 0, 5);
        assert_ne!(
            style_at(&vertical, 1, tail_column).bg,
            selected_bg,
            "vertical: row 1's tail past the selected columns must stay unstyled"
        );

        state.vertical = false;
        let full_line = render_paint_side(source, &[], &state, 0, 5);
        assert_eq!(
            style_at(&full_line, 1, tail_column).bg,
            selected_bg,
            "full-line: row 1's tail is swept into the selection between anchor and cursor"
        );
    }

    /// `h`/`l` step by characters, so the cursor can never land inside one - a column that did
    /// would produce a span `span_text` correctly refuses to read back.
    #[test]
    fn stepping_across_a_multibyte_character_lands_on_boundaries() {
        let source = "aéb\n";
        let mut state = paint_state_at(0, (0, 0), None);

        state.step_column(true, source);
        assert_eq!(state.cursor[0], (0, 1), "onto the two-byte character");
        state.step_column(true, source);
        assert_eq!(state.cursor[0], (0, 3), "past it in one step, not into it");
        state.step_column(false, source);
        assert_eq!(state.cursor[0], (0, 1), "and back the same way");
    }

    #[test]
    fn m_pairs_both_sides_selections_and_derives_move_from_identical_text() {
        let (before_src, after_src) = ("alpha\nbeta\n", "beta\nalpha\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 4);
        state.anchor[1] = Some((1, 0));
        state.cursor[1] = (1, 4);

        action_paint_match(&mut app, &mut state, before_src, after_src);

        let entries = &solution_entries(&app.mapping, &app.text_solution);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, HumanTextOperation::Match);
        assert_eq!(
            entries[0].verdict(before_src, after_src).unwrap(),
            HumanTextVerdict::Move,
            "identical text on both sides is a relocation"
        );
        assert!(app.dirty);
        assert_eq!(state.anchor, [None, None], "both selections are consumed");
    }

    #[test]
    fn m_without_a_selection_on_both_sides_paints_nothing_and_says_so() {
        let (before_src, after_src) = ("alpha\n", "alpha\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 4);

        action_paint_match(&mut app, &mut state, before_src, after_src);

        assert!(app.mapping.text_mappings.is_empty(), "nothing was painted");
        assert!(!app.dirty);
        assert!(
            app.status.as_deref().unwrap_or("").contains("both sides"),
            "got {:?}",
            app.status
        );
    }

    #[test]
    fn d_and_i_paint_one_sided_ranges_on_their_own_side() {
        let (before_src, after_src) = ("gone\nkept\n", "kept\nnew\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();

        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 3);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );

        state.side = 1;
        state.anchor[1] = Some((1, 0));
        state.cursor[1] = (1, 2);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Insert,
            before_src,
            after_src,
        );

        let entries = &solution_entries(&app.mapping, &app.text_solution);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            codediff::test::helper::human_mapping::span_text(before_src, entries[0].before[0]),
            Some("gone")
        );
        assert!(entries[0].after.is_empty(), "a delete has no after side");
        assert_eq!(
            codediff::test::helper::human_mapping::span_text(after_src, entries[1].after[0]),
            Some("new")
        );
        assert!(entries[1].before.is_empty(), "an insert has no before side");
    }

    /// `u` removes the whole entry, both halves of a `Match` included: a half-removed match is a
    /// malformed entry that `verdict` refuses to read, so removing one side is not a smaller edit
    /// but a broken file.
    #[test]
    fn u_removes_a_whole_match_from_either_side() {
        let (before_src, after_src) = ("alpha\nbeta\n", "beta\nalpha\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 4);
        state.anchor[1] = Some((1, 0));
        state.cursor[1] = (1, 4);
        action_paint_match(&mut app, &mut state, before_src, after_src);

        // Stand on the *after* half and unmark; the before half must go too.
        state.side = 1;
        state.cursor[1] = (1, 2);
        action_paint_unmark(&mut app, &state, before_src, after_src);

        assert!(
            solution_entries(&app.mapping, &app.text_solution).is_empty(),
            "both halves of the match should be gone"
        );
    }

    /// The `None`/`Some(empty)` distinction the field's `Option` exists for, reachable only
    /// deliberately.
    #[test]
    fn z_marks_an_unpainted_fixture_as_deliberately_empty() {
        let mut app = test_app();
        assert!(app.mapping.text_mappings.is_empty());

        action_paint_mark_empty(&mut app);

        assert_eq!(
            app.mapping.text_mappings.len(),
            1,
            "a named painting exists"
        );
        assert!(app.mapping.text_mappings[0].mapping.entries.is_empty());
        assert!(app.dirty);
    }

    #[test]
    fn z_refuses_to_touch_a_fixture_that_already_has_painted_ranges() {
        let (before_src, after_src) = ("gone\n", "\n");
        let mut app = test_app();
        let mut state = TextPaintState::default();
        state.anchor[0] = Some((0, 0));
        state.cursor[0] = (0, 3);
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );

        action_paint_mark_empty(&mut app);

        assert_eq!(
            solution_entries(&app.mapping, &app.text_solution).len(),
            1,
            "Z must not clear an existing painting"
        );
        assert!(
            app.status.as_deref().unwrap_or("").contains("already has"),
            "got {:?}",
            app.status
        );
    }

    #[test]
    fn the_text_view_renders_painted_ranges() {
        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 100, 24);
        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![],
            text_mappings: vec![NamedTextMapping {
                name: "Minimal".to_string(),
                mapping: HumanTextMapping {
                    entries: vec![HumanTextEntry {
                        operation: HumanTextOperation::Delete,
                        before: vec![HumanTextSpan {
                            start_row: 0,
                            start_column: 3,
                            end_row: 0,
                            end_column: 11,
                        }],
                        after: vec![],
                    }],
                },
            }],
        };

        terminal
            .draw(|f| {
                render_text_view_modal(
                    f,
                    area,
                    "fn old_name() {}",
                    "fn new_name() {}",
                    &mapping,
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    &TextPaintState::default(),
                );
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(text.contains("old_name"), "before content missing: {text}");
        assert!(text.contains("new_name"), "after content missing: {text}");
        assert!(
            text.contains("1 painted"),
            "the painted count should be in the title: {text}"
        );
    }

    fn rendered_text(terminal: &Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// The picker's header row is where its whole interaction state lives: the `Unmarked` count
    /// column exists, the sorted column carries a direction arrow, and a filtered column is
    /// marked - with the filters spelled out in the title so a compound one is readable.
    #[test]
    fn render_open_diff_picker_shows_the_unmarked_column_and_the_sort_and_filter_markers() {
        // Wide enough that the block title isn't truncated: this asserts on the title's contents,
        // so a narrower backend would be testing ratatui's truncation rather than the picker.
        let backend = ratatui::backend::TestBackend::new(160, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 160, 14);
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
        ];
        let unmarked =
            std::collections::HashMap::from([("alpha".to_string(), 42), ("bravo".to_string(), 0)]);
        let mut view = sort_view(DiffColumn::Unmarked);
        view.column = DiffColumn::Unmarked;
        view.filters.paint = FlagFilter::No;
        let modal = Modal::OpenDiffPicker {
            options,
            selected: 0,
            view,
            name_input: None,
        };

        terminal
            .draw(|f| {
                render_modal(
                    f,
                    area,
                    &modal,
                    "alpha",
                    None,
                    "",
                    "",
                    &HumanMapping::default(),
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    DiffPickerData {
                        unmarked: Some(&unmarked),
                        ..DiffPickerData::default()
                    },
                    None,
                )
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(text.contains("Unmarked^"), "sorted column header: {text}");
        assert!(text.contains("Paint*"), "filtered column marker: {text}");
        assert!(text.contains("42"), "the unmarked count itself: {text}");
        assert!(
            text.contains("unpainted only"),
            "the active filter spelled out in the title: {text}"
        );
        assert!(text.contains("s sort, f filter"), "the key legend: {text}");
    }

    /// The title is longer than the popup at ordinary terminal widths, so ratatui truncates it -
    /// and what has to survive that truncation is the filter list, the part that actually changes
    /// as the reader works. Guards the abbreviation in `render_open_diff_picker`'s title: at 110
    /// columns the key legend is expected to be cut, the filters are not.
    #[test]
    fn the_diff_picker_title_keeps_its_filter_list_when_the_terminal_truncates_it() {
        let backend = ratatui::backend::TestBackend::new(110, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 110, 12);
        let mut view = DiffPickerView::default();
        view.filters.paint = FlagFilter::No;
        view.filters.disagree = FlagFilter::Yes;
        let modal = Modal::OpenDiffPicker {
            options: vec![("alpha".to_string(), "handmade")],
            selected: 0,
            view,
            name_input: None,
        };

        terminal
            .draw(|f| {
                render_modal(
                    f,
                    area,
                    &modal,
                    "alpha",
                    None,
                    "",
                    "",
                    &HumanMapping::default(),
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    DiffPickerData::default(),
                    None,
                )
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("unpainted only AND disagreements only"),
            "both active filters must survive truncation at 110 columns: {text}"
        );
    }

    #[test]
    fn text_view_modal_renders_both_sides_content() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 24);

        terminal
            .draw(|f| {
                render_text_view_modal(
                    f,
                    area,
                    "fn old_name() {}",
                    "fn new_name() {}",
                    &HumanMapping::default(),
                    "Minimal",
                    TextOverlay::Human,
                    None,
                    None,
                    &TextPaintState::default(),
                );
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("old_name"),
            "before content missing from render: {text}"
        );
        assert!(
            text.contains("new_name"),
            "after content missing from render: {text}"
        );
    }

    #[test]
    fn unix_diff_modal_renders_diff_output() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 24);

        let output = run_unix_diff(
            b"fn main() {\n    old();\n}\n",
            b"fn main() {\n    new();\n}\n",
        )
        .unwrap();
        terminal
            .draw(|f| {
                render_unix_diff_modal(f, area, &output, 0);
            })
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("old();"),
            "removed line missing from render: {text}"
        );
        assert!(
            text.contains("new();"),
            "added line missing from render: {text}"
        );
    }

    #[test]
    fn centered_rect_at_least_uses_the_percentage_when_it_already_meets_the_minimum() {
        let area = Rect::new(0, 0, 100, 100);
        let rect = centered_rect_at_least(60, 30, 10, 10, area);
        assert_eq!(rect, centered_rect(60, 30, area));
    }

    #[test]
    fn centered_rect_at_least_grows_past_the_percentage_to_meet_the_minimum() {
        // A 100x20 terminal's 30%-height popup would be 6 rows, short of a 10-row minimum.
        let small_area = Rect::new(0, 0, 100, 20);
        let rect = centered_rect_at_least(60, 30, 10, 10, small_area);
        assert_eq!(
            rect.height, 10,
            "should grow to the minimum, not stay at 30% (6 rows)"
        );
        assert!(rect.height <= small_area.height);
    }

    #[test]
    fn centered_rect_at_least_never_exceeds_the_available_area() {
        // A terminal too small even for the minimum (a phone-sized SSH client is the motivating
        // case) must still produce a rect that fits, not one that's clamped to a minimum bigger
        // than the terminal itself.
        let tiny_area = Rect::new(0, 0, 20, 8);
        let rect = centered_rect_at_least(60, 30, 50, 20, tiny_area);
        assert!(rect.width <= tiny_area.width);
        assert!(rect.height <= tiny_area.height);
    }

    /// Regression guard for the real bug: on a small terminal (a phone-sized SSH client is the
    /// motivating case), `render_text_modal`'s old fixed `centered_rect(60, 30, area)` could come
    /// out short enough that the `> {input}` line - the actual input box, well past the first
    /// couple of lines of instructions - was clipped out of the visible area entirely, with no
    /// scroll indicator to hint why (a plain `Paragraph` has none). This is exactly the
    /// `PromptPromoteName` modal's own body shape (instructions, then a blank line, then `> `).
    #[test]
    fn render_text_modal_shows_every_line_including_the_input_box_on_a_small_terminal() {
        let backend = ratatui::backend::TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 40, 12);

        let body = "Enter a name for src/test/data/diffs/small/<name>/\n(letters, digits, - and _; must not already exist)\n\n> rust-rustdesk-\n\n[Enter] confirm   [Esc] cancel";
        terminal
            .draw(|f| render_text_modal(f, area, "Promote sample to test case", body))
            .unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("rust-rustdesk-"),
            "the input line must still be visible on a small terminal, not scrolled out of view: {text}"
        );
    }

    #[test]
    fn open_sample_picker_marks_solved_and_rejected_entries_and_can_hide_both() {
        let options = vec![
            (
                "rust-x-foo-abc12345-a".to_string(),
                SampleTriageStatus::Promoted,
                7,
            ),
            (
                "rust-x-foo-def67890-b".to_string(),
                SampleTriageStatus::Sampled,
                3,
            ),
            (
                "rust-x-foo-fed09876-c".to_string(),
                SampleTriageStatus::Rejected,
                2,
            ),
        ];

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 24);

        terminal
            .draw(|f| {
                render_open_sample_picker(
                    f,
                    area,
                    &options,
                    0,
                    false,
                    SampleSortOrder::Alphabetical,
                )
            })
            .unwrap();
        let text = rendered_text(&terminal);
        assert!(
            text.contains("rust-x-foo-abc12345-a (7) - SOLVED"),
            "solved marker missing: {text}"
        );
        assert!(
            text.contains("rust-x-foo-fed09876-c (2) - REJECTED"),
            "rejected marker missing: {text}"
        );
        assert!(
            text.contains("rust-x-foo-def67890-b (3)"),
            "unsolved entry missing: {text}"
        );
        assert!(
            text.contains("1/3"),
            "count should include all three entries: {text}"
        );

        terminal
            .draw(|f| {
                render_open_sample_picker(f, area, &options, 0, true, SampleSortOrder::Alphabetical)
            })
            .unwrap();
        let text = rendered_text(&terminal);
        assert!(
            !text.contains("SOLVED"),
            "solved entry should be hidden: {text}"
        );
        assert!(
            !text.contains("REJECTED"),
            "rejected entry should be hidden: {text}"
        );
        assert!(
            text.contains("rust-x-foo-def67890-b"),
            "unsolved entry should still show: {text}"
        );
        assert!(
            text.contains("1/1"),
            "count should only include the unsolved entry: {text}"
        );
    }

    #[test]
    fn render_open_sample_picker_shows_the_current_sort_order() {
        // Wider than the other picker tests: the title is long enough (position, hide-solved
        // hint, sort order, key hints) that an 80-column terminal's narrow ~46-column popup
        // truncates it well before reaching "sort:" - this test cares specifically about that
        // tail end, so it needs the room.
        let options = vec![("a".to_string(), SampleTriageStatus::Sampled, 1)];
        let backend = ratatui::backend::TestBackend::new(200, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 200, 24);

        terminal
            .draw(|f| {
                render_open_sample_picker(
                    f,
                    area,
                    &options,
                    0,
                    false,
                    SampleSortOrder::LargestDiffFirst,
                )
            })
            .unwrap();
        let text = rendered_text(&terminal);
        assert!(
            text.contains("largest diff first"),
            "title should reflect the current sort order: {text}"
        );
    }

    #[test]
    fn visible_sample_options_orders_by_the_requested_sort_order() {
        let options = vec![
            ("charlie".to_string(), SampleTriageStatus::Sampled, 5),
            ("alpha".to_string(), SampleTriageStatus::Sampled, 20),
            ("bravo".to_string(), SampleTriageStatus::Sampled, 1),
        ];

        let names = |order| -> Vec<String> {
            visible_sample_options(&options, false, order)
                .into_iter()
                .map(|(name, ..)| name)
                .collect()
        };

        assert_eq!(
            names(SampleSortOrder::Alphabetical),
            vec!["alpha", "bravo", "charlie"]
        );
        assert_eq!(
            names(SampleSortOrder::ReverseAlphabetical),
            vec!["charlie", "bravo", "alpha"]
        );
        assert_eq!(
            names(SampleSortOrder::SmallestDiffFirst),
            vec!["bravo", "charlie", "alpha"]
        );
        assert_eq!(
            names(SampleSortOrder::LargestDiffFirst),
            vec!["alpha", "charlie", "bravo"]
        );
    }

    #[test]
    fn sample_sort_order_next_cycles_through_all_four_and_back() {
        assert_eq!(
            SampleSortOrder::Alphabetical.next(),
            SampleSortOrder::ReverseAlphabetical
        );
        assert_eq!(
            SampleSortOrder::ReverseAlphabetical.next(),
            SampleSortOrder::SmallestDiffFirst
        );
        assert_eq!(
            SampleSortOrder::SmallestDiffFirst.next(),
            SampleSortOrder::LargestDiffFirst
        );
        assert_eq!(
            SampleSortOrder::LargestDiffFirst.next(),
            SampleSortOrder::Alphabetical
        );
    }

    #[test]
    fn open_sample_picker_enter_opens_the_visible_entry_not_the_raw_index() {
        // Regression guard for the switch from `options[selected]` to `visible.get(selected)`:
        // with a solved entry and a rejected entry hidden, `selected` indexes the *filtered*
        // list, so index 1 here must resolve to "unsolved-two" (the second visible entry), not
        // "unsolved-one" (index 2 in the unfiltered `options`) or either hidden entry.
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        app.modal = Some(Modal::OpenSamplePicker {
            options: vec![
                ("rejected-one".to_string(), SampleTriageStatus::Rejected, 0),
                ("solved-one".to_string(), SampleTriageStatus::Promoted, 0),
                ("unsolved-one".to_string(), SampleTriageStatus::Sampled, 0),
                ("unsolved-two".to_string(), SampleTriageStatus::Sampled, 0),
            ],
            selected: 1,
            hide_solved: true,
            sort_order: SampleSortOrder::Alphabetical,
        });
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        let target = handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        match target {
            Some(OpenTarget::Sample(name)) => assert_eq!(name, "unsolved-two"),
            other => panic!("expected OpenTarget::Sample(\"unsolved-two\"), got {other:?}"),
        }
    }

    #[test]
    fn open_sample_picker_s_advances_sort_order_and_resets_selection_to_first() {
        // Unlike `H` (which tracks the previously selected name across a re-sort), `s` always
        // resets `selected` to 0 - changing sort order is about jumping to whichever end of the
        // new order is interesting, not staying on what was picked under the old one.
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        app.modal = Some(Modal::OpenSamplePicker {
            options: vec![
                ("alpha".to_string(), SampleTriageStatus::Sampled, 5),
                ("bravo".to_string(), SampleTriageStatus::Sampled, 1),
                ("charlie".to_string(), SampleTriageStatus::Sampled, 20),
            ],
            selected: 2,
            hide_solved: false,
            sort_order: SampleSortOrder::Alphabetical,
        });
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        let target = handle_modal_key(
            &mut app,
            KeyCode::Char('s'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(target.is_none(), "s should not switch cases directly");
        match app.modal {
            Some(Modal::OpenSamplePicker {
                selected,
                sort_order,
                ..
            }) => {
                assert_eq!(selected, 0, "selection should reset to the first entry");
                assert_eq!(sort_order, SampleSortOrder::ReverseAlphabetical);
            }
            other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
        }
        assert_eq!(
            app.sample_sort_order,
            SampleSortOrder::ReverseAlphabetical,
            "the new sort order must persist on App too, not just this modal instance, so the \
             next O reopens with it instead of resetting to Alphabetical"
        );
    }

    #[test]
    fn open_sample_picker_h_persists_hide_solved_on_app() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        app.modal = Some(Modal::OpenSamplePicker {
            options: vec![("alpha".to_string(), SampleTriageStatus::Sampled, 5)],
            selected: 0,
            hide_solved: false,
            sort_order: SampleSortOrder::Alphabetical,
        });
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        handle_modal_key(
            &mut app,
            KeyCode::Char('H'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(
            app.sample_hide_solved,
            "H's new hide_solved value must persist on App too, so the next O reopens with it"
        );
    }

    #[test]
    fn open_commit_picker_j_k_move_selection_clamped_to_bounds() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitPicker {
            commits: vec![
                ("aaa".to_string(), "first commit".to_string()),
                ("bbb".to_string(), "second commit".to_string()),
            ],
            selected: 0,
        });

        // Up at the top must stay clamped at 0, not underflow.
        handle_modal_key(
            &mut app,
            KeyCode::Char('k'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        match &app.modal {
            Some(Modal::OpenCommitPicker { selected, .. }) => assert_eq!(*selected, 0),
            other => panic!("expected Modal::OpenCommitPicker, got {other:?}"),
        }

        // Two Downs must clamp at the last index (1), not run past it.
        for _ in 0..2 {
            handle_modal_key(
                &mut app,
                KeyCode::Char('j'),
                &flat,
                &flat,
                root,
                root,
                &caches,
                source.as_bytes(),
                source.as_bytes(),
                &Code::from_string(source, &Language::Rust),
                &Code::from_string(source, &Language::Rust),
            );
        }
        match &app.modal {
            Some(Modal::OpenCommitPicker { selected, .. }) => assert_eq!(*selected, 1),
            other => panic!("expected Modal::OpenCommitPicker, got {other:?}"),
        }
    }

    #[test]
    fn open_commit_picker_esc_cancels() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitPicker {
            commits: vec![("aaa".to_string(), "first commit".to_string())],
            selected: 0,
        });

        let target = handle_modal_key(
            &mut app,
            KeyCode::Esc,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(target.is_none());
        assert_eq!(app.status.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn open_commit_picker_enter_on_an_unresolvable_commit_reports_an_error_without_crashing() {
        // Doesn't depend on this repository's actual git history (which the CI checkout may only
        // have a shallow slice of - see `list_commit_files`'s doc comment): any hash git can't
        // resolve at all takes the same `git diff-tree` failure path, regardless of clone depth.
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitPicker {
            commits: vec![("not-a-real-commit-hash".to_string(), "bogus".to_string())],
            selected: 0,
        });

        let target = handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(
            target.is_none(),
            "an unresolvable commit must not switch cases"
        );
        match &app.modal {
            Some(Modal::OpenCommitPicker { .. }) => {}
            other => {
                panic!("expected to stay on Modal::OpenCommitPicker after the error, got {other:?}")
            }
        }
        assert!(
            app.status.is_some(),
            "the failure should be reported on the status line, not silently dropped"
        );
    }

    #[test]
    fn open_commit_file_picker_enter_opens_the_selected_file_as_an_open_target() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitFilePicker {
            hash: "abc123".to_string(),
            summary: "did a thing".to_string(),
            files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
            selected: 1,
        });

        let target = handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        match target {
            Some(OpenTarget::GitCommitFile {
                hash,
                summary,
                path,
            }) => {
                assert_eq!(hash, "abc123");
                assert_eq!(summary, "did a thing");
                assert_eq!(path, "src/b.rs");
            }
            other => panic!("expected OpenTarget::GitCommitFile, got {other:?}"),
        }
    }

    #[test]
    fn open_commit_file_picker_enter_from_a_dirty_git_commit_file_case_cannot_save_directly() {
        // Mirrors `CaseOrigin::Sample`'s existing `can_save = false`: a git-commit-sourced case
        // isn't a real diffs/ case yet either, so it needs `s`'s promote-name prompt (from the
        // main view), not a single-key save, before it can be switched away from.
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::GitCommitFile {
                path: "src/current.rs".to_string(),
            },
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        app.dirty = true;
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitFilePicker {
            hash: "abc123".to_string(),
            summary: "did a thing".to_string(),
            files: vec!["src/a.rs".to_string()],
            selected: 0,
        });

        let target = handle_modal_key(
            &mut app,
            KeyCode::Enter,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(
            target.is_none(),
            "must not switch immediately; ConfirmDiscardUnsaved decides first"
        );
        match app.modal {
            Some(Modal::ConfirmDiscardUnsaved { target, can_save }) => {
                assert!(
                    !can_save,
                    "a git-commit-sourced current case cannot be saved with a single key"
                );
                match target {
                    OpenTarget::GitCommitFile { path, .. } => assert_eq!(path, "src/a.rs"),
                    other => panic!("expected OpenTarget::GitCommitFile, got {other:?}"),
                }
            }
            other => panic!("expected Modal::ConfirmDiscardUnsaved, got {other:?}"),
        }
    }

    #[test]
    fn open_commit_file_picker_esc_cancels() {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenCommitFilePicker {
            hash: "abc123".to_string(),
            summary: "did a thing".to_string(),
            files: vec!["src/a.rs".to_string()],
            selected: 0,
        });

        let target = handle_modal_key(
            &mut app,
            KeyCode::Esc,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert!(target.is_none());
        assert_eq!(app.status.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn open_sample_picker_modal_selects_the_currently_open_case_under_the_given_sort_order() {
        let options = vec![
            ("alpha".to_string(), SampleTriageStatus::Sampled, 5),
            ("bravo".to_string(), SampleTriageStatus::Sampled, 1),
            ("charlie".to_string(), SampleTriageStatus::Sampled, 20),
        ];

        // "bravo" is index 1 in `options`' own order, but index 0 once sorted smallest-diff-first
        // - proves `selected` is computed against the sorted/filtered view, not raw `options`.
        let modal =
            open_sample_picker_modal(options, "bravo", false, SampleSortOrder::SmallestDiffFirst);

        match modal {
            Modal::OpenSamplePicker {
                selected,
                hide_solved,
                sort_order,
                ..
            } => {
                assert_eq!(selected, 0);
                assert!(!hide_solved);
                assert_eq!(sort_order, SampleSortOrder::SmallestDiffFirst);
            }
            other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
        }
    }

    #[test]
    fn open_sample_picker_modal_falls_back_to_the_first_entry_when_the_current_case_is_not_a_sample()
     {
        let options = vec![("alpha".to_string(), SampleTriageStatus::Sampled, 5)];
        let modal = open_sample_picker_modal(
            options,
            "not-a-sample-name",
            false,
            SampleSortOrder::Alphabetical,
        );
        match modal {
            Modal::OpenSamplePicker { selected, .. } => assert_eq!(selected, 0),
            other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
        }
    }

    #[test]
    fn visible_diff_options_narrows_to_the_given_dataset() {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "small"),
            ("charlie".to_string(), "handmade"),
            ("delta".to_string(), "full"),
        ];
        assert_eq!(
            visible_diff_options(&options, &dataset_view(None), DiffPickerData::default()),
            vec!["alpha", "bravo", "charlie", "delta"],
            "no filter should show every dataset"
        );
        assert_eq!(
            visible_diff_options(
                &options,
                &dataset_view(Some("handmade")),
                DiffPickerData::default()
            ),
            vec!["alpha", "charlie"]
        );
        assert_eq!(
            visible_diff_options(
                &options,
                &dataset_view(Some("full")),
                DiffPickerData::default()
            ),
            vec!["delta"]
        );
    }

    #[test]
    fn visible_diff_options_cmpl_filter_excludes_only_cases_the_map_measured() {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
            ("charlie".to_string(), "handmade"),
        ];
        let mut unmarked = std::collections::HashMap::new();
        unmarked.insert("alpha".to_string(), 4); // incomplete
        unmarked.insert("bravo".to_string(), 0); // complete
        // "charlie" deliberately absent - not yet scanned, or failed to load.
        let data = DiffPickerData {
            unmarked: Some(&unmarked),
            ..DiffPickerData::default()
        };

        assert_eq!(
            visible_diff_options(
                &options,
                &flag_view(DiffColumn::Cmpl, FlagFilter::Yes),
                data
            ),
            vec!["alpha", "charlie"],
            "'incomplete only' hides complete, and keeps incomplete and unscanned alike"
        );
        assert_eq!(
            visible_diff_options(&options, &flag_view(DiffColumn::Cmpl, FlagFilter::No), data),
            vec!["bravo", "charlie"],
            "'complete only' hides incomplete, and still keeps the unscanned one"
        );
        assert_eq!(
            visible_diff_options(&options, &DiffPickerView::default(), data),
            vec!["alpha", "bravo", "charlie"],
            "the filter off should show everything regardless of the map"
        );
    }

    /// Setting both `Cmpl` and `Unmarked` the same way narrows nothing further, so the title bar
    /// must not read as two constraints - see `DiffFilters::labels`.
    #[test]
    fn the_title_lists_a_matching_cmpl_and_unmarked_filter_once() {
        let mut filters = DiffFilters {
            cmpl: FlagFilter::Yes,
            unmarked: FlagFilter::Yes,
            ..DiffFilters::default()
        };
        assert_eq!(filters.labels(), vec!["incomplete only"]);

        // Opposite directions really are two constraints (an unsatisfiable pair, but the reader
        // should be able to see that), so both are listed.
        filters.unmarked = FlagFilter::No;
        assert_eq!(filters.labels(), vec!["incomplete only", "none unmarked"]);
    }

    /// `Cmpl` and `Unmarked` read one map (`App::diff_unmarked`), so their filters must select
    /// exactly the same rows - the two columns differ in how they *sort*, not in what they hide.
    #[test]
    fn the_cmpl_and_unmarked_filters_select_the_same_rows() {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
            ("charlie".to_string(), "handmade"),
        ];
        let unmarked =
            std::collections::HashMap::from([("alpha".to_string(), 4), ("bravo".to_string(), 0)]);
        let data = DiffPickerData {
            unmarked: Some(&unmarked),
            ..DiffPickerData::default()
        };

        for filter in [FlagFilter::Yes, FlagFilter::No] {
            assert_eq!(
                visible_diff_options(&options, &flag_view(DiffColumn::Cmpl, filter), data),
                visible_diff_options(&options, &flag_view(DiffColumn::Unmarked, filter), data),
                "{filter:?} must mean the same thing on both columns"
            );
        }
    }

    #[test]
    fn next_dataset_filter_cycles_through_diff_datasets_and_back_to_all() {
        // Walks every entry rather than hardcoding `DIFF_DATASETS`' length, so this doesn't need
        // editing again the next time a dataset is added (it already needed exactly that edit
        // once, when `stratified` became the fourth).
        let mut current = None;
        for &dataset in DIFF_DATASETS {
            current = next_dataset_filter(current);
            assert_eq!(current, Some(dataset));
        }
        assert_eq!(next_dataset_filter(current), None);
    }

    #[test]
    fn open_diff_picker_modal_selects_the_currently_open_case_under_the_given_filter() {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "small"),
            ("charlie".to_string(), "handmade"),
        ];
        let view = dataset_view(Some("handmade"));

        // "charlie" is index 2 in `options`' own order, but index 1 once filtered to just
        // "handmade" - proves `selected` is computed against the filtered view, not raw options.
        let modal = open_diff_picker_modal(options, "charlie", view, DiffPickerData::default());

        match modal {
            Modal::OpenDiffPicker { selected, view, .. } => {
                assert_eq!(selected, 1);
                assert_eq!(view.filters.dataset, Some("handmade"));
            }
            other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
        }
    }

    #[test]
    fn open_diff_picker_modal_falls_back_to_the_first_entry_when_the_current_case_is_filtered_out()
    {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "small"),
        ];
        let view = dataset_view(Some("small"));
        // "alpha" is the currently open case, but it's a "handmade" fixture and the filter above
        // is "small" - alpha isn't in the filtered view at all, so this must fall back to the
        // first visible entry instead of panicking or landing out of bounds.
        let modal = open_diff_picker_modal(options, "alpha", view, DiffPickerData::default());
        match modal {
            Modal::OpenDiffPicker { selected, .. } => assert_eq!(selected, 0),
            other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
        }
    }

    /// Opens the `o` picker over `options` with `view` in force and feeds it `keys` in order,
    /// handing back the App - the shared body of every picker key test below, so each says only
    /// what it is actually about.
    fn press_in_diff_picker(
        options: Vec<(String, &'static str)>,
        view: DiffPickerView,
        keys: &[KeyCode],
    ) -> App {
        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);
        app.modal = Some(Modal::OpenDiffPicker {
            options,
            selected: 0,
            view,
            name_input: None,
        });

        for key in keys {
            handle_modal_key(
                &mut app,
                *key,
                &flat,
                &flat,
                root,
                root,
                &caches,
                source.as_bytes(),
                source.as_bytes(),
                &Code::from_string(source, &Language::Rust),
                &Code::from_string(source, &Language::Rust),
            );
        }
        app
    }

    fn picker_view(app: &App) -> DiffPickerView {
        match &app.modal {
            Some(Modal::OpenDiffPicker { view, .. }) => view.clone(),
            other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
        }
    }

    /// `h`/`l` walk the cursor across the header and clamp at both ends rather than wrapping.
    #[test]
    fn open_diff_picker_h_and_l_move_the_column_cursor_and_clamp_at_the_ends() {
        let options = vec![("alpha".to_string(), "handmade")];

        let app = press_in_diff_picker(
            options.clone(),
            DiffPickerView::default(),
            &[KeyCode::Char('l'), KeyCode::Char('l')],
        );
        assert_eq!(picker_view(&app).column, DiffColumn::Cmpl);
        assert_eq!(
            app.diff_view.column,
            DiffColumn::Cmpl,
            "the cursor column persists on App too, so the next o reopens on it"
        );

        // Six presses from the far left overshoots the six-column table by one.
        let app = press_in_diff_picker(
            options.clone(),
            DiffPickerView::default(),
            &[
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
            ],
        );
        assert_eq!(
            picker_view(&app).column,
            DiffColumn::Disagree,
            "l must clamp at the last column, not wrap round to Name"
        );

        let app = press_in_diff_picker(options, DiffPickerView::default(), &[KeyCode::Char('h')]);
        assert_eq!(
            picker_view(&app).column,
            DiffColumn::Name,
            "h must clamp at the first column"
        );
    }

    /// `f` on `Dataset` is the old `d` key: it cycles the dataset filter, and - like every other
    /// filter and sort change here - persists the result on `App` so the next `o` reopens with it.
    #[test]
    fn open_diff_picker_f_on_the_dataset_column_persists_the_filter_on_app() {
        let app = press_in_diff_picker(
            vec![("alpha".to_string(), "handmade")],
            column_view(DiffColumn::Dataset),
            &[KeyCode::Char('f')],
        );

        assert_eq!(
            app.diff_view.filters.dataset,
            Some(DIFF_DATASETS[0]),
            "f's new filter must persist on App too, so the next o reopens with it"
        );
        assert_eq!(picker_view(&app).filters.dataset, Some(DIFF_DATASETS[0]));
    }

    /// `s` takes the sort over to the cursor column ascending, and flips direction when pressed
    /// again on the column that already owns it - the "last column selected sorts" rule.
    #[test]
    fn open_diff_picker_s_sorts_by_the_cursor_column_and_flips_on_a_second_press() {
        let view = column_view(DiffColumn::Dataset);
        let options = vec![("alpha".to_string(), "handmade")];

        let app = press_in_diff_picker(options.clone(), view.clone(), &[KeyCode::Char('s')]);
        assert_eq!(
            app.diff_view.sort,
            DiffSort {
                column: DiffColumn::Dataset,
                descending: false
            }
        );

        let app = press_in_diff_picker(options, view, &[KeyCode::Char('s'), KeyCode::Char('s')]);
        assert_eq!(
            app.diff_view.sort,
            DiffSort {
                column: DiffColumn::Dataset,
                descending: true
            },
            "a second s on the same column reverses it rather than moving on"
        );
    }

    /// While the `Name` filter's prompt is open it takes every keystroke - so a name containing
    /// `j`, `s` or `f` is typed rather than moving the selection and re-sorting mid-word.
    #[test]
    fn open_diff_picker_name_filter_prompt_swallows_command_keys_until_enter() {
        let options = vec![
            ("rust-add-if".to_string(), "handmade"),
            ("java-fix".to_string(), "handmade"),
        ];

        let app = press_in_diff_picker(
            options.clone(),
            DiffPickerView::default(),
            &[KeyCode::Char('f'), KeyCode::Char('j'), KeyCode::Char('s')],
        );
        match &app.modal {
            Some(Modal::OpenDiffPicker {
                name_input, view, ..
            }) => {
                assert_eq!(name_input.as_deref(), Some("js"));
                assert_eq!(
                    view.sort,
                    DiffSort::default(),
                    "the s typed into the prompt must not have re-sorted the table"
                );
            }
            other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
        }

        // Enter commits it, lowercased, and it narrows the list.
        let app = press_in_diff_picker(
            options.clone(),
            DiffPickerView::default(),
            &[
                KeyCode::Char('f'),
                KeyCode::Char('R'),
                KeyCode::Char('u'),
                KeyCode::Enter,
            ],
        );
        assert_eq!(app.diff_view.filters.name.as_deref(), Some("ru"));
        assert_eq!(
            visible_diff_options(&options, &app.diff_view, DiffPickerData::from_app(&app)),
            vec!["rust-add-if"]
        );

        // Esc abandons the prompt and leaves the filter exactly as it was.
        let app = press_in_diff_picker(
            options,
            DiffPickerView::default(),
            &[KeyCode::Char('f'), KeyCode::Char('r'), KeyCode::Esc],
        );
        assert_eq!(picker_view(&app).filters.name, None);
        assert!(
            matches!(
                &app.modal,
                Some(Modal::OpenDiffPicker {
                    name_input: None,
                    ..
                })
            ),
            "Esc must close the prompt but leave the picker open"
        );
    }

    /// An empty submission clears the filter rather than being stored as a needle that matches
    /// everything while the header still reads as filtered.
    #[test]
    fn open_diff_picker_name_filter_prompt_clears_on_an_empty_submission() {
        let app = press_in_diff_picker(
            vec![("rust-add-if".to_string(), "handmade")],
            name_view("rust"),
            &[
                KeyCode::Char('f'),
                KeyCode::Backspace,
                KeyCode::Backspace,
                KeyCode::Backspace,
                KeyCode::Backspace,
                KeyCode::Enter,
            ],
        );

        assert_eq!(app.diff_view.filters.name, None);
    }

    /// Moving the cursor must never kick off one of the corpus-wide scans - only `s`/`f` do, and
    /// those are deliberate presses that can afford the seconds it costs (see
    /// `ensure_diff_column_data`).
    #[test]
    fn open_diff_picker_column_movement_does_not_trigger_a_corpus_scan() {
        let app = press_in_diff_picker(
            vec![("alpha".to_string(), "handmade")],
            DiffPickerView::default(),
            &[
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
                KeyCode::Char('l'),
            ],
        );

        assert_eq!(picker_view(&app).column, DiffColumn::Disagree);
        assert!(app.diff_unmarked.is_none());
        assert!(app.diff_text_painted.is_none());
        assert!(app.diff_disagreement.is_none());
    }

    #[test]
    fn draw_ui_shows_only_the_focused_panel_below_the_single_panel_width_threshold() {
        let before_source = "fn before_marker() {}\n";
        let after_source = "fn after_marker() {}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
        let before_unmarked = count_unmarked(&before_flat, &caches, status_before);
        let after_unmarked = count_unmarked(&after_flat, &caches, status_after);

        // Narrower than `SINGLE_PANEL_WIDTH_THRESHOLD`: only the focused (Before, by
        // `App::new`'s default) panel should render, and the After panel's content shouldn't
        // appear anywhere on screen.
        let backend = ratatui::backend::TestBackend::new(SINGLE_PANEL_WIDTH_THRESHOLD - 1, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw_ui(
                    f,
                    &mut app,
                    &before_flat,
                    &after_flat,
                    &caches,
                    before_source.as_bytes(),
                    after_source.as_bytes(),
                    before_unmarked,
                    after_unmarked,
                    "test",
                )
            })
            .unwrap();
        let text = rendered_text(&terminal);
        assert!(
            text.contains("before_marker"),
            "focused panel missing from render: {text}"
        );
        assert!(
            !text.contains("after_marker"),
            "unfocused panel should not render in single-panel mode: {text}"
        );

        // At or above the threshold, both panels render side by side.
        let backend = ratatui::backend::TestBackend::new(SINGLE_PANEL_WIDTH_THRESHOLD, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw_ui(
                    f,
                    &mut app,
                    &before_flat,
                    &after_flat,
                    &caches,
                    before_source.as_bytes(),
                    after_source.as_bytes(),
                    before_unmarked,
                    after_unmarked,
                    "test",
                )
            })
            .unwrap();
        let text = rendered_text(&terminal);
        assert!(
            text.contains("before_marker"),
            "before panel missing from wide render: {text}"
        );
        assert!(
            text.contains("after_marker"),
            "after panel missing from wide render: {text}"
        );
    }

    #[test]
    fn help_modal_renders_keybindings() {
        // Sized generously (well past HELP_TEXT's longest line and line count) so nothing is
        // clipped by the popup's width or height -- this test is about content, not layout.
        //
        // The height has to stay ahead of HELP_TEXT: the popup takes 90% of it and spends two rows
        // on borders, so it renders `0.9 * height - 2` lines. At 80 rows that was 70 against a
        // 73-line reference, and this test started failing for the reference having grown rather
        // than for anything it exists to check - which had already cost two rounds of trimming
        // real content to fit a fixture. The modal scrolls (j/k) precisely because the reference
        // outgrew one screen long ago on any ordinary terminal.
        let backend = ratatui::backend::TestBackend::new(140, 120);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 140, 120);

        terminal.draw(|f| render_help_modal(f, area, 0)).unwrap();

        let text = rendered_text(&terminal);
        assert!(
            text.contains("Keybindings"),
            "help title missing from render: {text}"
        );
        assert!(
            text.contains("switch focus between"),
            "first entry missing from render: {text}"
        );
        assert!(
            text.contains("toggle this help"),
            "help entry missing from render: {text}"
        );
        assert!(
            text.contains("quit"),
            "last entry missing from render: {text}"
        );
    }

    /// Parses a tiny Rust snippet for the `fully_solved_nodes`/`flatten_visible` tests below,
    /// decoupled from any real fixture on disk.
    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let language =
            codediff::code::language::to_treesitter(&codediff::code::Language::Rust).unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn find_first<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find_map(|child| find_first(child, kind))
    }

    /// The function body's two top-level statements, `a();` and `b();`.
    fn two_statements(root: Node) -> (Node, Node) {
        let block = find_first(root, "block").unwrap();
        let mut cursor = block.walk();
        let statements: Vec<Node> = block
            .children(&mut cursor)
            .filter(|n| n.kind() == "expression_statement")
            .collect();
        assert_eq!(statements.len(), 2);
        (statements[0], statements[1])
    }

    /// Every `expression_statement` directly in the function body - unlike `two_statements`,
    /// doesn't assume a fixed count, so it works for the 2/3-identical-`foo()`-call fixtures the
    /// multi-map group tests below use.
    fn block_statements(root: Node) -> Vec<Node> {
        let block = find_first(root, "block").unwrap();
        let mut cursor = block.walk();
        block
            .children(&mut cursor)
            .filter(|n| n.kind() == "expression_statement")
            .collect()
    }

    fn mark_subtree_matched(node: Node, caches: &mut Caches) {
        caches.before_match.insert(node.id(), usize::MAX);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            mark_subtree_matched(child, caches);
        }
    }

    #[test]
    fn fully_solved_nodes_hides_fully_marked_subtree_but_keeps_unmarked_ancestors() {
        let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
        let root = tree.root_node();
        let (stmt_a, stmt_b) = two_statements(root);
        let block = find_first(root, "block").unwrap();

        let mut caches = Caches::default();
        // `a();` and everything under it is matched: fully solved.
        mark_subtree_matched(stmt_a, &mut caches);
        // `b();` itself is matched, but its `call_expression` child is left Unmarked, so the
        // statement as a whole is not fully solved.
        caches.before_match.insert(stmt_b.id(), usize::MAX);

        let solved = fully_solved_nodes(root, &caches, status_before);

        assert!(
            solved.contains(&stmt_a.id()),
            "fully marked subtree should be solved"
        );
        assert!(
            !solved.contains(&stmt_b.id()),
            "partially marked subtree should not be solved"
        );
        assert!(
            !solved.contains(&block.id()),
            "block has an unsolved descendant, so isn't solved"
        );
        assert!(
            !solved.contains(&root.id()),
            "root has an unsolved descendant, so isn't solved"
        );
    }

    #[test]
    fn count_unmarked_nodes_in_tree_counts_every_hole_not_just_the_first() {
        let tree = parse_rust("fn main() {\n    a();\n}\n");
        let root = tree.root_node();

        let total = count_unmarked_nodes_in_tree(root, &Caches::default(), status_before);
        assert!(
            total > 1,
            "nothing marked at all should count every node in the tree, got {total}"
        );

        let mut caches = Caches::default();
        mark_subtree_matched(root, &mut caches);
        assert_eq!(
            count_unmarked_nodes_in_tree(root, &caches, status_before),
            0,
            "every node (including unnamed tokens) is marked, so none should be left unmarked"
        );

        // Unmark two nodes again. The boolean predicate this replaced would have stopped at the
        // first; the count is what the picker's `Unmarked` column ranks by, so it has to see both.
        let stmt = find_first(root, "expression_statement").unwrap();
        let call = find_first(stmt, "call_expression").unwrap();
        caches.before_match.remove(&call.id());
        caches.before_match.remove(&stmt.id());
        assert_eq!(
            count_unmarked_nodes_in_tree(root, &caches, status_before),
            2,
            "two holes anywhere in the tree should count as two, not as one"
        );
    }

    /// The work queue must not drop, duplicate or reorder anything relative to a plain loop -
    /// every worker count has to produce the identical map. Runs over enough synthetic names that
    /// the shared cursor is genuinely contended, with a `scan` that returns `None` for some of
    /// them so the filtering path is covered too.
    #[test]
    fn scan_corpus_returns_the_same_map_at_every_worker_count() {
        let names: Vec<(String, &'static str)> = (0..500)
            .map(|i| (format!("case-{i:03}"), "handmade"))
            .collect();
        // Deliberately a pure function of the name, so the expected map is knowable independently
        // of which thread happened to run which entry.
        let scan = |name: &str| {
            let n: usize = name.trim_start_matches("case-").parse().unwrap();
            (n % 3 != 0).then_some(n * 2)
        };

        let sequential = scan_corpus_with_threads(&names, 1, scan);
        assert_eq!(
            sequential.len(),
            500 - 500usize.div_ceil(3),
            "the sequential baseline itself must drop exactly the None entries"
        );

        for threads in [2, 3, 8, 64] {
            assert_eq!(
                scan_corpus_with_threads(&names, threads, scan),
                sequential,
                "{threads} workers must produce the same map as one"
            );
        }
    }

    /// More workers than entries must not spawn idle threads or lose work - the cursor runs out
    /// immediately for most of them.
    #[test]
    fn scan_corpus_handles_more_workers_than_entries() {
        let names = vec![("only".to_string(), "handmade")];
        assert_eq!(
            scan_corpus_with_threads(&names, 8, |name| Some(name.len())),
            std::collections::HashMap::from([("only".to_string(), 4)])
        );
        assert!(
            scan_corpus_with_threads(&[], 8, |name: &str| Some(name.len())).is_empty(),
            "an empty corpus is not an error"
        );
    }

    /// A panicking worker must take the process down the way a sequential scan always did, rather
    /// than quietly handing back a map missing that thread's share - which would read as `?` in
    /// the picker and be indistinguishable from "not scanned yet".
    #[test]
    fn scan_corpus_propagates_a_worker_panic() {
        let names: Vec<(String, &'static str)> =
            (0..64).map(|i| (format!("case-{i}"), "handmade")).collect();

        // The default hook would dump a backtrace for a panic this test is deliberately causing.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(|| {
            scan_corpus_with_threads(&names, 4, |name: &str| {
                if name == "case-40" {
                    panic!("scan blew up");
                }
                Some(name.len())
            })
        });

        std::panic::set_hook(hook);

        assert!(result.is_err(), "the panic must not be swallowed");
    }

    #[test]
    fn default_scan_threads_is_at_least_one_and_within_the_cap() {
        let threads = default_scan_threads();
        assert!(
            (1..=MAX_SCAN_THREADS).contains(&threads),
            "got {threads}, outside 1..={MAX_SCAN_THREADS}"
        );
    }

    #[test]
    fn diff_case_unmarked_count_returns_some_for_a_real_case_on_disk() {
        // Full integrated path (`diffs_case_dir`/`code_pair_from_dir`/`human_mapping::load`, not
        // crafted temp files) against whatever's actually checked in under src/test/data/diffs/ -
        // skips rather than fails if none exist yet, same convention
        // `sample_diff_line_count_is_nonzero_for_a_real_sample_on_disk` uses for this repo's
        // optional/local-only test data.
        let Ok(options) = list_available_cases() else {
            return;
        };
        let Some((name, _)) = options.first() else {
            return;
        };
        assert!(
            diff_case_unmarked_count(name).is_some(),
            "a real, on-disk case should always resolve to Some(_), not None"
        );
    }

    #[test]
    fn open_diff_picker_f_on_cmpl_uses_the_cached_unmarked_map_without_recomputing_it() {
        let view = column_view(DiffColumn::Cmpl);

        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        // Pre-seeded, so this exercises only the filter - not the lazy full-corpus scan (see
        // `open_diff_picker_f_computes_the_unmarked_map_lazily_when_not_yet_cached` for that).
        app.diff_unmarked = Some(std::collections::HashMap::from([
            ("alpha".to_string(), 3),
            ("bravo".to_string(), 0),
        ]));
        app.modal = Some(Modal::OpenDiffPicker {
            options: vec![
                ("alpha".to_string(), "handmade"),
                ("bravo".to_string(), "handmade"),
            ],
            selected: 0,
            view,
            name_input: None,
        });
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        handle_modal_key(
            &mut app,
            KeyCode::Char('f'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert_eq!(
            app.diff_view.filters.cmpl,
            FlagFilter::Yes,
            "f should persist the new filter on App too, so the next o reopens with it"
        );
        match &app.modal {
            Some(Modal::OpenDiffPicker { view, options, .. }) => {
                assert_eq!(view.filters.cmpl, FlagFilter::Yes);
                assert_eq!(
                    options.len(),
                    2,
                    "the full options list itself is untouched"
                );
            }
            other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
        }
        assert_eq!(
            app.diff_unmarked.as_ref().unwrap().len(),
            2,
            "an already-cached map should not be recomputed"
        );
    }

    #[test]
    fn open_diff_picker_f_computes_the_unmarked_map_lazily_when_not_yet_cached() {
        // Full integrated path against whatever's actually under src/test/data/diffs/ - skips if
        // there's nothing to scan, same convention as
        // `diff_case_unmarked_count_returns_some_for_a_real_case_on_disk` above.
        let Ok(options) = list_available_cases() else {
            return;
        };
        if options.is_empty() {
            return;
        }

        let source = "fn main() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            HumanMapping::default(),
        );
        assert!(app.diff_unmarked.is_none());

        let view = column_view(DiffColumn::Unmarked);
        app.modal = Some(Modal::OpenDiffPicker {
            options: options.clone(),
            selected: 0,
            view,
            name_input: None,
        });
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        handle_modal_key(
            &mut app,
            KeyCode::Char('s'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        let map = app
            .diff_unmarked
            .as_ref()
            .expect("s on Unmarked should compute the map lazily when it wasn't cached yet");
        // Cases that fail to load are deliberately absent rather than present with a made-up
        // count, so an equality would be wrong - but a *loose* upper bound alone would pass even
        // if the parallel work queue silently dropped most of the corpus, which is the thing
        // worth guarding here. `scan_corpus_returns_the_same_map_at_every_worker_count` proves
        // the queue itself loses nothing; this only has to prove the real scan is wired to it and
        // reaches nearly every case.
        assert!(map.len() <= options.len());
        assert!(
            map.len() * 10 >= options.len() * 9,
            "the scan covered only {} of {} cases - the work queue is dropping fixtures",
            map.len(),
            options.len()
        );
    }

    #[test]
    fn flatten_visible_skips_hidden_subtree_entirely_but_keeps_siblings() {
        let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
        let root = tree.root_node();
        let (stmt_a, stmt_b) = two_statements(root);

        let mut hidden = std::collections::HashSet::new();
        hidden.insert(stmt_a.id());

        let flat = FlatIndex::new(flatten_visible(
            root,
            &std::collections::HashSet::new(),
            Some(&hidden),
        ));

        assert!(
            !flat.iter().any(|(n, _)| n.id() == stmt_a.id()),
            "hidden node itself should not appear"
        );
        assert!(
            flat.iter().any(|(n, _)| n.id() == stmt_b.id()),
            "sibling of a hidden node should still appear"
        );
        // The root is always an ancestor of the still-visible sibling, so it must survive too.
        assert!(flat.iter().any(|(n, _)| n.id() == root.id()));
    }

    /// The synthetic-Caches tests above prove `fully_solved_nodes` is correct *given* every node
    /// in a subtree (including unnamed tokens like `;`, `{`, `}`) has an entry. They don't prove
    /// the real marking path actually produces that. `M` (`auto_match_pair`) is the tool the docs
    /// point people at for matching a whole subtree at once, specifically so `H` has something to
    /// hide -- this drives it for real, on a real (unchanged) pair, and checks the result through
    /// the same `rebuild_caches` -> `fully_solved_nodes` pipeline the running app uses.
    #[test]
    fn fully_solved_nodes_hides_a_subtree_matched_for_real_via_m() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let (before_stmt_a, _) = two_statements(before_root);
        let (after_stmt_a, _) = two_statements(after_root);
        let (_, before_stmt_b) = two_statements(before_root);

        let mut mapping = HumanMapping::default();
        let mut matched = 0usize;
        let mut skipped = 0usize;
        let mut before_collapsed = std::collections::HashSet::new();
        let mut after_collapsed = std::collections::HashSet::new();
        let before_paths = precompute_paths(before_root);
        let after_paths = precompute_paths(after_root);
        let mut touched_before = std::collections::HashSet::new();
        let mut touched_after = std::collections::HashSet::new();

        auto_match_pair(
            &mut mapping.entries,
            &mut touched_before,
            &mut touched_after,
            &Caches::default(),
            before_stmt_a,
            after_stmt_a,
            source.as_bytes(),
            source.as_bytes(),
            &before_paths,
            &after_paths,
            &mut matched,
            &mut skipped,
            &mut before_collapsed,
            &mut after_collapsed,
        );

        let caches = rebuild_caches(&mapping.entries, before_root, after_root);
        assert_eq!(
            caches.unresolved, 0,
            "every entry M produced should resolve: {:?}",
            mapping.entries
        );

        let solved = fully_solved_nodes(before_root, &caches, status_before);
        assert!(
            solved.contains(&before_stmt_a.id()),
            "M should mark every child (including unnamed tokens), fully solving the subtree: {:?}",
            mapping.entries
        );
        assert!(
            !solved.contains(&before_stmt_b.id()),
            "b(); was never matched, so it must not be treated as solved"
        );
    }

    #[test]
    fn action_match_to_end_matches_identical_trees_completely() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let no_hashes = rustc_hash::FxHashMap::default();

        let outcome = action_match_to_end(
            &mut app,
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
        )
        .unwrap();

        assert!(matches!(outcome, ActionOutcome::Done(_)));
        assert!(app.dirty);

        let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
        for (node, _) in before_flat.iter() {
            assert_ne!(
                status_before(*node, &caches),
                NodeStatus::Unmarked,
                "every node, including unnamed tokens, should be matched: {:?} unmatched",
                node.kind()
            );
        }

        // Running it again once everything is matched must be a no-op, not a duplicate sweep.
        let entries_before = app.mapping.entries.len();
        let outcome = action_match_to_end(
            &mut app,
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
        )
        .unwrap();
        assert!(matches!(outcome, ActionOutcome::Done(ref msg) if msg == "Nothing left to match"));
        assert_eq!(app.mapping.entries.len(), entries_before);
    }

    #[test]
    fn action_match_to_end_stops_at_a_kind_mismatch_but_keeps_prior_matches() {
        let before_source = "fn main() {\n    a();\n}\n";
        let after_source = "fn main() {\n    1;\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let no_hashes = rustc_hash::FxHashMap::default();

        let outcome = action_match_to_end(
            &mut app,
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            before_source.as_bytes(),
            after_source.as_bytes(),
            &no_hashes,
            &no_hashes,
        )
        .unwrap();

        let (before_id, after_id, before_kind, after_kind) = match outcome {
            ActionOutcome::NeedsModal(modal) => match *modal {
                Modal::ConfirmKindMismatch {
                    before_id,
                    after_id,
                    before_kind,
                    after_kind,
                    recursive,
                } => {
                    assert!(
                        !recursive,
                        "f should raise a single-pair mismatch, not a recursive one"
                    );
                    (before_id, after_id, before_kind, after_kind)
                }
                other => panic!("expected ConfirmKindMismatch, got {other:?}"),
            },
            ActionOutcome::Done(msg) => {
                panic!("expected a kind mismatch modal, action completed instead: {msg}")
            }
        };
        assert_ne!(before_kind, after_kind);

        // The common prefix (fn main() { ... before the differing statement) must already be
        // matched, even though the sweep didn't run to completion.
        assert!(app.dirty);
        assert!(
            !app.mapping.entries.is_empty(),
            "should have matched at least the common prefix"
        );

        // The cursor is parked exactly on the mismatched pair, ready for a human (or a plain `m`)
        // to resolve it and then resume with `f` again.
        assert_eq!(app.before.cursor_id, before_id);
        assert_eq!(app.after.cursor_id, after_id);
    }

    fn collect_subtree_ids(node: Node, out: &mut Vec<usize>) {
        out.push(node.id());
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_subtree_ids(child, out);
        }
    }

    #[test]
    fn action_match_to_end_does_not_pair_a_trailing_statement_against_the_wrong_node() {
        // Before has an extra trailing statement the After side has nothing to pair it with.
        // `f` pairs cursors positionally (exactly like repeated `m`), so once the shared `a(); b();`
        // prefix is consumed the After cursor lands on the block's closing `}` while the Before
        // cursor is still sitting on `c();` -- a kind mismatch, so the sweep must stop there rather
        // than inventing a match for `c();`.
        let before_source = "fn main() {\n    a();\n    b();\n    c();\n}\n";
        let after_source = "fn main() {\n    a();\n    b();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let block = find_first(before_root, "block").unwrap();
        let mut cursor = block.walk();
        let statement_c = block
            .children(&mut cursor)
            .filter(|n| n.kind() == "expression_statement")
            .nth(2)
            .expect("before source has three statements");
        let mut untouchable_ids = Vec::new();
        collect_subtree_ids(statement_c, &mut untouchable_ids);

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let no_hashes = rustc_hash::FxHashMap::default();

        let outcome = action_match_to_end(
            &mut app,
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            before_source.as_bytes(),
            after_source.as_bytes(),
            &no_hashes,
            &no_hashes,
        )
        .unwrap();

        assert!(
            app.dirty,
            "the shared a(); b(); prefix should have been matched"
        );
        match outcome {
            ActionOutcome::NeedsModal(modal) => match *modal {
                Modal::ConfirmKindMismatch {
                    before_kind,
                    after_kind,
                    ..
                } => assert_ne!(before_kind, after_kind),
                other => panic!("expected ConfirmKindMismatch, got {other:?}"),
            },
            ActionOutcome::Done(msg) => panic!(
                "expected the sweep to stop on a kind mismatch once `c();` has nothing left to pair \
                 with, action completed instead: {msg}"
            ),
        }

        let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
        for id in untouchable_ids {
            assert!(
                !caches.before_match.contains_key(&id) && !caches.before_removed.contains_key(&id),
                "the trailing `c();` statement (or any of its children) must not have been paired \
                 with anything on the After side"
            );
        }
    }

    /// Regression guard for a real hang: on a ~5,500-node real-world fixture, the original
    /// implementation (which called `apply_match_entry`/`rebuild_caches` -- each O(current entry
    /// count) -- once per node, and re-derived the cursor's flat-array position by linear scan
    /// every iteration) took 26s to reach only 740 of 5477 matches, and was still accelerating: an
    /// effective hang for anything but a toy fixture. Generates a large *synthetic* identical
    /// before/after tree (no dependency on any file under src/test/data/, which can be renamed or
    /// removed) and asserts the sweep finishes fast. If this regresses back to quadratic, this
    /// test will time out or take drastically longer, not just get slower by a little.
    #[test]
    fn action_match_to_end_is_linear_not_quadratic_in_tree_size() {
        let mut source = String::from("fn main() {\n");
        for i in 0..3000 {
            source.push_str(&format!("    a{i}();\n"));
        }
        source.push_str("}\n");

        let before_tree = parse_rust(&source);
        let after_tree = parse_rust(&source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let after_flat = FlatIndex::new(flatten_visible(
            after_root,
            &std::collections::HashSet::new(),
            None,
        ));
        assert!(
            before_flat.len() > 10_000,
            "expected a large tree, got {} nodes",
            before_flat.len()
        );

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        let no_hashes = rustc_hash::FxHashMap::default();

        let start = std::time::Instant::now();
        let outcome = action_match_to_end(
            &mut app,
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert!(matches!(outcome, ActionOutcome::Done(_)));
        assert_eq!(app.mapping.entries.len(), before_flat.len());
        assert!(
            elapsed.as_secs() < 5,
            "took {elapsed:?} to sweep {} nodes -- the old O(n^2) implementation took 26s to do \
             barely a tenth of a ~5,500-node real fixture, so anything anywhere near that here \
             means the quadratic blowup is back",
            before_flat.len()
        );
    }

    /// `M`'s recursive workhorse (`auto_match_pair`) had the same O(n^2) shape `f` did, for the
    /// same reason: it called `apply_match_entry` (O(current entry count) per call, via
    /// `remove_direct_entries_for`'s full scan) once per node in the subtree. On the same
    /// ~5,500-node real fixture matched against itself (so `same_shape` holds all the way down and
    /// the whole tree gets recursed), the original implementation didn't finish within 2 minutes.
    /// Mirrors `action_match_to_end_is_linear_not_quadratic_in_tree_size`'s synthetic large tree so
    /// this doesn't depend on any file under src/test/data/.
    #[test]
    fn action_match_subtree_is_linear_not_quadratic_in_tree_size() {
        let mut source = String::from("fn main() {\n");
        for i in 0..3000 {
            source.push_str(&format!("    a{i}();\n"));
        }
        source.push_str("}\n");

        let before_tree = parse_rust(&source);
        let after_tree = parse_rust(&source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let after_flat = FlatIndex::new(flatten_visible(
            after_root,
            &std::collections::HashSet::new(),
            None,
        ));
        assert!(
            before_flat.len() > 10_000,
            "expected a large tree, got {} nodes",
            before_flat.len()
        );

        let mut mapping = HumanMapping::default();
        let caches = Caches::default();
        let mut before_collapsed = std::collections::HashSet::new();
        let mut after_collapsed = std::collections::HashSet::new();
        let no_hashes = rustc_hash::FxHashMap::default();

        let start = std::time::Instant::now();
        let outcome = action_match_subtree(
            &mut mapping,
            &before_flat,
            &after_flat,
            before_root.id(),
            after_root.id(),
            before_root,
            after_root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &mut before_collapsed,
            &mut after_collapsed,
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert!(matches!(outcome, ActionOutcome::Done(_)));
        assert_eq!(
            mapping.entries.len(),
            before_flat.len(),
            "M should match every node in the identical subtree"
        );
        assert!(
            elapsed.as_secs() < 5,
            "took {elapsed:?} to sweep {} nodes -- the old O(n^2) implementation didn't finish \
             within 2 minutes on a comparably sized real fixture, so anything anywhere near that \
             here means the quadratic blowup is back",
            before_flat.len()
        );
    }

    #[test]
    fn m_preserves_a_pre_existing_match_under_a_subtree_it_bails_out_of() {
        // Shapes: `if true { a(); }` before vs `if true { a(); c(); }` after -- the `if`'s inner
        // block has 3 children before, 4 after, so `auto_match_pair` bails at that block (pushes
        // one MatchButNotIdentical for the block itself, does not recurse into its children).
        // `a();`'s `expression_statement` sits *below* that bail point, so `M` (pressed above it,
        // at the whole function) should never touch its pre-existing entry.
        let before_source = "fn main() {\n    if true {\n        a();\n    }\n    b();\n}\n";
        let after_source =
            "fn main() {\n    if true {\n        a();\n        c();\n    }\n    b();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let inner_block_before = find_first(before_root, "if_expression")
            .and_then(|n| find_first(n, "block"))
            .unwrap();
        let inner_block_after = find_first(after_root, "if_expression")
            .and_then(|n| find_first(n, "block"))
            .unwrap();
        let d_before = inner_block_before
            .child(1)
            .filter(|n| n.kind() == "expression_statement")
            .expect("a(); statement");
        let d_after = inner_block_after
            .child(1)
            .filter(|n| n.kind() == "expression_statement")
            .expect("a(); statement");

        let mut mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(d_before)),
                after_path: Some(path_for_node(d_after)),
            }],
            ..Default::default()
        };

        let function_before = before_root.child(0).unwrap();
        let function_after = after_root.child(0).unwrap();
        let before_flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let after_flat = FlatIndex::new(flatten_visible(
            after_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let no_hashes = rustc_hash::FxHashMap::default();
        let mut before_collapsed = std::collections::HashSet::new();
        let mut after_collapsed = std::collections::HashSet::new();

        action_match_subtree(
            &mut mapping,
            &before_flat,
            &after_flat,
            function_before.id(),
            function_after.id(),
            before_root,
            after_root,
            &Caches::default(),
            before_source.as_bytes(),
            after_source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &mut before_collapsed,
            &mut after_collapsed,
        )
        .unwrap();

        let caches = rebuild_caches(&mapping.entries, before_root, after_root);
        assert_eq!(
            caches.before_match.get(&d_before.id()),
            Some(&d_after.id()),
            "M bailed at an ancestor without recursing into `a();` -- its pre-existing match should \
             survive untouched, not be silently dropped: {:?}",
            mapping.entries
        );
    }

    #[test]
    fn m_replaces_a_pre_existing_match_on_a_node_it_actually_revisits() {
        // Identical before/after: `same_shape` holds at every level, so `M` pressed at the root
        // recurses all the way down and revisits every node, including `a();`. Pre-seed a *wrong*
        // pre-existing entry for `a();` (pointing at `b();` instead of its own counterpart) and
        // confirm `M` replaces it with exactly one correct entry -- not a leftover stale one
        // alongside the new one, which would silently corrupt the saved mapping.
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let (before_stmt_a, _) = two_statements(before_root);
        let (after_stmt_a, after_stmt_b) = two_statements(after_root);

        let mut mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(before_stmt_a)),
                after_path: Some(path_for_node(after_stmt_b)), // deliberately wrong partner
            }],
            ..Default::default()
        };

        let function_before = before_root.child(0).unwrap();
        let function_after = after_root.child(0).unwrap();
        let before_flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let after_flat = FlatIndex::new(flatten_visible(
            after_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let no_hashes = rustc_hash::FxHashMap::default();
        let mut before_collapsed = std::collections::HashSet::new();
        let mut after_collapsed = std::collections::HashSet::new();

        action_match_subtree(
            &mut mapping,
            &before_flat,
            &after_flat,
            function_before.id(),
            function_after.id(),
            before_root,
            after_root,
            &Caches::default(),
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &mut before_collapsed,
            &mut after_collapsed,
        )
        .unwrap();

        let matching: Vec<_> = mapping
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .before_path
                    .as_ref()
                    .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
                    .is_some_and(|n| n.id() == before_stmt_a.id())
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one entry for `a();` after M, found {}: {:?}",
            matching.len(),
            mapping.entries
        );

        let caches = rebuild_caches(&mapping.entries, before_root, after_root);
        assert_eq!(
            caches.before_match.get(&before_stmt_a.id()),
            Some(&after_stmt_a.id()),
            "M should have replaced the stale wrong-partner entry with the correct one: {:?}",
            mapping.entries
        );
    }

    fn write_csv(path: &Path, rows: &[(&str, &str, &str, &str, &str, &str)]) {
        let mut writer = csv::Writer::from_path(path).unwrap();
        writer
            .write_record([
                "language",
                "repository",
                "commit",
                "path",
                "promoted_to",
                "dataset",
            ])
            .unwrap();
        for (language, repository, commit, row_path, promoted_to, dataset) in rows {
            writer
                .write_record([language, repository, commit, row_path, promoted_to, dataset])
                .unwrap();
        }
        writer.flush().unwrap();
    }

    /// (path, promoted_to, dataset) per row - the three columns every test in this section
    /// actually cares about; language/repository/commit are only there to match rows.
    fn read_csv(path: &Path) -> Vec<(String, String, String)> {
        let mut reader = csv::Reader::from_path(path).unwrap();
        reader
            .records()
            .map(|r| {
                let r = r.unwrap();
                (
                    r[3].to_string(),
                    r.get(4).unwrap_or("").to_string(),
                    r.get(5).unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn update_sample_csv_sets_promoted_to_on_the_matching_row_only() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[
                ("Rust", "repo", "abc123", "src/a.rs", "", "small"),
                ("Rust", "repo", "def456", "src/b.rs", "", "full"),
            ],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
        assert!(found);

        let rows = read_csv(file.path());
        assert_eq!(
            rows,
            vec![
                (
                    "src/a.rs".to_string(),
                    "rust-new-case".to_string(),
                    "small".to_string()
                ),
                (
                    "src/b.rs".to_string(),
                    "".to_string(),
                    // Every other row's dataset must survive untouched, same as its other
                    // columns - this is the one column a naive "just rewrite promoted_to"
                    // implementation could plausibly clobber.
                    "full".to_string()
                ),
            ]
        );
    }

    #[test]
    fn update_sample_csv_returns_false_when_no_row_matches() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "other-repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
        assert!(!found);

        // Untouched: no row matched, so nothing should have been rewritten.
        let rows = read_csv(file.path());
        assert_eq!(
            rows,
            vec![("src/a.rs".to_string(), "".to_string(), "small".to_string())]
        );
    }

    #[test]
    fn update_sample_csv_returns_false_when_file_does_not_exist() {
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found =
            update_sample_csv_at(Path::new("/nonexistent/sample.csv"), &source, "name").unwrap();
        assert!(!found);
    }

    #[test]
    fn sample_triage_statuses_at_reads_the_status_column_for_every_row() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[
                (
                    "Rust",
                    "repo",
                    "abc123",
                    "src/a.rs",
                    "rust-already-promoted",
                    "small",
                ),
                ("Rust", "repo", "def456", "src/b.rs", "", "small"),
            ],
        );
        // Reject the second row so the map has one of each of the three statuses (the third being
        // whatever `write_csv` alone leaves as its backward-compat default, exercised by the
        // no-`status`-column case below).
        let rejected_source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "def456".to_string(),
            path: "src/b.rs".to_string(),
            dataset: "small".to_string(),
        };
        reject_sample_csv_at(file.path(), &rejected_source, "not interesting").unwrap();

        let statuses = sample_triage_statuses_at(file.path()).unwrap();
        assert_eq!(
            statuses.get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "abc123".to_string(),
                "src/a.rs".to_string(),
            )),
            Some(&SampleTriageStatus::Promoted)
        );
        assert_eq!(
            statuses.get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "def456".to_string(),
                "src/b.rs".to_string(),
            )),
            Some(&SampleTriageStatus::Rejected)
        );
    }

    #[test]
    fn sample_triage_statuses_at_defaults_an_unmatched_row_to_sampled() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );

        let statuses = sample_triage_statuses_at(file.path()).unwrap();
        assert_eq!(
            statuses.get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "abc123".to_string(),
                "src/a.rs".to_string(),
            )),
            Some(&SampleTriageStatus::Sampled)
        );
    }

    #[test]
    fn sample_triage_statuses_at_is_empty_when_file_does_not_exist() {
        let statuses = sample_triage_statuses_at(Path::new("/nonexistent/sample.csv")).unwrap();
        assert!(statuses.is_empty());
    }

    /// (path, promoted_to, dataset, status, comment) per row - like `read_csv`, but for tests that
    /// also care about the two newest columns.
    fn read_csv_with_status(path: &Path) -> Vec<(String, String, String, String, String)> {
        let mut reader = csv::Reader::from_path(path).unwrap();
        reader
            .records()
            .map(|r| {
                let r = r.unwrap();
                (
                    r[3].to_string(),
                    r.get(4).unwrap_or("").to_string(),
                    r.get(5).unwrap_or("").to_string(),
                    r.get(6).unwrap_or("").to_string(),
                    r.get(7).unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn update_sample_csv_sets_status_to_promoted() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        assert!(update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap());

        let rows = read_csv_with_status(file.path());
        assert_eq!(
            rows,
            vec![(
                "src/a.rs".to_string(),
                "rust-new-case".to_string(),
                "small".to_string(),
                "PROMOTED".to_string(),
                "".to_string(),
            )]
        );
    }

    #[test]
    fn reject_sample_csv_at_sets_reason_and_status_without_touching_promoted_to() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[
                ("Rust", "repo", "abc123", "src/a.rs", "", "small"),
                ("Rust", "repo", "def456", "src/b.rs", "", "full"),
            ],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found =
            reject_sample_csv_at(file.path(), &source, "duplicate of an existing case").unwrap();
        assert!(found);

        let rows = read_csv_with_status(file.path());
        assert_eq!(
            rows,
            vec![
                (
                    "src/a.rs".to_string(),
                    // Rejection must never populate promoted_to.
                    "".to_string(),
                    "small".to_string(),
                    "REJECTED".to_string(),
                    "duplicate of an existing case".to_string(),
                ),
                (
                    "src/b.rs".to_string(),
                    "".to_string(),
                    "full".to_string(),
                    "SAMPLED".to_string(),
                    "".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn reject_sample_csv_at_returns_false_when_no_row_matches() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "other-repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found = reject_sample_csv_at(file.path(), &source, "reason").unwrap();
        assert!(!found);

        // Untouched: no row matched, so the file is never rewritten -- still the original
        // (pre-`status`/`comment`) 6-column shape, not a backfilled 8-column one.
        let rows = read_csv_with_status(file.path());
        assert_eq!(
            rows,
            vec![(
                "src/a.rs".to_string(),
                "".to_string(),
                "small".to_string(),
                "".to_string(),
                "".to_string(),
            )]
        );
    }

    #[test]
    fn set_sample_comment_at_sets_comment_without_touching_status_or_promoted_to() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[
                (
                    "Rust",
                    "repo",
                    "abc123",
                    "src/a.rs",
                    "rust-already-promoted",
                    "small",
                ),
                ("Rust", "repo", "def456", "src/b.rs", "", "full"),
            ],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        let found = set_sample_comment_at(file.path(), &source, "worth a second look").unwrap();
        assert!(found);

        let rows = read_csv_with_status(file.path());
        assert_eq!(
            rows,
            vec![
                (
                    "src/a.rs".to_string(),
                    // A comment must never touch promoted_to or status - unlike reject, this can
                    // be set on an already-PROMOTED row without disturbing either.
                    "rust-already-promoted".to_string(),
                    "small".to_string(),
                    "PROMOTED".to_string(),
                    "worth a second look".to_string(),
                ),
                (
                    "src/b.rs".to_string(),
                    "".to_string(),
                    "full".to_string(),
                    "SAMPLED".to_string(),
                    "".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn set_sample_comment_at_with_an_empty_comment_clears_a_previous_one() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        assert!(set_sample_comment_at(file.path(), &source, "first note").unwrap());
        assert!(set_sample_comment_at(file.path(), &source, "").unwrap());

        let rows = read_csv_with_status(file.path());
        assert_eq!(rows[0].4, "");
    }

    #[test]
    fn set_sample_comment_at_returns_false_when_no_row_matches() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "other-repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        assert!(!set_sample_comment_at(file.path(), &source, "note").unwrap());
    }

    #[test]
    fn sample_comment_at_returns_the_trimmed_comment_when_present() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        set_sample_comment_at(file.path(), &source, "  needs a closer look  ").unwrap();

        assert_eq!(
            sample_comment_at(file.path(), &source).unwrap(),
            Some("needs a closer look".to_string())
        );
    }

    #[test]
    fn sample_comment_at_is_none_when_comment_is_empty_or_row_is_missing() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
        );
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
            dataset: "small".to_string(),
        };
        // Present row, never-set comment.
        assert_eq!(sample_comment_at(file.path(), &source).unwrap(), None);

        // No matching row at all.
        let missing_source = SampleSource {
            repository: "other-repo".to_string(),
            ..source
        };
        assert_eq!(
            sample_comment_at(file.path(), &missing_source).unwrap(),
            None
        );
    }

    #[test]
    fn algo_reason_reports_the_pass_that_produced_each_side_of_a_match() {
        let source = "fn f() { a(); }\n";
        let before = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
        let after = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.expect("diff has AST");

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        // Identical before/after source: the whole tree matches via a single hash comparison at
        // the root, so both roots should report `IdenticalHash` - and nothing further down should
        // even have its own entry (see `add_delete_mappings`'s sibling passes: a hash-matched
        // subtree's descendants are never visited individually).
        let before_reason = algo_reason_before(before_ast.root_node(), &diff_ast);
        let after_reason = algo_reason_after(after_ast.root_node(), &diff_ast);
        assert_eq!(before_reason, Some(ASTMappingReason::IdenticalHash));
        assert_eq!(after_reason, Some(ASTMappingReason::IdenticalHash));
        assert_eq!(reason_label(before_reason.unwrap()), "IdHash");
    }

    #[test]
    fn algo_reason_is_none_when_the_diff_has_no_entry_for_the_node() {
        // A fresh, unpopulated ASTDiff has no entries at all, so every lookup should miss cleanly
        // rather than panicking - this is the state before `p` has ever been pressed.
        let source = "fn f() {}\n";
        let code = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
        let root = code.ast.as_ref().unwrap().root_node();
        let empty_diff = ASTDiff::default();

        assert_eq!(algo_reason_before(root, &empty_diff), None);
        assert_eq!(algo_reason_after(root, &empty_diff), None);
    }

    #[test]
    fn reason_label_matches_benchmark_optimal_solutions_abbreviations() {
        // Kept in sync by hand with `src/bin/benchmark_optimal_solutions.rs`'s `REASONS` table -
        // this test exists so a label drift between the two tools fails loudly instead of quietly
        // making the same abbreviation mean two different things.
        assert_eq!(reason_label(ASTMappingReason::IdenticalHash), "IdHash");
        assert_eq!(
            reason_label(ASTMappingReason::IdenticalHashOfAncestor),
            "IdHashAnc"
        );
        assert_eq!(
            reason_label(ASTMappingReason::FullyMappingSubtrees),
            "FullMap"
        );
        assert_eq!(
            reason_label(ASTMappingReason::StructurallyIdenticalSubtrees),
            "StructId"
        );
        assert_eq!(
            reason_label(ASTMappingReason::StructurallyIdenticalAncestor),
            "StructAnc"
        );
        assert_eq!(reason_label(ASTMappingReason::OptimalIDU), "OptIDU");
        assert_eq!(reason_label(ASTMappingReason::APTED("final_pass")), "APTED");
        assert_eq!(reason_label(ASTMappingReason::FlatSequenceDiff), "FlatSeq");
        assert_eq!(reason_label(ASTMappingReason::MovedSubtree), "Moved");
        assert_eq!(reason_label(ASTMappingReason::LeadingSibling), "LeadSib");
        assert_eq!(
            reason_label(ASTMappingReason::GreedyAnchorBlock),
            "GreedyAnchor"
        );
        assert_eq!(
            reason_label(ASTMappingReason::BottomUpPropagation),
            "BottomUpProp"
        );
    }

    #[test]
    fn reason_detail_shows_apted_provenance_but_reason_label_does_not() {
        let reason = ASTMappingReason::APTED("bottom_up_expansion");
        assert_eq!(reason_label(reason), "APTED");
        assert_eq!(reason_detail(reason), "APTED:bottom_up_expansion");
        // Every other variant has no payload to show, so `reason_detail` just falls back to the
        // same short label as `reason_label`.
        assert_eq!(
            reason_detail(ASTMappingReason::BottomUpPropagation),
            "BottomUpProp"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Multi-map groups (Phase 2: x/c selection, m/M commit, u removal)
    // -----------------------------------------------------------------------------------------

    #[test]
    fn multi_map_group_operation_is_identical_only_when_every_member_shares_one_hash() {
        let mut before_hash = rustc_hash::FxHashMap::default();
        let mut after_hash = rustc_hash::FxHashMap::default();
        before_hash.insert(1, 42);
        before_hash.insert(2, 42);
        after_hash.insert(10, 42);
        after_hash.insert(11, 42);

        let before_ids: std::collections::BTreeSet<usize> = [1, 2].into_iter().collect();
        let after_ids: std::collections::BTreeSet<usize> = [10, 11].into_iter().collect();

        assert_eq!(
            multi_map_group_operation(&before_ids, &after_ids, &before_hash, &after_hash),
            HumanOperation::Identical
        );

        after_hash.insert(11, 99);
        assert_eq!(
            multi_map_group_operation(&before_ids, &after_ids, &before_hash, &after_hash),
            HumanOperation::MatchButNotIdentical
        );
    }

    #[test]
    fn commit_multi_map_group_replaces_any_prior_entry_touching_its_nodes() {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);
        assert_eq!(before_foos.len(), 3);
        assert_eq!(after_foos.len(), 2);

        // A pre-existing plain entry pairing the first before-foo with the first after-foo, which
        // the new group commit (covering all 3/2) should displace.
        let mut mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(before_foos[0])),
                after_path: Some(path_for_node(after_foos[0])),
            }],
            ..Default::default()
        };

        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();

        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            HumanOperation::Identical,
            true,
        )
        .unwrap();

        assert!(
            mapping.entries.is_empty(),
            "the pre-existing plain entry touching a group member should be removed: {:?}",
            mapping.entries
        );
        assert_eq!(mapping.groups.len(), 1);
        assert_eq!(mapping.groups[0].before_paths.len(), 3);
        assert_eq!(mapping.groups[0].after_paths.len(), 2);
        assert!(mapping.groups[0].with_children);
    }

    #[test]
    fn commit_multi_map_group_replaces_a_prior_group_sharing_a_node() {
        let source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);

        let first_pair_before: std::collections::BTreeSet<usize> =
            [before_foos[0].id(), before_foos[1].id()]
                .into_iter()
                .collect();
        let first_pair_after: std::collections::BTreeSet<usize> =
            [after_foos[0].id(), after_foos[1].id()]
                .into_iter()
                .collect();
        let mut mapping = HumanMapping::default();
        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &first_pair_before,
            &first_pair_after,
            HumanOperation::Identical,
            false,
        )
        .unwrap();
        assert_eq!(mapping.groups.len(), 1);

        // A second group sharing before_foos[1] with the first should replace it, not coexist.
        let second_before: std::collections::BTreeSet<usize> =
            [before_foos[1].id(), before_foos[2].id()]
                .into_iter()
                .collect();
        let second_after: std::collections::BTreeSet<usize> =
            [after_foos[2].id()].into_iter().collect();
        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &second_before,
            &second_after,
            HumanOperation::Identical,
            false,
        )
        .unwrap();

        assert_eq!(
            mapping.groups.len(),
            1,
            "the first group should have been dropped, not left alongside the second: {:?}",
            mapping.groups
        );
        assert_eq!(mapping.groups[0].before_paths.len(), 2);
    }

    #[test]
    fn commit_multi_map_group_orders_paths_by_source_position_not_by_arena_id() {
        // A `BTreeSet<usize>` orders by node id, not source position - parse-unstable (same
        // lesson as this project's benchmark-determinism-fix). The committed group's paths must
        // come out in source order regardless, so re-selecting the same nodes in a later session
        // can't shuffle a `human_mapping.json` that otherwise didn't change.
        let source = "fn main() {\n    a();\n    b();\n    c();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let stmts = block_statements(root);
        assert_eq!(stmts.len(), 3);

        let mut mapping = HumanMapping::default();
        let ids: std::collections::BTreeSet<usize> = stmts.iter().map(|n| n.id()).collect();
        commit_multi_map_group(
            &mut mapping,
            root,
            root,
            &ids,
            &ids,
            HumanOperation::Identical,
            false,
        )
        .unwrap();

        let expected: Vec<Vec<String>> = stmts.iter().map(|n| path_for_node(*n)).collect();
        assert_eq!(
            mapping.groups[0].before_paths, expected,
            "paths should be ordered by source position (a, b, c)"
        );
        assert_eq!(mapping.groups[0].after_paths, expected);
    }

    #[test]
    fn commit_multi_map_group_with_children_clears_a_pre_existing_descendant_entry() {
        let before_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);
        assert_eq!(before_foos.len(), 2);
        assert_eq!(after_foos.len(), 1);

        // A stale entry on a descendant of `before_foos[0]` (its inner `call_expression`),
        // pointing at some unrelated after node - exactly what a `with_children` commit must
        // sweep, the same way `d`/`i`'s own `clear_before_descendants`/`clear_after_descendants`
        // calls already do for a plain delete/insert-with-children mark.
        let descendant = find_first(before_foos[0], "call_expression").unwrap();
        let mut mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(descendant)),
                after_path: Some(path_for_node(after_foos[0])),
            }],
            ..Default::default()
        };

        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();

        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            HumanOperation::Identical,
            true,
        )
        .unwrap();

        assert!(
            mapping.entries.is_empty(),
            "the stale descendant entry should have been cleared by the with_children commit: {:?}",
            mapping.entries
        );
        assert_eq!(mapping.groups.len(), 1);
    }

    #[test]
    fn action_commit_multi_map_group_errors_when_a_member_is_under_a_deleted_with_children_ancestor()
     {
        let before_source =
            "fn main() {\n    if true {\n        foo();\n        foo();\n    }\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let if_expr = find_first(before_root, "if_expression").unwrap();
        let inner_block = find_first(if_expr, "block").unwrap();
        let mut cursor = inner_block.walk();
        let before_foos: Vec<Node> = inner_block
            .children(&mut cursor)
            .filter(|n| n.kind() == "expression_statement")
            .collect();
        assert_eq!(before_foos.len(), 2);

        let after_foos = block_statements(after_root);
        assert_eq!(after_foos.len(), 2);

        let mut mapping = HumanMapping::default();
        let mut caches = Caches::default();
        caches.before_removed.insert(if_expr.id(), true);

        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();
        let no_hashes = rustc_hash::FxHashMap::default();

        let result = action_commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            &no_hashes,
            &no_hashes,
            &caches,
            false,
        );
        match result {
            Err(err) => assert!(
                err.to_string()
                    .contains("covered by an ancestor's delete-with-children mark"),
                "{err}"
            ),
            Ok(_) => panic!(
                "expected an error: a selected node sits under an ancestor already marked deleted-with-children"
            ),
        }
        assert!(mapping.groups.is_empty());
    }

    #[test]
    fn action_commit_multi_map_group_errors_when_one_side_is_empty() {
        let source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let mut mapping = HumanMapping::default();
        let before_ids: std::collections::BTreeSet<usize> = block_statements(before_root)
            .iter()
            .map(|n| n.id())
            .collect();
        let after_ids = std::collections::BTreeSet::new();
        let no_hashes = rustc_hash::FxHashMap::default();

        let result = action_commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            &no_hashes,
            &no_hashes,
            &Caches::default(),
            false,
        );
        match result {
            Err(err) => assert!(
                err.to_string()
                    .contains("at least one selected node on both sides"),
                "{err}"
            ),
            Ok(_) => panic!("expected an error when one side of the selection is empty"),
        }
        assert!(mapping.groups.is_empty());
    }

    #[test]
    fn action_commit_multi_map_group_raises_a_modal_for_mixed_kinds() {
        let before_source = "fn main() {\n    foo();\n    let x = 1;\n}\n";
        let after_source = "fn main() {\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let block = find_first(before_root, "block").unwrap();
        let mut cursor = block.walk();
        let before_nodes: Vec<Node> = block
            .children(&mut cursor)
            .filter(|n| n.kind() == "expression_statement" || n.kind() == "let_declaration")
            .collect();
        assert_eq!(before_nodes.len(), 2);
        assert_ne!(before_nodes[0].kind(), before_nodes[1].kind());

        let after_nodes = block_statements(after_root);
        assert_eq!(after_nodes.len(), 1);

        let mut mapping = HumanMapping::default();
        let before_ids: std::collections::BTreeSet<usize> =
            before_nodes.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_nodes.iter().map(|n| n.id()).collect();
        let no_hashes = rustc_hash::FxHashMap::default();

        let outcome = action_commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            &no_hashes,
            &no_hashes,
            &Caches::default(),
            false,
        )
        .unwrap();

        match outcome {
            ActionOutcome::NeedsModal(modal) => match *modal {
                Modal::ConfirmMultiMapGroup { kinds, .. } => {
                    assert!(kinds.len() > 1, "{:?}", kinds);
                }
                other => panic!("expected ConfirmMultiMapGroup, got {other:?}"),
            },
            ActionOutcome::Done(msg) => {
                panic!("expected a mixed-kinds confirmation, action completed instead: {msg}")
            }
        }
        assert!(
            mapping.groups.is_empty(),
            "nothing should commit until the modal is confirmed"
        );
    }

    #[test]
    fn action_commit_multi_map_group_commits_directly_when_kinds_match() {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);
        let mut mapping = HumanMapping::default();
        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();
        let no_hashes = rustc_hash::FxHashMap::default();

        let outcome = action_commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            &no_hashes,
            &no_hashes,
            &Caches::default(),
            true,
        )
        .unwrap();

        assert!(matches!(outcome, ActionOutcome::Done(_)));
        assert_eq!(mapping.groups.len(), 1);
        // No content hashes were supplied (`no_hashes`), so every lookup misses and the group
        // falls back to `MatchButNotIdentical` - see `multi_map_group_operation`.
        assert_eq!(
            mapping.groups[0].operation,
            HumanOperation::MatchButNotIdentical
        );
        assert!(mapping.groups[0].with_children);
    }

    #[test]
    fn action_unmark_on_a_group_member_removes_the_whole_group() {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);

        let mut mapping = HumanMapping::default();
        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();
        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            HumanOperation::Identical,
            false,
        )
        .unwrap();
        assert_eq!(mapping.groups.len(), 1);

        let before_flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let after_flat = FlatIndex::new(flatten_visible(
            after_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);

        // Any one member - here, the leftover before-foo that landed on Delete rather than a
        // matched pair - should be enough to drop the whole group.
        let leftover = before_foos
            .iter()
            .find(|n| {
                caches.before_group.contains_key(&n.id())
                    && !caches.before_match.contains_key(&n.id())
            })
            .expect("exactly one before-foo should be the group's leftover");

        let msg = action_unmark(
            &mut mapping,
            Focus::Before,
            &before_flat,
            &after_flat,
            leftover.id(),
            after_foos[0].id(),
            before_root,
            after_root,
            &caches,
        )
        .unwrap();

        assert!(msg.contains("Removed multi-map group"), "{msg}");
        assert!(mapping.groups.is_empty());
    }

    #[test]
    fn handle_key_x_toggles_multi_select_and_c_clears_both_sides() {
        let source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let before_foos = block_statements(before_root);

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        app.before.cursor_id = before_foos[0].id();
        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let caches = Caches::default();
        let no_hashes = rustc_hash::FxHashMap::default();

        handle_key(
            &mut app,
            KeyCode::Char('x'),
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        assert_eq!(app.before_multi_select.len(), 1);
        assert!(app.before_multi_select.contains(&before_foos[0].id()));

        // Pressing x again on the same node toggles it back out.
        handle_key(
            &mut app,
            KeyCode::Char('x'),
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        assert!(app.before_multi_select.is_empty());

        app.before_multi_select.insert(before_foos[0].id());
        app.after_multi_select.insert(before_foos[1].id());
        handle_key(
            &mut app,
            KeyCode::Char('c'),
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
        assert!(app.before_multi_select.is_empty());
        assert!(app.after_multi_select.is_empty());
    }

    #[test]
    fn handle_key_m_with_a_pending_selection_commits_a_multi_map_group() {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);

        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            before_root.id(),
            after_root.id(),
            HumanMapping::default(),
        );
        app.before_multi_select = before_foos.iter().map(|n| n.id()).collect();
        app.after_multi_select = after_foos.iter().map(|n| n.id()).collect();

        let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
        let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
        let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
        let no_hashes = rustc_hash::FxHashMap::default();

        handle_key(
            &mut app,
            KeyCode::Char('m'),
            &before_flat,
            &after_flat,
            before_root,
            after_root,
            &caches,
            before_source.as_bytes(),
            after_source.as_bytes(),
            &no_hashes,
            &no_hashes,
            &Code::from_string(before_source, &Language::Rust),
            &Code::from_string(after_source, &Language::Rust),
        );

        assert_eq!(app.mapping.groups.len(), 1, "{:?}", app.mapping.groups);
        assert!(app.mapping.entries.is_empty(), "{:?}", app.mapping.entries);
        assert!(app.dirty);
        assert!(
            app.before_multi_select.is_empty() && app.after_multi_select.is_empty(),
            "the selection should be cleared once committed"
        );
        // `m`, not `M`: the committed group should not require subtree closure.
        assert!(!app.mapping.groups[0].with_children);
    }

    #[test]
    fn render_panel_marks_a_group_matched_node_and_a_pending_selection_distinctly() {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let before_foos = block_statements(before_root);
        let after_foos = block_statements(after_root);

        let mut mapping = HumanMapping::default();
        let before_ids: std::collections::BTreeSet<usize> =
            before_foos.iter().map(|n| n.id()).collect();
        let after_ids: std::collections::BTreeSet<usize> =
            after_foos.iter().map(|n| n.id()).collect();
        commit_multi_map_group(
            &mut mapping,
            before_root,
            after_root,
            &before_ids,
            &after_ids,
            HumanOperation::Identical,
            false,
        )
        .unwrap();

        let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
        let flat = FlatIndex::new(flatten_visible(
            before_root,
            &std::collections::HashSet::new(),
            None,
        ));
        let mut panel = PanelState::new(before_root.id());

        // Mark one before-foo (not part of any group) as a pending multi-map selection, to prove
        // it renders distinctly from the already-committed group members.
        let mut pending = std::collections::BTreeSet::new();
        let plain_node = find_first(before_root, "function_item").unwrap();
        pending.insert(plain_node.id());

        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 60, 20);

        terminal
            .draw(|f| {
                render_panel(
                    f,
                    area,
                    "Before",
                    &flat,
                    &mut panel,
                    &caches,
                    Side::Before,
                    before_source.as_bytes(),
                    true,
                    None,
                    false,
                    0,
                    &pending,
                );
            })
            .unwrap();

        // `render_panel` draws inside a bordered `Block`, so row 0 and column 0 of the buffer are
        // the border itself - list content starts at row 1, column 1.
        let content = terminal.backend().buffer().content();
        let plain_row_idx = flat
            .iter()
            .position(|(n, _)| n.id() == plain_node.id())
            .unwrap()
            + 1;
        let group_row_idx = flat
            .iter()
            .position(|(n, _)| n.id() == before_foos[0].id())
            .unwrap()
            + 1;

        let plain_cell = &content[plain_row_idx * 60 + 1];
        assert_eq!(
            plain_cell.fg,
            Color::Magenta,
            "a pending multi-map selection should render in a distinct color"
        );

        let group_row_text: String = (0..60)
            .map(|col| content[group_row_idx * 60 + col].symbol())
            .collect();
        assert!(
            group_row_text.contains('g'),
            "a group-derived match should carry the 'g' marker: {group_row_text:?}"
        );
    }
}
