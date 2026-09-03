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
/// Just the case names from [`list_available_cases`], for the `scan_corpus` calls that only ever
/// look a case up by name (all of them - the dataset half is for the picker's own Dataset column).
fn list_available_case_names() -> Result<Vec<String>> {
    Ok(list_available_cases()?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

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
fn scan_corpus<T, F>(names: &[String], scan: F) -> std::collections::HashMap<String, T>
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
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&names, diff_case_unmarked_count)
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
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    // `Some(..unwrap_or(false))`, not `diff_case_has_text_mapping` directly: this map is keyed on
    // every listed case, with an unreadable one recorded as unpainted rather than left absent -
    // the fail-open direction this scan has always had, and the one place the four scans differ
    // in what they do with a `None`.
    scan_corpus(&names, |name| {
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
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&names, diff_case_disagreement_bytes)
}

/// Every case's note, keyed by case name, for the `o` picker. Cases without one are simply
/// absent.
///
/// The cheapest of the three picker scans by a wide margin: a few hundred bytes per fixture where
/// `compute_diff_text_painted` reads 1.4 GB of JSON and `compute_diff_unmarked` parses both
/// sides with tree-sitter. Most fixtures have no `description.md` at all, so most of this is a
/// failed `stat`.
fn compute_diff_comments() -> std::collections::HashMap<String, String> {
    let Ok(names) = list_available_case_names() else {
        return std::collections::HashMap::new();
    };
    scan_corpus(&names, read_note)
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
mod tests;
