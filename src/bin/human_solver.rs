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
* A helper binary for building the ground-truth AST mappings used by src/test/optimal_solutions.
*
* Run as `cargo run --bin human_solver -- <name>`, where `<name>` is the name of a directory under
* `src/test/data/diffs/` (e.g. "rust-add-if"). If `<name>` is omitted, the first available case
* (alphabetically) opens instead - press `o` to pick a different one. It opens a Ratatui TUI
* showing the TreeSitter ASTs
* of the before and after code side by side (not the source text), lets a human walk both trees
* independently and mark nodes as matching, deleted or inserted, and saves the result as
* `src/test/data/diffs/<name>/human_mapping.json`. It also creates the corresponding
* `src/test/optimal_solutions/<name>.rs` test file (if one doesn't already exist), which simply
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
*                  `diff_inventory` prefers it over the sample.csv comment when writing
*                  diffs.csv, and the prompt pre-fills from that comment when no note exists yet,
*                  so promoting carries one forward. On a sample it still edits sample.csv
*                  instead: prompts for
*                  text, pre-filled with whatever's already recorded, and records it verbatim in
*                  the matching sample.csv row's `comment` column -- unlike `R`, doesn't touch
*                  `status`, works whether the row is SAMPLED/PROMOTED/REJECTED, and an empty
*                  submission clears the comment rather than being rejected as invalid input. If a
*                  comment is present when the sample is later promoted (`s`), it's also written as
*                  a leading comment in the generated optimal_solutions test stub. Has no effect on
*                  a real test case or a git-commit-sourced case, same as `R`
*   o              open a different test case: lists every directory under
*                  src/test/data/diffs/{handmade,small,full,stratified}/, j/k to move, Enter to
*                  open, Esc to cancel. Press `d` inside this picker to cycle which folder it's
*                  narrowed down to (all -> handmade -> small -> full -> stratified -> all - see
*                  DIFF_DATASETS).
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
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

use codediff::code::language::{language_for_path, to_treesitter};
use codediff::code::{Code, Language};
use codediff::diff::text::TextDiff;
use codediff::diff::{ASTDiff, ASTMappingReason, NodeCache, diff_code};
use codediff::test::helper::human_mapping::{
    self, Caches, HumanMapping, HumanMappingEntry, HumanOperation, HumanTextEntry,
    HumanTextMapping, HumanTextOperation, HumanTextSpan, HumanTextVerdict, MarkKind, MultiMapGroup,
    NamedTextMapping, NodeStatus, is_inherited_removed, path_refs, rebuild_caches_for_mapping,
    status_after, status_before,
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
                 it). Tab side, hjkl/0/$/g/G move, v select; d/i paint the
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
o              open a different test case (src/test/data/diffs/); inside, d cycles
                 the dataset (all, handmade, small, full, stratified), H narrows
                 to cases with an unmarked node left (first press scans the whole
                 corpus, a few seconds), X narrows to cases with no text painting
                 yet -- H and X are separate queues, AND-ed; all persist across o
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

/// `options` (`list_available_cases`'s output) narrowed to just `filter`'s dataset - or every
/// entry's name, unsorted-relative-to-each-other-again since `options` is already alphabetical,
/// when `filter` is `None` ("all") - and, if `hide_complete` is set, further narrowed to cases
/// `completeness` marks as having at least one `NodeStatus::Unmarked` node left (see
/// `diff_case_is_incomplete`). A case missing from `completeness` (not yet scanned, or
/// `compute_diff_completeness` couldn't load it) is kept visible under `hide_complete` too - fail
/// open, since hiding something the scan never actually confirmed as done would be misleading.
fn visible_diff_options(
    options: &[(String, &'static str)],
    filter: Option<&'static str>,
    hide_complete: bool,
    completeness: Option<&std::collections::HashMap<String, bool>>,
    hide_painted: bool,
    text_painted: Option<&std::collections::HashMap<String, bool>>,
) -> Vec<String> {
    options
        .iter()
        .filter(|(_, dataset)| filter.is_none_or(|f| *dataset == f))
        .filter(|(name, _)| {
            !hide_complete
                || completeness
                    .and_then(|m| m.get(name))
                    .copied()
                    .unwrap_or(true)
        })
        // Fails open the same way `hide_complete` does, but toward the opposite default: a case
        // the scan never reached is treated as *unpainted* and stays visible, since hiding
        // something never confirmed as done would quietly drop it out of the work queue.
        .filter(|(name, _)| {
            !hide_painted
                || !text_painted
                    .and_then(|m| m.get(name))
                    .copied()
                    .unwrap_or(false)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Builds the `o` picker's modal from a freshly-listed `options`, `current_name` (the case
/// already open, so it starts selected if it's still visible under `dataset_filter`/
/// `hide_complete`), and the persisted `dataset_filter`/`hide_complete` (`App::diff_dataset_filter`/
/// `diff_hide_complete`) - same shape as `open_sample_picker_modal` for `O`, and for the same
/// reason: keeping the real logic here, not in the `KeyCode::Char('o')`/`'d'`/`'H'` handlers,
/// makes it unit-testable without real files under src/test/data/diffs/.
#[allow(clippy::too_many_arguments)]
fn open_diff_picker_modal(
    options: Vec<(String, &'static str)>,
    current_name: &str,
    dataset_filter: Option<&'static str>,
    hide_complete: bool,
    completeness: Option<&std::collections::HashMap<String, bool>>,
    hide_painted: bool,
    text_painted: Option<&std::collections::HashMap<String, bool>>,
) -> Modal {
    let visible = visible_diff_options(
        &options,
        dataset_filter,
        hide_complete,
        completeness,
        hide_painted,
        text_painted,
    );
    let selected = visible
        .iter()
        .position(|name| name == current_name)
        .unwrap_or(0)
        .min(visible.len().saturating_sub(1));
    Modal::OpenDiffPicker {
        options,
        selected,
        dataset_filter,
        hide_complete,
        hide_painted,
    }
}

/// Cycles the `o` picker's dataset filter, in `DIFF_DATASETS` order, wrapping back to "all"
/// (`None`) after the last one - `d`'s handler, same convention as `SampleSortOrder::next`.
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

/// Whether any node in `root`'s subtree has `NodeStatus::Unmarked` under `caches` - short-circuits
/// on the first one found, so only a fully-annotated ("complete") fixture pays for a full walk.
fn tree_has_unmarked_node(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> bool {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if status_fn(node, caches) == NodeStatus::Unmarked {
            return true;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Whether `name`'s current human-authored mapping leaves any node, in either its before or after
/// tree, `NodeStatus::Unmarked` - i.e. whether there's still annotation work left on it. `None` if
/// the case's code or mapping couldn't be loaded at all - no `human_mapping.json` yet, a directory
/// that doesn't parse as a valid case, or (rarer) source that no longer parses. `compute_diff_completeness`
/// treats every one of those the same as `Some(true)` ("needs attention"), rather than silently
/// excluding a broken case from the incomplete-only view - a case that fails to load is exactly
/// the kind of thing this filter should surface, not hide. Pressing Enter on it in the picker
/// still goes through `load_case`'s own error handling as normal; this function doesn't change
/// what opening it does, only whether `H` shows it.
fn diff_case_is_incomplete(name: &str) -> Option<bool> {
    let dir = diffs_case_dir(name)?;
    let (before, after) = code_pair_from_dir(&dir).ok().flatten()?;
    let mapping = human_mapping::load(name).ok()?;
    let before_root = before.ast.as_ref()?.root_node();
    let after_root = after.ast.as_ref()?.root_node();
    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
    Some(
        tree_has_unmarked_node(before_root, &caches, status_before)
            || tree_has_unmarked_node(after_root, &caches, status_after),
    )
}

/// Refreshes just `name`'s entry in `App::diff_completeness`, if the cache has been built at all
/// this session - called after a save, since that's the only way a case's completeness can change
/// mid-session, and a targeted single-fixture refresh is cheap, unlike rebuilding the whole cache
/// (see that field's own doc comment for why that's worth avoiding).
fn refresh_diff_completeness(app: &mut App, name: &str) {
    if let Some(map) = &mut app.diff_completeness
        && let Some(incomplete) = diff_case_is_incomplete(name)
    {
        map.insert(name.to_string(), incomplete);
    }
}

/// Builds `App::diff_completeness` for every case `list_available_cases` currently lists - the
/// `o` picker's `H` toggle needs this for the whole corpus before it can filter, unlike `O`'s
/// `hide_solved` (a cheap lookup against sample.csv, no parsing involved). Practical to run across
/// this repo's whole ~230-fixture corpus (roughly 10s, most of it parsing rather than the
/// unmarked-node check itself) specifically because `rebuild_caches_for_mapping` resolves every
/// entry's path through a `PathCache` rather than rescanning siblings per entry - see
/// `rebuild_caches`'s own doc comment for the very different cost that used to be.
fn compute_diff_completeness() -> std::collections::HashMap<String, bool> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    options
        .into_iter()
        .map(|(name, _)| {
            let incomplete = diff_case_is_incomplete(&name).unwrap_or(true);
            (name, incomplete)
        })
        .collect()
}

/// Whether `name`'s human mapping already carries a painted text-range mapping (see
/// `HumanTextMapping`) - the text-painting counterpart of `diff_case_is_incomplete`.
///
/// **A substring scan, not a JSON parse, and that is not a micro-optimization.** The corpus's 500
/// `human_mapping.json` files come to ~1.4 GB (one is ~29,600 lines on its own), and parsing them
/// all to ask whether one key is present costs about ten seconds - the same order as the `H` scan
/// this was meant to be the cheap counterpart of. Searching for the quoted key instead is a
/// linear scan with no allocation per entry.
///
/// The token includes its quotes deliberately. Every string this file stores is either a
/// tree-sitter node kind or a `kind:index` path element, and `serde_json` escapes any quote inside
/// a string value as `\"` - so a bare `"text_mapping"` can only be a JSON *key*, never content.
/// Key order doesn't matter either, unlike a tail-only read: a hand-edited file that moved the key
/// is still found.
///
/// `None` if the file can't be read, which `compute_diff_text_painted` treats as "not painted" for
/// the same fail-open reason `compute_diff_completeness` treats its failures as "needs attention":
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
/// call sites) as `refresh_diff_completeness`: saving is the only thing that can change a case's
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
/// Much cheaper than `compute_diff_completeness`, which it otherwise mirrors: this scans bytes,
/// where that one parses both source files with tree-sitter and walks two trees. Still done lazily
/// on first `X` rather than eagerly on every `o`, both to match `H`'s behaviour and because 1.4 GB
/// of mapping files is not free to read however cheap the per-file test is.
fn compute_diff_text_painted() -> std::collections::HashMap<String, bool> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    options
        .into_iter()
        .map(|(name, _)| {
            let painted = diff_case_has_text_mapping(&name).unwrap_or(false);
            (name, painted)
        })
        .collect()
}

/// Every case's note, keyed by case name, for the `o` picker. Cases without one are simply
/// absent.
///
/// The cheapest of the three picker scans by a wide margin: a few hundred bytes per fixture where
/// `compute_diff_text_painted` reads 1.4 GB of JSON and `compute_diff_completeness` parses both
/// sides with tree-sitter. Most fixtures have no `description.md` at all, so most of this is a
/// failed `stat`.
fn compute_diff_comments() -> std::collections::HashMap<String, String> {
    let Ok(options) = list_available_cases() else {
        return std::collections::HashMap::new();
    };
    options
        .into_iter()
        .filter_map(|(name, _)| read_note(&name).map(|note| (name, note)))
        .collect()
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
// Git-commit source (`C`): building a handmade diffs/ case directly from this repository's own
// history, instead of from a materialized sample. Shells out to the system `git` (same approach
// as `run_unix_diff`'s `diff -u`) rather than linking `git2`: that crate is already a dependency,
// but only behind the `stats` feature (it pulls in openssl/libssh2 build deps for the research
// sampling tools), and this binary only requires `test-fixtures` - adding `stats` here just to
// list commits and read blobs would be a heavy new build requirement for a TUI tool that
// previously needed none of it.
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

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Before,
    After,
}

impl Focus {
    fn toggle(self) -> Self {
        match self {
            Focus::Before => Focus::After,
            Focus::After => Focus::Before,
        }
    }
}

struct PanelState {
    cursor_id: usize,
    collapsed: std::collections::HashSet<usize>,
    scroll: usize,
    /// Number of rows available for list content, as of the last render (`render_panel`'s
    /// `inner_height`). Used by `action_align` to decide whether a node is currently on screen and,
    /// if not, how big a window to center it in -- 0 until the first frame has been drawn.
    viewport_height: usize,
}

impl PanelState {
    fn new(root_id: usize) -> Self {
        Self {
            cursor_id: root_id,
            collapsed: std::collections::HashSet::new(),
            scroll: 0,
            viewport_height: 0,
        }
    }
}

/// The overlay theme this session paints with, read once at startup from the same
/// `.codediff.toml` the `codediff` binary uses - so a theme picked there applies here too, and
/// Dracula (that file's own default) applies when nothing was ever picked.
///
/// A process-wide `OnceLock` rather than a field threaded through every render function, for the
/// same reason `tui::theme` keeps its custom palette process-global: `palette()` stays the single
/// resolution point and the dozen call sites below stay unchanged. human_solver has no theme
/// picker of its own, so there is nothing to change it at runtime and nothing `OnceLock` costs.
static OVERLAY_THEME: std::sync::OnceLock<OverlayTheme> = std::sync::OnceLock::new();

/// The colors to paint with. Falls back to the default theme (Dracula) when `main` hasn't
/// installed one - which is exactly the case in this file's own render tests, keeping them
/// deterministic instead of dependent on whatever `.codediff.toml` the machine happens to have.
fn overlay_palette() -> OverlayPalette {
    OVERLAY_THEME.get().copied().unwrap_or_default().palette()
}

/// Names offered first in the save-as picker, in this order.
///
/// Suggestions, not a schema. `Minimal` and `Full` are the two ends of the commonest genuine
/// ambiguity - the same edit painted as tightly as it can be, or as generously - and
/// `Only one solution` records the opposite finding, that this fixture has exactly one defensible
/// answer. Nothing requires any of them: the picker's last entry takes a free-form name, a fixture
/// may carry one painting or five, and a painting may be empty.
const SUGGESTED_SOLUTION_NAMES: &[&str] = &["Minimal", "Full", "Only one solution"];

/// Which painting to start on when a case is opened: its first existing one, or the first
/// suggestion when it has none.
fn starting_solution(mapping: &HumanMapping) -> String {
    mapping
        .text_mappings
        .first()
        .map(|named| named.name.clone())
        .unwrap_or_else(|| SUGGESTED_SOLUTION_NAMES[0].to_string())
}

/// The names offered by the save-as picker: every painting this fixture already has, then whichever
/// suggestions it doesn't. Existing names come first so the common case - resaving the painting
/// currently being edited - is at the top rather than buried under three constants.
fn solution_picker_names(mapping: &HumanMapping) -> Vec<String> {
    let mut names: Vec<String> = mapping
        .text_mappings
        .iter()
        .map(|named| named.name.clone())
        .collect();
    for suggestion in SUGGESTED_SOLUTION_NAMES {
        if !names.iter().any(|name| name == suggestion) {
            names.push((*suggestion).to_string());
        }
    }
    names
}

/// The entries of the painting named `solution`, or an empty slice if this fixture has no painting
/// under that name yet.
fn solution_entries<'a>(mapping: &'a HumanMapping, solution: &str) -> &'a [HumanTextEntry] {
    mapping
        .text_mappings
        .iter()
        .find(|named| named.name == solution)
        .map(|named| named.mapping.entries.as_slice())
        .unwrap_or(&[])
}

/// The entries of the painting named `solution`, creating it if this is the first range painted
/// under that name.
///
/// Creating it here is what turns "this fixture has no painting called X" into "it has one", which
/// is the distinction `text_mappings` carries in place of an `Option` - so a painting that ends up
/// empty still has to be reached deliberately, via `Z`.
fn solution_entries_mut<'a>(
    mapping: &'a mut HumanMapping,
    solution: &str,
) -> &'a mut Vec<HumanTextEntry> {
    if !mapping
        .text_mappings
        .iter()
        .any(|named| named.name == solution)
    {
        mapping.text_mappings.push(NamedTextMapping {
            name: solution.to_string(),
            mapping: HumanTextMapping::default(),
        });
    }
    &mut mapping
        .text_mappings
        .iter_mut()
        .find(|named| named.name == solution)
        .expect("just inserted if missing")
        .mapping
        .entries
}

/// `s`: stores the current painting under another name, **keeping the one it came from**.
///
/// This is a branch, not a rename, and the difference is the whole point: a fixture whose text
/// rendering has more than one defensible answer needs both answers on disk at once. An earlier
/// version moved the ranges to the target and dropped the source, which meant a fixture could only
/// ever hold one painting however many times you saved - the exact thing named paintings exist to
/// avoid.
///
/// `copy` decides what a *new* name starts from: the current painting's ranges (the usual case -
/// two answers to the same edit normally share most of their spans, so starting from a copy and
/// diverging is far less work than repainting), or nothing.
///
/// Choosing a name that already exists never writes: it just switches to it, exactly as `L` would.
/// Merging would leave overlapping duplicates and replacing would silently discard a painting
/// somebody made, and neither is recoverable here - there is no undo.
fn action_save_solution_as(app: &mut App, target: &str, copy: bool) {
    let target = target.trim();
    if target.is_empty() {
        app.status = Some("A solution needs a name".to_string());
        return;
    }
    if target == app.text_solution {
        app.status = Some(format!("Already painting under '{target}'"));
        return;
    }
    if app
        .mapping
        .text_mappings
        .iter()
        .any(|named| named.name == target)
    {
        action_load_solution(app, target);
        app.status = Some(format!(
            "'{target}' already exists - switched to it, nothing was overwritten"
        ));
        return;
    }

    let entries = if copy {
        solution_entries(&app.mapping, &app.text_solution).to_vec()
    } else {
        Vec::new()
    };
    let count = entries.len();
    app.mapping.text_mappings.push(NamedTextMapping {
        name: target.to_string(),
        mapping: HumanTextMapping { entries },
    });
    app.text_solution = target.to_string();
    app.dirty = true;
    app.status = Some(if copy {
        format!(
            "Started '{target}' as a copy of the previous painting ({count} range(s)) - {} now on file",
            app.mapping.text_mappings.len()
        )
    } else {
        format!(
            "Started '{target}' empty - {} painting(s) now on file",
            app.mapping.text_mappings.len()
        )
    });
}

/// Deletes the painting named `target`, and picks something sensible to edit next.
///
/// The way out of a fixture painted `Only one solution` that turns out to need `Minimal` and
/// `Full` after all: the single painting has to go, or it sits there claiming the rendering is
/// unambiguous while two paintings next to it say it is not.
///
/// Deleting the last painting leaves the fixture *unpainted* - `text_mappings` empty, which is a
/// different state from a painting with no ranges in it (see `HumanMapping::text_mappings`). That
/// is the honest outcome and the status line says so, because it is also what the `X` open-picker
/// filter and `diffs.csv` will report from then on.
fn action_delete_solution(app: &mut App, target: &str) {
    let before = app.mapping.text_mappings.len();
    app.mapping
        .text_mappings
        .retain(|named| named.name != target);
    if app.mapping.text_mappings.len() == before {
        app.status = Some(format!("No painting called '{target}'"));
        return;
    }
    app.dirty = true;

    // Only the painting being edited forces a move. Deleting some *other* one must leave the
    // reader where they were - being yanked into a different painting because a third was thrown
    // away is how you paint ranges into the wrong one without noticing.
    let was_editing = app.text_solution == target;
    if was_editing {
        app.text_solution = starting_solution(&app.mapping);
    }
    app.status = Some(if app.mapping.text_mappings.is_empty() {
        format!("Deleted '{target}' - this fixture is now unpainted (save with s)")
    } else if was_editing {
        format!(
            "Deleted '{target}' - now editing '{}' ({} painting(s) left)",
            app.text_solution,
            app.mapping.text_mappings.len()
        )
    } else {
        format!(
            "Deleted '{target}' - still editing '{}' ({} painting(s) left)",
            app.text_solution,
            app.mapping.text_mappings.len()
        )
    });
}

/// Switches which painting the text view is editing, without touching any of them.
fn action_load_solution(app: &mut App, target: &str) {
    app.text_solution = target.to_string();
    let count = solution_entries(&app.mapping, target).len();
    app.status = Some(format!("Editing '{target}' ({count} range(s))"));
}

/// What the `t` view is painting on screen. Cycled by `p`, the same key that runs codediff's diff
/// in the tree view.
///
/// The point of the third mode is that neither of the first two answers the question a person
/// painting ground truth actually has, which is "where do we differ". Flipping between two full
/// renderings and spotting the difference by eye is exactly the job `text_mapping_disagreements`
/// already does per byte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TextOverlay {
    /// The human's own painting for the current solution.
    #[default]
    Human,
    /// What codediff's own diff renders, through the same `TextDiff` projection the TUI and the
    /// mapping site use - not a re-derivation, the real thing.
    CodeDiff,
    /// Only the bytes where the two disagree, painted with the *human's* label so the colour still
    /// says what the human claimed. Empty means they agree everywhere.
    Disagreements,
}

impl TextOverlay {
    fn next(self) -> Self {
        match self {
            TextOverlay::Human => TextOverlay::CodeDiff,
            TextOverlay::CodeDiff => TextOverlay::Disagreements,
            TextOverlay::Disagreements => TextOverlay::Human,
        }
    }

    fn label(self) -> &'static str {
        match self {
            TextOverlay::Human => "human",
            TextOverlay::CodeDiff => "codediff",
            TextOverlay::Disagreements => "disagreements",
        }
    }
}

/// codediff's own text ranges for this case, as painting spans - one list per side.
///
/// Goes through `TextDiff::from`, the same projection the real TUI renders and the mapping site
/// draws, so what shows here is what codediff actually produces rather than a second
/// interpretation of its node mapping that could drift from it.
fn codediff_text_spans(before: &Code, after: &Code) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let diff = diff_code(before, after);
    let Some(ast_diff) = diff.ast.as_ref() else {
        return [Vec::new(), Vec::new()];
    };
    let node_cache = NodeCache::build(before, after);
    let text_diff = TextDiff::from(before, after, ast_diff, &node_cache);

    let convert = |ranges: Vec<codediff::diff::text::RangeMatch>| {
        ranges
            .into_iter()
            .filter(|range_match| !range_match.source.is_empty())
            .filter_map(|range_match| {
                let verdict = match range_match.operation {
                    codediff::diff::text::TextOperation::Move => HumanTextVerdict::Move,
                    codediff::diff::text::TextOperation::Update => HumanTextVerdict::Update,
                    codediff::diff::text::TextOperation::Delete => HumanTextVerdict::Delete,
                    codediff::diff::text::TextOperation::Insert => HumanTextVerdict::Insert,
                    // Identical text is the unpainted background on both sides of this comparison.
                    _ => return None,
                };
                Some((
                    HumanTextSpan {
                        start_row: range_match.source.start_row,
                        start_column: range_match.source.start_column,
                        end_row: range_match.source.end_row,
                        end_column: range_match.source.end_column,
                    },
                    verdict,
                ))
            })
            .collect()
    };
    [convert(text_diff.all(0)), convert(text_diff.all(1))]
}

/// The spans where the human's painting and codediff's rendering disagree, labelled with the
/// human's verdict.
///
/// Computed here rather than through `text_mapping_disagreements` because that one compares the
/// painting against the *tree* mapping, which is a different question: this view is about the
/// human's text against codediff's text, with no node mapping in between.
fn overlay_disagreement_spans(
    painted: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    algo: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    before_src: &str,
    after_src: &str,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for (side, source) in [(0usize, before_src), (1usize, after_src)] {
        let lines: Vec<&str> = source.split('\n').collect();
        for (row, line) in lines.iter().enumerate() {
            // One pass per row, coalescing adjacent disagreeing columns into a single span so a
            // whole differing line shows as one range rather than eighty.
            let mut run_start: Option<usize> = None;
            let mut run_verdict = HumanTextVerdict::Update;
            let push = |start: usize, end: usize, verdict, out: &mut Vec<_>| {
                out.push((
                    HumanTextSpan {
                        start_row: row,
                        start_column: start,
                        end_row: row,
                        end_column: end,
                    },
                    verdict,
                ));
            };
            for (column, _) in line.char_indices() {
                let human = verdict_at(&painted[side], row, column, line.len());
                let theirs = verdict_at(&algo[side], row, column, line.len());
                if human == theirs {
                    if let Some(start) = run_start.take() {
                        push(start, column, run_verdict, &mut out[side]);
                    }
                    continue;
                }
                // Colour by whichever side has an opinion, preferring the human's - the reader is
                // checking their own work, so "what I said" is the more useful signal.
                let verdict = human.or(theirs).unwrap_or(HumanTextVerdict::Update);
                match run_start {
                    Some(_) if run_verdict == verdict => {}
                    Some(start) => {
                        push(start, column, run_verdict, &mut out[side]);
                        run_start = Some(column);
                        run_verdict = verdict;
                    }
                    None => {
                        run_start = Some(column);
                        run_verdict = verdict;
                    }
                }
            }
            if let Some(start) = run_start {
                push(start, line.len(), run_verdict, &mut out[side]);
            }
        }
    }
    out
}

/// The verdict covering `(row, column)`, or `None` where nothing does.
fn verdict_at(
    spans: &[(HumanTextSpan, HumanTextVerdict)],
    row: usize,
    column: usize,
    row_len: usize,
) -> Option<HumanTextVerdict> {
    spans
        .iter()
        .find(|(span, _)| span_covers(*span, row, column, row_len))
        .map(|(_, verdict)| *verdict)
}

/// Cursor, selection and scroll for the `t` text-painting view, one set per side.
///
/// Both sides carry a live cursor and an independent selection at all times, mirroring the AST
/// panels: that is what lets `m` pair the two current selections in one keystroke instead of
/// needing a pending-selection handshake, exactly as the tree's own `m` pairs the two panel
/// cursors.
///
/// Columns are **byte** offsets into a row, matching `HumanTextSpan` and `TextRange`, so a painted
/// span needs no conversion on the way out. Cursor movement steps by *characters* even so - see
/// [`TextPaintState::step_column`] - since landing mid-character would produce a span that
/// `span_text` correctly refuses to read back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TextPaintState {
    /// 0 = before, 1 = after. Same side convention as `TextDiff::all`.
    side: usize,
    /// `(row, byte column)` per side.
    cursor: [(usize, usize); 2],
    /// Where a selection was started with `v`, per side; `None` when nothing is selected.
    anchor: [Option<(usize, usize)>; 2],
    /// The digits typed so far at the `:` line prompt, if it is open.
    ///
    /// Kept on the paint state rather than raised as its own modal: the text view *is* a modal,
    /// and nesting one inside it to read a number would mean carrying the whole painting state
    /// through the nested modal and back. A file worth jumping around in is exactly one too big to
    /// reach with `j`, which is the case this exists for.
    line_prompt: Option<String>,
    /// Ranges banked with `x`, per side, waiting to be committed by `d`/`i`/`m`.
    ///
    /// This is what makes an N:M match possible: one live selection can only ever describe one
    /// range, so several occurrences on a side have to accumulate somewhere first. Named and keyed
    /// to match the tree panels' own multi-map selection (`x` to bank, `c` to clear), since it is
    /// the same idea one granularity down.
    pending: [Vec<HumanTextSpan>; 2],
    /// Top visible row per side. Independent, unlike the read-only view this replaced: painting a
    /// move means looking at two places that are nowhere near each other.
    scroll: [usize; 2],
}

impl TextPaintState {
    /// The row's text, or `""` past the end of the file.
    fn row_text(source: &str, row: usize) -> &str {
        source.split('\n').nth(row).unwrap_or("")
    }

    fn row_count(source: &str) -> usize {
        source.split('\n').count()
    }

    /// Moves the cursor `delta` rows, clamping the column to the new row and to a character
    /// boundary.
    fn step_row(&mut self, delta: isize, source: &str) {
        let (row, column) = self.cursor[self.side];
        let last = Self::row_count(source).saturating_sub(1);
        let row = if delta < 0 {
            row.saturating_sub(delta.unsigned_abs())
        } else {
            (row + delta as usize).min(last)
        };
        let line = Self::row_text(source, row);
        let column = column.min(line.len());
        // Clamping can land mid-character on a row whose earlier bytes are multi-byte; walk back
        // to the nearest boundary rather than storing a column no span can be read from.
        let column = (0..=column)
            .rev()
            .find(|&c| line.is_char_boundary(c))
            .unwrap_or(0);
        self.cursor[self.side] = (row, column);
    }

    /// Moves the cursor one *character* left or right, wrapping across rows at the ends.
    fn step_column(&mut self, forward: bool, source: &str) {
        let (row, column) = self.cursor[self.side];
        let line = Self::row_text(source, row);
        if forward {
            match line[column..].chars().next() {
                Some(ch) => self.cursor[self.side] = (row, column + ch.len_utf8()),
                None if row + 1 < Self::row_count(source) => self.cursor[self.side] = (row + 1, 0),
                None => {}
            }
        } else if column > 0 {
            let previous = line[..column]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.cursor[self.side] = (row, previous);
        } else if row > 0 {
            let previous_row = row - 1;
            self.cursor[self.side] = (previous_row, Self::row_text(source, previous_row).len());
        }
    }

    /// The selection on `side` as a span, or `None` if nothing is selected there.
    ///
    /// The end is exclusive and includes the character *under* the cursor, which is what a reader
    /// painting a range sees highlighted - a selection that stopped one character short of its own
    /// cursor would be a surprise every time.
    fn selection(&self, side: usize, source: &str) -> Option<HumanTextSpan> {
        let anchor = self.anchor[side]?;
        let cursor = self.cursor[side];
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        let end_line = Self::row_text(source, end.0);
        let end_column = match end_line[end.1.min(end_line.len())..].chars().next() {
            Some(ch) => end.1 + ch.len_utf8(),
            // The cursor sits past the last character of its row: extend through the newline, so
            // selecting to end-of-line and pressing `d` deletes the line rather than all but its
            // break.
            None => end.1,
        };
        let span = HumanTextSpan {
            start_row: start.0,
            start_column: start.1,
            end_row: end.0,
            end_column,
        };
        (!span.is_empty()).then_some(span)
    }

    /// Every range `side` would commit right now: whatever `x` has banked, plus the live selection
    /// if there is one.
    ///
    /// Banked-plus-live rather than either alone, so `v`-select-`x`-select-`m` works without a
    /// final `x` - forgetting to bank the last range before committing would otherwise silently
    /// drop it, which is exactly the kind of loss this view has no undo for.
    fn committable(&self, side: usize, source: &str) -> Vec<HumanTextSpan> {
        let mut spans = self.pending[side].clone();
        if let Some(live) = self.selection(side, source) {
            spans.push(live);
        }
        spans
    }

    /// Keeps the cursor's row inside a `height`-row viewport.
    fn scroll_into_view(&mut self, height: usize) {
        let row = self.cursor[self.side].0;
        let top = &mut self.scroll[self.side];
        if row < *top {
            *top = row;
        } else if height > 0 && row >= *top + height {
            *top = row + 1 - height;
        }
    }
}

/// `m`: pairs everything selected on the before side with everything selected on the after side,
/// as one `Match`.
///
/// Needs ranges on *both* sides, deliberately: that mirrors the tree's own `m`, which pairs the
/// two panel cursors in one keystroke rather than making the human hold a pending selection in
/// their head across a panel switch.
///
/// N:M falls out of the same key. Bank extra ranges with `x` and they all go into one entry - three
/// occurrences of a token before against two after is a single correspondence, not five. What that
/// entry is worth is then checked immediately rather than at save time: `verdict` refuses a group
/// whose spans disagree within a side, and refusing at the keystroke is the only point where the
/// human still has the selection in front of them to fix.
fn action_paint_match(
    app: &mut App,
    state: &mut TextPaintState,
    before_src: &str,
    after_src: &str,
) {
    let before = state.committable(0, before_src);
    let after = state.committable(1, after_src);
    if before.is_empty() || after.is_empty() {
        app.status =
            Some("Match needs a selection on both sides - press v on each, then m".to_string());
        return;
    }

    let entry = HumanTextEntry {
        operation: HumanTextOperation::Match,
        before,
        after,
    };
    // Resolved now, not at save time: the human is told what they just asserted, and a group whose
    // spans don't hold up is rejected while the selection is still on screen.
    let verdict = match entry.verdict(before_src, after_src) {
        Ok(verdict) => verdict,
        Err(err) => {
            app.status = Some(format!("Not matched: {err:#}"));
            return;
        }
    };

    let shape = format!("{}:{}", entry.before.len(), entry.after.len());
    let solution = app.text_solution.clone();
    solution_entries_mut(&mut app.mapping, &solution).push(entry);
    app.dirty = true;
    state.anchor = [None; 2];
    state.pending = [Vec::new(), Vec::new()];
    app.status = Some(match verdict {
        HumanTextVerdict::Move => {
            format!("Matched {shape}: identical text, recorded as a move")
        }
        HumanTextVerdict::Update => {
            format!("Matched {shape}: text differs, recorded as an update")
        }
        other => format!("Matched {shape} ({other:?})"),
    });
}

/// `d` / `i`: paints everything selected on the focused side as a one-sided removal or addition.
///
/// Takes banked ranges too, so the same token removed in several places is one decision rather
/// than one per occurrence - but unlike a match, these carry no identity constraint: with nothing
/// to pair against, spans that read differently assert nothing unsound.
fn action_paint_one_sided(
    app: &mut App,
    state: &mut TextPaintState,
    operation: HumanTextOperation,
    before_src: &str,
    after_src: &str,
) {
    let side = match operation {
        HumanTextOperation::Delete => 0,
        HumanTextOperation::Insert => 1,
        HumanTextOperation::Match => return,
    };
    let source = if side == 0 { before_src } else { after_src };
    let spans = state.committable(side, source);
    if spans.is_empty() {
        let (key, what, panel) = match operation {
            HumanTextOperation::Delete => ("d", "delete", "Before"),
            _ => ("i", "insert", "After"),
        };
        app.status = Some(format!(
            "Nothing selected on the {panel} side - press v there, move, then {key} to {what}"
        ));
        return;
    }

    let count = spans.len();
    let entry = if side == 0 {
        HumanTextEntry {
            operation,
            before: spans,
            after: Vec::new(),
        }
    } else {
        HumanTextEntry {
            operation,
            before: Vec::new(),
            after: spans,
        }
    };
    if let Err(err) = entry.verdict(before_src, after_src) {
        app.status = Some(format!("Not painted: {err:#}"));
        return;
    }

    let solution = app.text_solution.clone();
    solution_entries_mut(&mut app.mapping, &solution).push(entry);
    app.dirty = true;
    state.anchor[side] = None;
    state.pending[side].clear();
    app.status = Some(match operation {
        HumanTextOperation::Delete => format!("Painted {count} deletion(s)"),
        _ => format!("Painted {count} insertion(s)"),
    });
}

/// `u`: removes whichever painted entry covers the focused cursor.
///
/// Removes the *whole entry*, both sides of a `Match` included. A half-removed match would be a
/// malformed entry, which `HumanTextEntry::verdict` rightly refuses to read - so the alternative
/// to removing both is not a smaller edit, it is a broken file.
fn action_paint_unmark(app: &mut App, state: &TextPaintState, before_src: &str, after_src: &str) {
    let side = state.side;
    let source = if side == 0 { before_src } else { after_src };
    let (row, column) = state.cursor[side];
    let row_len = TextPaintState::row_text(source, row).len();

    let solution = app.text_solution.clone();
    let entries = solution_entries_mut(&mut app.mapping, &solution);
    let before_count = entries.len();
    entries.retain(|entry| {
        let spans = if side == 0 {
            &entry.before
        } else {
            &entry.after
        };
        !spans
            .iter()
            .any(|span| span_covers(*span, row, column, row_len))
    });
    let removed = before_count - entries.len();
    if removed == 0 {
        app.status = Some("Nothing painted here".to_string());
        return;
    }
    app.dirty = true;
    app.status = Some(format!("Removed {removed} painted range(s)"));
}

/// `Z`: marks this fixture's painting as complete even though nothing was painted.
///
/// Only reachable when there is genuinely nothing to paint - two identical files. Without it that
/// case is indistinguishable from an unvisited fixture, since both would leave `text_mapping` at
/// `None`, and a completeness count would quietly under-report forever.
fn action_paint_mark_empty(app: &mut App) {
    let solution = app.text_solution.clone();
    if !solution_entries(&app.mapping, &solution).is_empty() {
        app.status = Some(format!(
            "'{solution}' already has painted ranges - u removes them one at a time"
        ));
        return;
    }
    solution_entries_mut(&mut app.mapping, &solution);
    app.dirty = true;
    app.status = Some(format!("Marked '{solution}' as painted with no changes"));
}

/// A blocking prompt raised by `m`/`M` that needs a direct human answer before the mapping entry
/// can be finalized. While `App::modal` is `Some`, the event loop routes keys to
/// `handle_modal_key` instead of the normal keybindings.
#[derive(Debug, Clone)]
enum Modal {
    /// The before and after cursor nodes have different kinds. Shown before adding any mapping
    /// for them, since codediff itself never maps nodes of different kinds together (see
    /// `ASTDiff::is_valid`), so confirming this will always show up as a mismatch against
    /// codediff's actual diff -- which is fine for exploration, but worth confirming explicitly.
    ConfirmKindMismatch {
        before_id: usize,
        after_id: usize,
        before_kind: String,
        after_kind: String,
        /// Whether this originated from `M` (recursive), in which case confirming also
        /// auto-matches the rest of the subtree.
        recursive: bool,
    },
    /// Raised by `m`/`M` when a multi-map selection (see `App::before_multi_select`/
    /// `after_multi_select`, toggled by `x`) is non-empty but its members don't all share one AST
    /// node kind. Same "confirm before doing something codediff's own diff would never produce"
    /// posture as `ConfirmKindMismatch`, generalized to a set of ids instead of a single pair.
    ConfirmMultiMapGroup {
        before_ids: Vec<usize>,
        after_ids: Vec<usize>,
        operation: HumanOperation,
        with_children: bool,
        kinds: Vec<String>,
    },
    /// Raised by `o`: pick a test case (a directory under src/test/data/diffs/) to open. Each
    /// option is paired with which of `DIFF_DATASETS` it lives under; `d` cycles `dataset_filter`
    /// (all -> handmade -> small -> full -> all) to narrow the list down to one at a time, and `H`
    /// toggles `hide_complete` (narrowing to cases with at least one unmarked node left - see
    /// `diff_case_is_incomplete`/`App::diff_completeness`). Like `OpenSamplePicker`, `selected`
    /// indexes into the filtered view (`visible_diff_options`), not `options` itself.
    OpenDiffPicker {
        options: Vec<(String, &'static str)>,
        selected: usize,
        dataset_filter: Option<&'static str>,
        hide_complete: bool,
        /// The `X` filter: narrow to cases with no painted text mapping yet (see
        /// `App::diff_hide_painted`).
        hide_painted: bool,
    },
    /// Raised by `O`: pick a sampled candidate (a directory under src/test/data/samples/) to
    /// open. Each option is paired with its `SampleTriageStatus` (per the matching sample.csv
    /// row's `status` column) -- a promotion is shown as " - SOLVED" and a rejection as
    /// " - REJECTED", and both are left out of the list entirely when `hide_solved` is set --
    /// and with its `sample_diff_line_count` (computed once when the picker opens, not on every
    /// `s` press). `selected` indexes into `visible_sample_options(&options, hide_solved,
    /// sort_order)`, not `options` itself.
    OpenSamplePicker {
        options: Vec<(String, SampleTriageStatus, usize)>,
        selected: usize,
        hide_solved: bool,
        sort_order: SampleSortOrder,
    },
    /// Raised when a picker's selection is confirmed while the current mapping has unsaved
    /// changes: asks whether to save the *current* case before switching to `target`.
    /// `can_save` is false when the current case is a sample, since promoting one needs a name
    /// (see `PromptPromoteName`) rather than being a single-key save.
    ConfirmDiscardUnsaved { target: OpenTarget, can_save: bool },
    /// Raised by `s` when the current case is a sample: asks for the name to promote it under in
    /// `src/test/data/diffs/`. Re-raised with `error` set (input preserved) if the name is
    /// invalid or already in use.
    PromptPromoteName {
        input: String,
        error: Option<String>,
    },
    /// Raised by `R` when the current case is a sample: asks for a reason to reject it instead of
    /// promoting it. Recorded as-is in sample.csv's `comment` column, with `status` set to
    /// `REJECTED` and `promoted_to` left untouched (empty). Re-raised with `error` set (input
    /// preserved) if the reason is empty or the sample.csv row can't be found - same posture as
    /// `PromptPromoteName`.
    PromptRejectReason {
        input: String,
        error: Option<String>,
    },
    /// Raised by `e` when the current case is a sample: enters or edits a free-form comment on it,
    /// pre-filled with whatever's already recorded (if anything). Recorded as-is in sample.csv's
    /// `comment` column - unlike `PromptRejectReason`, doesn't touch `status`, and an empty
    /// submission is valid (clears the comment). If a comment is present when the sample is later
    /// promoted (`s`), it's also written as a leading doc comment in the generated
    /// optimal_solutions test stub - see `action_promote`/`ensure_stub_test`. Re-raised with
    /// `error` set (input preserved) if the sample.csv row can't be found - same posture as
    /// `PromptPromoteName`.
    PromptComment {
        input: String,
        error: Option<String>,
    },
    /// Raised by `/`: asks for text to search for. Pre-filled with `App::last_search`, if any.
    /// `Enter` runs the search (`action_search`) and closes the modal either way (found or not) -
    /// unlike `PromptPromoteName`, a failed search isn't invalid input to correct, just "nothing
    /// found from here", reported on the status line instead of re-prompting.
    PromptSearch { input: String },
    /// Raised by `t`: both sides' source side by side, for reading the actual code instead of
    /// navigating the AST tree - and for *painting* the human's text-range ground truth onto it
    /// (see `HumanTextMapping`), which is a second, independent account of the same diff that the
    /// tree mapping cannot supply. `T` while open switches to `UnixDiffView` instead.
    TextView { state: TextPaintState },
    /// Raised by `s` (saving) or `L` (loading) inside the text view: which named painting
    /// (`HumanMapping::text_mappings`) to store the current ranges under, or to switch to editing.
    ///
    /// `names` is `solution_picker_names`' output - this fixture's existing paintings first, then
    /// whichever of `SUGGESTED_SOLUTION_NAMES` it doesn't have - and the list always renders one
    /// extra row past its end for a free-form name, which `new_name` fills in once typing starts.
    /// `state` is carried so closing this picker returns to the text view exactly where it was.
    SolutionPicker {
        names: Vec<String>,
        selected: usize,
        /// `true` for `s` (save the current ranges under the chosen name), `false` for `L` (just
        /// switch which painting is being edited). The two differ only in what `Enter` does.
        saving: bool,
        new_name: Option<String>,
        /// The painting `D` has been pressed once on, awaiting a second `D` to actually delete it.
        ///
        /// Two keystrokes rather than one, and the name carried through rather than an index:
        /// deleting a painting throws away work that may have taken an hour and there is no undo
        /// here, so the confirmation has to be about the *painting the reader saw named on screen*
        /// - not about whichever row the cursor happens to be on by the time the second key lands.
        confirm_delete: Option<String>,
        state: TextPaintState,
    },
    /// Raised by `T`: shows the output of running the system `diff -u` between the before and
    /// after content -- a plain line-based diff, as a point of comparison against codediff's own
    /// AST-based diff (`p`). `t` while open switches to `TextView` instead.
    UnixDiffView { output: String, scroll: u16 },
    /// Raised by `?`: lists every keybinding. `?` or `Esc` while open closes it.
    Help { scroll: u16 },
    /// Raised by `C`: pick a commit from this repository's own `git log` (see `list_repo_commits`)
    /// to open. `j`/`k` move, `Enter` lists the files it changed (`OpenCommitFilePicker`), `Esc`
    /// cancels. `(hash, summary)` pairs, newest first, same order `list_repo_commits` returns.
    OpenCommitPicker {
        commits: Vec<(String, String)>,
        selected: usize,
    },
    /// Raised when a commit is chosen in `OpenCommitPicker`: pick which of the files it changed to
    /// open. `hash`/`summary` are carried along from that commit, just for display and to build
    /// the `OpenTarget` on `Enter`. Unlike the picker it was raised from, `Esc` here cancels
    /// entirely rather than returning to `OpenCommitPicker` - consistent with every other modal in
    /// this file, none of which have a "back" step either.
    OpenCommitFilePicker {
        hash: String,
        summary: String,
        files: Vec<String>,
        selected: usize,
    },
}

/// Which open picker (`o`, `O`, or `C`) a pending switch came from, and enough to load it.
#[derive(Debug, Clone)]
enum OpenTarget {
    Diffs(String),
    Sample(String),
    /// From `C`'s file picker: `path` as changed by commit `hash` (`summary` is only carried
    /// along for the status message once it's opened - see `run_event_loop`).
    GitCommitFile {
        hash: String,
        summary: String,
        path: String,
    },
}

impl OpenTarget {
    fn name(&self) -> &str {
        match self {
            OpenTarget::Diffs(name) | OpenTarget::Sample(name) => name,
            OpenTarget::GitCommitFile { path, .. } => path,
        }
    }
}

/// Where the currently open case's content lives: a committed test case, a not-yet-promoted
/// sample, or a file read straight out of this repository's own git history (`C`). Determines what
/// `s` does (see `Modal::PromptPromoteName`) and what `o`/`O`/`C` need to know before switching
/// away with unsaved changes.
#[derive(Debug, Clone)]
enum CaseOrigin {
    Diffs,
    Sample(SampleSource),
    /// `path` as it stood in whichever commit `C` opened it from - the commit's own hash/summary
    /// aren't kept here since nothing after load needs them again: `App::name` already carries a
    /// short hash (set once, in `run_event_loop`) for display, and promoting writes straight from
    /// `before_src`/`after_src` (the content already on screen), not by re-reading git.
    GitCommitFile {
        path: String,
    },
}

struct App {
    /// Name of the currently open case: a directory under src/test/data/diffs/ (if `origin` is
    /// `Diffs`), src/test/data/samples/ (if `origin` is `Sample`), or a `<path>@<short hash>`
    /// display label with no directory of its own (if `origin` is `GitCommitFile`). Can change at
    /// runtime via the `o`/`O`/`C` (open) pickers, or via promoting a sample or git-commit-sourced
    /// case with `s`.
    name: String,
    origin: CaseOrigin,
    focus: Focus,
    before: PanelState,
    after: PanelState,
    mapping: HumanMapping,
    dirty: bool,
    status: Option<String>,
    modal: Option<Modal>,
    should_quit: bool,
    /// codediff's own diff, computed on demand by `p` and rendered in parentheses next to each
    /// node's human-marked status glyph for a quick visual diff against the human mapping. `None`
    /// until `p` has been pressed at least once for the current case.
    algo_diff: Option<ASTDiff>,
    /// Toggled by `H`: when true, a subtree is left out of both panels' flattened view entirely
    /// once every node in it (the root and all descendants) has `NodeStatus` other than
    /// `Unmarked` -- i.e. nothing left in it to review. Recomputed fresh each frame from the
    /// current mapping, so it can't drift out of sync with what's actually marked.
    hide_solved: bool,
    /// Toggled by `r`: when true, each node's algo-verdict glyph (see `algo_diff`) is followed by
    /// the short label of the `ASTMappingReason` codediff recorded for it (e.g. "IdHash", "APTED")
    /// -- which pass is responsible for that mapping, not just what the mapping is. Has no effect
    /// until `algo_diff` is populated (`p`).
    show_reason: bool,
    /// The `O` picker's own hide-solved toggle (distinct from `hide_solved` above, which hides
    /// solved *subtrees* in the AST panels, not solved *samples* in this list) and sort order,
    /// persisted here rather than reset every time a fresh `Modal::OpenSamplePicker` is built --
    /// so picking "smallest diff first" and hiding already-promoted samples once, then closing
    /// the picker to work through a few, sticks for the next `O` instead of reverting to A-Z/show
    /// all every time.
    sample_hide_solved: bool,
    sample_sort_order: SampleSortOrder,
    /// The `o` picker's dataset filter (cycled by `d` - see `DIFF_DATASETS`), persisted here for
    /// the same reason as `sample_hide_solved`/`sample_sort_order` above: so filtering down to
    /// e.g. just `handmade` sticks across closing and reopening the picker. `None` shows every
    /// dataset.
    diff_dataset_filter: Option<&'static str>,
    /// The `o` picker's "incomplete only" filter (toggled by `H` inside it), persisted here for
    /// the same reason as `diff_dataset_filter` above.
    diff_hide_complete: bool,
    /// The `o` picker's "unpainted only" filter (toggled by `X` inside it), persisted for the same
    /// reason. Independent of `diff_hide_complete`: a fixture's tree mapping and its text-range
    /// painting are two separate ground truths (see `HumanTextMapping`), so "still needs nodes
    /// marked" and "still needs text painted" are different queues and combine as an AND when both
    /// are on.
    diff_hide_painted: bool,
    /// Cache of, for every case `list_available_cases` lists, whether it already has a painted
    /// text mapping (see `diff_case_has_text_mapping`). `None` until the first `X` press, the same
    /// lazy-once-per-session contract `diff_completeness` has - though this scan is much cheaper,
    /// since it skims JSON rather than parsing source with tree-sitter.
    diff_text_painted: Option<std::collections::HashMap<String, bool>>,
    /// Every case's `description.md`, for the `o` picker's note marker and footer. `None` until
    /// the first `o` press, the same lazy-once-per-session contract its two neighbours have - but
    /// unlike them this is loaded on `o` itself rather than on the key that filters by it, since
    /// it is displayed rather than filtered on and has to be there the first time the list is
    /// drawn. Cases with no note are absent from the map, not present-and-empty.
    diff_comments: Option<std::collections::HashMap<String, String>>,
    /// What the `t` view is painting on screen (see `TextOverlay`) - the human's own ranges,
    /// codediff's, or only where they differ. Cycled by `p` inside that view.
    text_overlay: TextOverlay,
    /// codediff's own text ranges for the open case, per side, computed on first use and dropped
    /// when the case changes. `None` until `p` has cycled past `Human` at least once - running the
    /// diff costs real time on a large fixture, and the default view never needs it.
    algo_text_spans: Option<[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    /// Which named text painting (`HumanMapping::text_mappings`) the `t` view is editing. Starts
    /// at the fixture's first existing painting, or `SUGGESTED_SOLUTION_NAMES[0]` when it has
    /// none, and is changed by `s` (save-as) / `L` (load) inside that view.
    text_solution: String,
    /// Cache of, for every case `list_available_cases` lists, whether it has at least one
    /// `NodeStatus::Unmarked` node left (see `diff_case_is_incomplete`) - `None` until the first
    /// time `H` is pressed inside the `o` picker, since scanning the whole corpus (parsing every
    /// case's before/after code, not just listing directory names) takes real wall-clock time -
    /// roughly 10s across this repo's own ~230 fixtures as of this writing. Kept for the rest of
    /// the session once built; refreshed for just the current case's own entry after `s` saves it,
    /// rather than dropped and rebuilt from scratch, so repeated saves while triaging incomplete
    /// cases don't each cost a fresh full scan.
    diff_completeness: Option<std::collections::HashMap<String, bool>>,
    /// The last text searched for with `/` (`Modal::PromptSearch`), if any - pre-fills the prompt
    /// next time, so `/` then `Enter` repeats the same search from wherever the cursor landed,
    /// without retyping it.
    last_search: Option<String>,
    /// Node ids pending inclusion in a multi-map group, toggled by `x` (and cleared by `c` or by
    /// `m`/`M` committing them). Plain node ids rather than borrowed `Node`s, the same convention
    /// `PanelState::cursor_id` already uses, since `App` outlives any one parse of `before`/
    /// `after` (a case switch reparses both trees under the same `App`). Cleared on every case
    /// switch (see `run_event_loop`'s three `SessionEnd::Open` arms) since an id from the old
    /// trees could otherwise collide with an unrelated node in the new ones.
    before_multi_select: std::collections::BTreeSet<usize>,
    after_multi_select: std::collections::BTreeSet<usize>,
}

impl App {
    fn new(
        name: String,
        origin: CaseOrigin,
        before_root_id: usize,
        after_root_id: usize,
        mapping: HumanMapping,
    ) -> Self {
        // Start on whichever painting this fixture already has, so reopening a case resumes where
        // it was left rather than silently starting a second, near-duplicate solution.
        let text_solution = starting_solution(&mapping);
        Self {
            name,
            origin,
            focus: Focus::Before,
            before: PanelState::new(before_root_id),
            after: PanelState::new(after_root_id),
            mapping,
            dirty: false,
            status: Some(
                "Loaded. m match, d/D delete, i/I insert, u unmark, s save, q quit, o open."
                    .to_string(),
            ),
            modal: None,
            should_quit: false,
            algo_diff: None,
            hide_solved: false,
            show_reason: false,
            sample_hide_solved: false,
            sample_sort_order: SampleSortOrder::Alphabetical,
            diff_dataset_filter: None,
            diff_hide_complete: false,
            diff_hide_painted: false,
            diff_text_painted: None,
            diff_comments: None,
            text_solution,
            text_overlay: TextOverlay::default(),
            algo_text_spans: None,
            diff_completeness: None,
            last_search: None,
            before_multi_select: std::collections::BTreeSet::new(),
            after_multi_select: std::collections::BTreeSet::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tree flattening & node status
// ---------------------------------------------------------------------------------------------

/// Flattens a tree into preorder (node, depth) pairs, skipping the children of collapsed nodes and
/// (if `hidden` is given) any node -- and its whole subtree -- present in `hidden` entirely. A
/// node that's hidden this way doesn't get a row of its own, unlike a collapsed one.
fn flatten_visible<'a>(
    root: Node<'a>,
    collapsed: &std::collections::HashSet<usize>,
    hidden: Option<&std::collections::HashSet<usize>>,
) -> Vec<(Node<'a>, usize)> {
    let mut out = Vec::new();
    walk_visible(root, 0, collapsed, hidden, &mut out);
    out
}

fn walk_visible<'a>(
    node: Node<'a>,
    depth: usize,
    collapsed: &std::collections::HashSet<usize>,
    hidden: Option<&std::collections::HashSet<usize>>,
    out: &mut Vec<(Node<'a>, usize)>,
) {
    if hidden.is_some_and(|hidden| hidden.contains(&node.id())) {
        return;
    }
    out.push((node, depth));
    if collapsed.contains(&node.id()) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_visible(child, depth + 1, collapsed, hidden, out);
    }
}

/// `flatten_visible`'s output, paired with a node id -> index lookup table built alongside it.
/// Resolving a node id back to its position in the flat list -- almost always
/// `PanelState::cursor_id`, to move it or to find where the cursor row is for scrolling/rendering
/// -- used to mean an O(n) linear scan of the flat list (`.position()`/`.find()`), repeated on
/// every cursor move, every mark, and every single redraw. On a 30k-node tree that scan alone
/// showed up as real per-keystroke latency; this makes it O(1) instead.
///
/// Derefs to `[(Node, usize)]`, so any caller that only ever iterated or indexed the flat list
/// positionally (never by node id) needs no changes at all -- it's a drop-in replacement for the
/// bare `Vec<(Node, usize)>` `flatten_visible` used to hand back directly.
struct FlatIndex<'a> {
    nodes: Vec<(Node<'a>, usize)>,
    by_id: rustc_hash::FxHashMap<usize, usize>,
}

impl<'a> FlatIndex<'a> {
    fn new(nodes: Vec<(Node<'a>, usize)>) -> Self {
        let by_id = nodes
            .iter()
            .enumerate()
            .map(|(index, (node, _))| (node.id(), index))
            .collect();
        Self { nodes, by_id }
    }

    /// `id`'s position in the flat list, in O(1).
    fn index_of(&self, id: usize) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    /// The node with id `id`, in O(1). `None` if `id` isn't currently visible (e.g. hidden under
    /// a collapsed ancestor, or under `H`'s hide-solved filter).
    fn node_for_id(&self, id: usize) -> Option<Node<'a>> {
        self.index_of(id).map(|index| self.nodes[index].0)
    }
}

impl<'a> std::ops::Deref for FlatIndex<'a> {
    type Target = [(Node<'a>, usize)];

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

/// Node IDs whose entire subtree -- the node itself and every descendant -- has `NodeStatus`
/// other than `Unmarked`: nothing left in it to review. Used by the `H` (hide solved) toggle to
/// prune those subtrees from the flattened view (via `flatten_visible`'s `hidden` set) while any
/// node that's still `Unmarked` stays visible, along with its full ancestor chain (an ancestor of
/// an `Unmarked` node can never itself be fully solved, so it's never included here).
fn fully_solved_nodes(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> std::collections::HashSet<usize> {
    let mut solved = std::collections::HashSet::new();
    mark_fully_solved(root, caches, status_fn, &mut solved);
    solved
}

/// Post-order: returns whether `node`'s own subtree is fully solved, recording it in `solved` if
/// so. A node counts as solved only if it is itself marked *and* every child is fully solved.
fn mark_fully_solved(
    node: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    solved: &mut std::collections::HashSet<usize>,
) -> bool {
    let mut cursor = node.walk();
    let mut all_children_solved = true;
    for child in node.children(&mut cursor) {
        if !mark_fully_solved(child, caches, status_fn, solved) {
            all_children_solved = false;
        }
    }

    let is_solved = all_children_solved && status_fn(node, caches) != NodeStatus::Unmarked;
    if is_solved {
        solved.insert(node.id());
    }
    is_solved
}

/// codediff's own per-node verdict, computed from an `ASTDiff` (via `p`) the same way `NodeStatus`
/// is computed from the human mapping, but collapsed to a single glyph rather than distinguishing
/// with-children/inherited marks: `before_node_map`/`after_node_map` already carry that down to
/// every descendant node directly, since codediff maps (or zero-maps) every node in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlgoStatus {
    /// Mapped to a node on the other side (whatever the specific `ASTMappingOperation`).
    Matched,
    Deleted,
    Inserted,
    /// No entry for this node at all, e.g. the tree root (see `ASTDiff::is_complete`) or a diff
    /// that hasn't been recomputed since the tree changed underneath it.
    Unknown,
}

fn algo_status_before(node: Node, diff_ast: &ASTDiff) -> AlgoStatus {
    match diff_ast.before_node_map.get(&node.id()) {
        Some(0) => AlgoStatus::Deleted,
        Some(_) => AlgoStatus::Matched,
        None => AlgoStatus::Unknown,
    }
}

fn algo_status_after(node: Node, diff_ast: &ASTDiff) -> AlgoStatus {
    match diff_ast.after_node_map.get(&node.id()) {
        Some(0) => AlgoStatus::Inserted,
        Some(_) => AlgoStatus::Matched,
        None => AlgoStatus::Unknown,
    }
}

fn algo_status_glyph(status: AlgoStatus) -> &'static str {
    match status {
        AlgoStatus::Matched => "M",
        AlgoStatus::Deleted => "-",
        AlgoStatus::Inserted => "+",
        AlgoStatus::Unknown => "?",
    }
}

/// Which pass produced the Before node's mapping entry, if any -- `diff_ast.mapping` has one entry
/// per node (see `apted::common::add_delete_mappings`/`add_insert_mappings`), keyed by
/// `(before_id, after_id)` with `0` standing in for "no partner" on whichever side is missing, so
/// this looks up the entry the same way for a match, a delete, or (in principle) an unresolved
/// node -- `None` only when `before_node_map` itself has no entry at all (`AlgoStatus::Unknown`).
fn algo_reason_before(node: Node, diff_ast: &ASTDiff) -> Option<ASTMappingReason> {
    let after_id = *diff_ast.before_node_map.get(&node.id())?;
    diff_ast
        .mapping
        .get(&(node.id(), after_id))
        .map(|m| m.reason)
}

/// Same as [`algo_reason_before`], but for the After tree.
fn algo_reason_after(node: Node, diff_ast: &ASTDiff) -> Option<ASTMappingReason> {
    let before_id = *diff_ast.after_node_map.get(&node.id())?;
    diff_ast
        .mapping
        .get(&(before_id, node.id()))
        .map(|m| m.reason)
}

/// Short column-style label for an `ASTMappingReason`. Thin wrapper around
/// `ASTMappingReason::bucket_label`, shared with `src/bin/benchmark_optimal_solutions.rs`'s
/// reason-count columns so the same abbreviation means the same thing in both tools. Collapses
/// `APTED`'s provenance payload to a bare "APTED" - see [`reason_detail`] for the version that
/// shows it.
fn reason_label(reason: ASTMappingReason) -> &'static str {
    reason.bucket_label()
}

/// Same short label as [`reason_label`], except for `APTED`, where it also appends the
/// provenance payload (e.g. `"APTED:final_pass"`) - see `ASTMappingReason::APTED`'s doc comment
/// on why that payload exists. Used for the `r`-toggle's per-node display (`render_panel`), where
/// "which pass matched it" is exactly the point; `reason_label` stays the bare bucket label
/// everywhere a stable, provenance-independent abbreviation is needed instead (the reason-count
/// table this tool shares an abbreviation scheme with).
fn reason_detail(reason: ASTMappingReason) -> String {
    match reason {
        ASTMappingReason::APTED(source) => format!("APTED:{source}"),
        other => reason_label(other).to_string(),
    }
}

/// True if codediff's verdict for the Before `node` disagrees with the human's, once the human has
/// actually made a decision about it: not just whether both sides call it "matched", but whether
/// they agree on *what* it's matched to (mirrors the comparison `check_entry` makes for the
/// `optimal_solutions` tests). A node the human hasn't marked yet has nothing to disagree with, so
/// it's never flagged, even if codediff already has an opinion.
fn algo_disagrees_before(node: Node, caches: &Caches, diff_ast: &ASTDiff) -> bool {
    let algo_partner = diff_ast.before_node_map.get(&node.id()).copied();
    if let Some(human_after_id) = caches.before_match.get(&node.id()) {
        return algo_partner != Some(*human_after_id);
    }
    if caches.before_removed.contains_key(&node.id())
        || is_inherited_removed(node, &caches.before_removed)
    {
        return algo_partner != Some(0);
    }
    false
}

/// Same as [`algo_disagrees_before`], but for the After tree.
fn algo_disagrees_after(node: Node, caches: &Caches, diff_ast: &ASTDiff) -> bool {
    let algo_partner = diff_ast.after_node_map.get(&node.id()).copied();
    if let Some(human_before_id) = caches.after_match.get(&node.id()) {
        return algo_partner != Some(*human_before_id);
    }
    if caches.after_removed.contains_key(&node.id())
        || is_inherited_removed(node, &caches.after_removed)
    {
        return algo_partner != Some(0);
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------------------------

fn move_cursor(panel: &mut PanelState, flat: &FlatIndex, delta: i32) {
    if flat.is_empty() {
        return;
    }
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    let new_idx = (idx as i32 + delta).clamp(0, flat.len() as i32 - 1) as usize;
    panel.cursor_id = flat[new_idx].0.id();
}

fn jump_to_edge(panel: &mut PanelState, flat: &[(Node, usize)], to_start: bool) {
    let edge = if to_start { flat.first() } else { flat.last() };
    if let Some((node, _)) = edge {
        panel.cursor_id = node.id();
    }
}

/// Moves `panel`'s cursor forward to the next node (strictly after the current position) with
/// `NodeStatus::Unmarked`, if one exists. Leaves the cursor untouched otherwise.
fn advance_to_next_unmarked(
    panel: &mut PanelState,
    flat: &FlatIndex,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) {
    let Some(idx) = flat.index_of(panel.cursor_id) else {
        return;
    };
    for (node, _) in &flat[idx + 1..] {
        if status_fn(*node, caches) == NodeStatus::Unmarked {
            panel.cursor_id = node.id();
            return;
        }
    }
}

/// After a match finalizes, walks both panels' cursors forward to their own next unmarked node
/// (independently), as a quality-of-life step-through-the-tree convenience. Recomputes caches
/// fresh from `app.mapping`, since the caller's caches predate the change that was just applied.
fn advance_both_to_next_unmarked(
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the Before panel: used after a
/// delete, which only touches the Before side, so only that cursor should step forward.
fn advance_before_to_next_unmarked(
    app: &mut App,
    before_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the After panel: used after an
/// insert, which only touches the After side, so only that cursor should step forward.
fn advance_after_to_next_unmarked(
    app: &mut App,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Moves `panel`'s cursor to the next (`forward`) or previous node where `disagrees_fn` is true,
/// relative to its current position, wrapping around the ends of `flat` like a `vim` `n`/`N`
/// search. Returns the node landed on, or `None` if `disagrees_fn` is false for every node.
fn advance_to_next_mismatch<'a>(
    panel: &mut PanelState,
    flat: &FlatIndex<'a>,
    caches: &Caches,
    diff_ast: &ASTDiff,
    disagrees_fn: fn(Node, &Caches, &ASTDiff) -> bool,
    forward: bool,
) -> Option<Node<'a>> {
    if flat.is_empty() {
        return None;
    }
    let len = flat.len();
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    for step in 1..=len {
        let i = if forward {
            (idx + step) % len
        } else {
            (idx + len - step) % len
        };
        let (node, _) = flat[i];
        if disagrees_fn(node, caches, diff_ast) {
            panel.cursor_id = node.id();
            return Some(node);
        }
    }
    None
}

/// Implements `n`/`N`: moves the focused panel's cursor to the next/previous node where codediff's
/// verdict (`p`) disagrees with the human mapping (the same condition that draws the trailing `*`
/// in `render_panel`). Requires `p` to have been run at least once for the current case.
fn action_next_mismatch(
    app: &mut App,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    caches: &Caches,
    forward: bool,
) -> Result<String> {
    let diff_ast = app
        .algo_diff
        .as_ref()
        .context("No codediff result yet; press 'p' to run it first")?;
    let found = match focus {
        Focus::Before => advance_to_next_mismatch(
            &mut app.before,
            before_flat,
            caches,
            diff_ast,
            algo_disagrees_before,
            forward,
        ),
        Focus::After => advance_to_next_mismatch(
            &mut app.after,
            after_flat,
            caches,
            diff_ast,
            algo_disagrees_after,
            forward,
        ),
    };
    match found {
        Some(node) => Ok(format!("Jumped to mismatch: '{}'", node.kind())),
        None => bail!("No mismatches in this panel"),
    }
}

/// Moves `panel`'s cursor to the next leaf node (in `flat`'s order, i.e. document order) whose own
/// text contains `query`, wrapping around like `n`/`N`'s mismatch search
/// (`advance_to_next_mismatch`, which this otherwise mirrors exactly - kept separate rather than
/// generalized into one shared function, since the two predicates need different captured context,
/// `Caches`+`ASTDiff` vs. `query`+`src`, and a bare `fn` pointer can't capture either).
///
/// Leaf nodes only (`child_count() == 0`), not "any node whose subtree's text contains query": a
/// non-leaf node's own text is the concatenation of all its descendants', so almost every ancestor
/// of a real match would *also* "match" under a naive whole-subtree check - starting from most
/// cursor positions, that means landing on some enclosing container (often the file root) instead
/// of the actual token, which is not what a human doing the AST-browser equivalent of Ctrl-F wants.
fn advance_to_next_search_match<'a>(
    panel: &mut PanelState,
    flat: &FlatIndex<'a>,
    src: &[u8],
    query: &str,
) -> Option<Node<'a>> {
    if flat.is_empty() {
        return None;
    }
    let len = flat.len();
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    for step in 1..=len {
        let i = (idx + step) % len;
        let (node, _) = flat[i];
        if node.child_count() == 0 && node.utf8_text(src).unwrap_or("").contains(query) {
            panel.cursor_id = node.id();
            return Some(node);
        }
    }
    None
}

/// Implements `/`'s search: moves the focused panel's cursor to the next leaf node containing
/// `query`, starting just after its current position and wrapping around. Case-sensitive, plain
/// substring match - no regex, matching what was actually asked for.
fn action_search(
    app: &mut App,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_src: &[u8],
    after_src: &[u8],
    query: &str,
) -> Result<String> {
    let found = match focus {
        Focus::Before => {
            advance_to_next_search_match(&mut app.before, before_flat, before_src, query)
        }
        Focus::After => advance_to_next_search_match(&mut app.after, after_flat, after_src, query),
    };
    match found {
        Some(node) => Ok(format!("Found '{query}' in '{}'", node.kind())),
        None => bail!("No node containing '{query}' found in this panel"),
    }
}

fn expand_or_descend(panel: &mut PanelState, flat: &FlatIndex) {
    let Some(node) = flat.node_for_id(panel.cursor_id) else {
        return;
    };
    if node.child_count() == 0 {
        return;
    }
    if panel.collapsed.remove(&node.id()) {
        return; // was collapsed; expanding is enough, stay put
    }
    let mut cursor = node.walk();
    if let Some(first_child) = node.children(&mut cursor).next() {
        panel.cursor_id = first_child.id();
    }
}

fn collapse_or_ascend(panel: &mut PanelState, flat: &FlatIndex) {
    let Some(node) = flat.node_for_id(panel.cursor_id) else {
        return;
    };
    if node.child_count() > 0 && !panel.collapsed.contains(&node.id()) {
        panel.collapsed.insert(node.id());
        return;
    }
    if let Some(parent) = node.parent() {
        panel.cursor_id = parent.id();
    }
}

fn ensure_visible(scroll: &mut usize, cursor_idx: usize, viewport_height: usize) {
    let viewport_height = viewport_height.max(1);
    if cursor_idx < *scroll {
        *scroll = cursor_idx;
    } else if cursor_idx >= *scroll + viewport_height {
        *scroll = cursor_idx + 1 - viewport_height;
    }
}

/// Finds the node with id `id` anywhere in `root`'s subtree, regardless of collapse state (unlike
/// `flatten_visible`, which only sees expanded nodes). Used by `action_align` to locate a matched
/// node that may currently be hidden under a collapsed ancestor.
fn find_node_by_id_anywhere(root: Node, id: usize) -> Option<Node> {
    if root.id() == id {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(found) = find_node_by_id_anywhere(child, id) {
            return Some(found);
        }
    }
    None
}

/// Removes every strict ancestor of `node` from `collapsed`, so `node` is guaranteed to appear in
/// that panel's `flatten_visible` output.
fn expand_ancestors(collapsed: &mut std::collections::HashSet<usize>, node: Node) {
    let mut current = node;
    while let Some(parent) = current.parent() {
        collapsed.remove(&parent.id());
        current = parent;
    }
}

/// Shared tail of `action_align`/`action_align_algo`: moves the *other* panel's cursor to
/// `target_id`. If the target is hidden under a collapsed ancestor, expands every ancestor along
/// its path so it becomes visible. If the target wasn't already on screen (whether because it was
/// hidden, or just scrolled out of view), centers the other panel's viewport on it;
/// `idx.saturating_sub(half).min(max_scroll)` naturally clamps that centering at the start/end of
/// the tree, where a true center isn't possible.
fn align_cursor_to(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
    target_id: usize,
) -> Result<String> {
    let other_root = match focus {
        Focus::Before => after_root,
        Focus::After => before_root,
    };
    let other = match focus {
        Focus::Before => &mut app.after,
        Focus::After => &mut app.before,
    };

    let was_visible = FlatIndex::new(flatten_visible(other_root, &other.collapsed, None))
        .index_of(target_id)
        .is_some_and(|idx| {
            idx >= other.scroll && idx < other.scroll + other.viewport_height.max(1)
        });

    let target_node = find_node_by_id_anywhere(other_root, target_id)
        .context("Matched node not found in tree")?;
    expand_ancestors(&mut other.collapsed, target_node);
    other.cursor_id = target_id;

    if !was_visible {
        let flat = FlatIndex::new(flatten_visible(other_root, &other.collapsed, None));
        let idx = flat.index_of(target_id).unwrap_or(0);
        let height = other.viewport_height.max(1);
        let max_scroll = flat.len().saturating_sub(height);
        other.scroll = idx.saturating_sub(height / 2).min(max_scroll);
    }

    Ok(format!("Aligned to matched '{}'", target_node.kind()))
}

/// Implements `a`: aligns to the node the *human mapping* says the cursor node is matched with, if
/// any. See [`align_cursor_to`] for how the target is made visible.
fn action_align(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (own_cursor, matches) = match focus {
        Focus::Before => (app.before.cursor_id, &caches.before_match),
        Focus::After => (app.after.cursor_id, &caches.after_match),
    };

    let target_id = *matches
        .get(&own_cursor)
        .context("Cursor node is not matched to anything")?;

    align_cursor_to(app, focus, before_root, after_root, target_id)
}

/// Implements `A`: like `a`, but aligns to the node *codediff's own diff* (`p`) says the cursor
/// node is mapped to, instead of the human mapping. Requires `p` to have been run at least once
/// for the current case, and fails if codediff mapped the cursor node to nothing (i.e. it
/// considers it deleted/inserted rather than matched).
fn action_align_algo(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
) -> Result<String> {
    let target_id = {
        let diff_ast = app
            .algo_diff
            .as_ref()
            .context("No codediff result yet; press 'p' to run it first")?;
        let own_cursor = match focus {
            Focus::Before => app.before.cursor_id,
            Focus::After => app.after.cursor_id,
        };
        let node_map = match focus {
            Focus::Before => &diff_ast.before_node_map,
            Focus::After => &diff_ast.after_node_map,
        };
        *node_map
            .get(&own_cursor)
            .context("codediff has no verdict for this node")?
    };

    if target_id == 0 {
        bail!("codediff maps this node to nothing (deleted/inserted), not to a matching node");
    }

    align_cursor_to(app, focus, before_root, after_root, target_id)
}

// ---------------------------------------------------------------------------------------------
// Marking actions
// ---------------------------------------------------------------------------------------------

/// Removes any existing entry whose before_path resolves to `before_id`, or whose after_path
/// resolves to `after_id`, so that re-marking a node cleanly replaces its previous decision
/// instead of leaving stale/contradictory entries behind.
fn remove_direct_entries_for(
    entries: &mut Vec<HumanMappingEntry>,
    before_id: Option<usize>,
    after_id: Option<usize>,
    before_root: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        let touches_before = before_id.is_some()
            && entry
                .before_path
                .as_ref()
                .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == before_id)
                .unwrap_or(false);
        let touches_after = after_id.is_some()
            && entry
                .after_path
                .as_ref()
                .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == after_id)
                .unwrap_or(false);
        !(touches_before || touches_after)
    });
}

/// Like [`remove_direct_entries_for`], but removes every entry touching *any* id in `before_ids`/
/// `after_ids` in one pass, instead of one id at a time. Used by `apply_modal_choice` to batch-clear
/// a whole subtree's worth of potential conflicts before `auto_match_pair` recurses into it and
/// appends entries directly -- doing this per-node instead (i.e. calling `remove_direct_entries_for`
/// once per node like `apply_match_entry` does) is what made `M` quadratic over a big subtree.
fn remove_entries_touching(
    entries: &mut Vec<HumanMappingEntry>,
    before_ids: &std::collections::HashSet<usize>,
    after_ids: &std::collections::HashSet<usize>,
    before_root: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        let touches_before = entry
            .before_path
            .as_ref()
            .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
            .is_some_and(|n| before_ids.contains(&n.id()));
        let touches_after = entry
            .after_path
            .as_ref()
            .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
            .is_some_and(|n| after_ids.contains(&n.id()));
        !(touches_before || touches_after)
    });
}

/// Finds a node anywhere in `root`'s subtree by id, unlike [`find_node_by_id`], which only looks
/// among the (possibly collapsed/hidden) visible rows a `flat` slice covers. Used only when
/// resolving a multi-map selection at commit time: a node toggled into `App::before_multi_select`/
/// `after_multi_select` with `x` can end up hidden by a later `Left`/`H` press on an ancestor
/// before `m`/`M` commits the group, and it must still resolve correctly then.
fn find_node_anywhere(root: Node, id: usize) -> Option<Node> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.id() == id {
            return Some(n);
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// Removes any existing [`MultiMapGroup`] that shares a node (on either side) with `before_ids`/
/// `after_ids` - the group-level counterpart of [`remove_entries_touching`], used so committing a
/// new multi-map selection can't leave a stale group half-referencing a node that just got
/// reassigned to a different group or a plain match.
fn remove_groups_touching(
    groups: &mut Vec<MultiMapGroup>,
    before_ids: &std::collections::HashSet<usize>,
    after_ids: &std::collections::HashSet<usize>,
    before_root: Node,
    after_root: Node,
) {
    groups.retain(|group| {
        let touches_before = group.before_paths.iter().any(|p| {
            node_for_path(before_root, &path_refs(p))
                .ok()
                .is_some_and(|n| before_ids.contains(&n.id()))
        });
        let touches_after = group.after_paths.iter().any(|p| {
            node_for_path(after_root, &path_refs(p))
                .ok()
                .is_some_and(|n| after_ids.contains(&n.id()))
        });
        !(touches_before || touches_after)
    });
}

fn is_strict_descendant_of(node: Node, ancestor: Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.id() == ancestor.id() {
            return true;
        }
        current = parent;
    }
    false
}

/// Drops any entry whose before_path resolves to a strict descendant of `ancestor`. Used when
/// marking `ancestor` deleted-with-children, so a previous direct mark on one of its descendants
/// (e.g. a Match) can't survive alongside it and export a self-contradictory mapping.
fn clear_before_descendants(
    entries: &mut Vec<HumanMappingEntry>,
    ancestor: Node,
    before_root: Node,
) {
    entries.retain(|entry| {
        !entry
            .before_path
            .as_ref()
            .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

/// Same as [`clear_before_descendants`], but for the After tree (used by insert-with-children).
fn clear_after_descendants(entries: &mut Vec<HumanMappingEntry>, ancestor: Node, after_root: Node) {
    entries.retain(|entry| {
        !entry
            .after_path
            .as_ref()
            .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

/// What an `m`/`M` press should do next: either it's fully resolved (a mapping entry was added,
/// or nothing needed to change), or it needs a direct human answer before anything is written.
enum ActionOutcome {
    Done(String),
    /// Boxed: `Modal`'s largest variant is much bigger than `Done`'s `String`, so an unboxed
    /// enum would pay that size on every `ActionOutcome` returned. One indirection on a
    /// keystroke-rate path is free; the size difference is what clippy flags.
    NeedsModal(Box<Modal>),
}

/// True if `b` and `a` have the exact same text.
fn node_values_equal(b: Node, a: Node, before_src: &[u8], after_src: &[u8]) -> bool {
    b.utf8_text(before_src).unwrap_or("") == a.utf8_text(after_src).unwrap_or("")
}

/// Replaces any existing direct entry touching `b` or `a` with a single new entry pairing them
/// under `operation`.
fn apply_match_entry(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    b: Node,
    a: Node,
    operation: HumanOperation,
) {
    remove_direct_entries_for(
        &mut mapping.entries,
        Some(b.id()),
        Some(a.id()),
        before_root,
        after_root,
    );
    mapping.entries.push(HumanMappingEntry {
        operation,
        before_path: Some(path_for_node(b)),
        after_path: Some(path_for_node(a)),
    });
}

/// Classifies a same-kind pair with children as `Identical` or `MatchButNotIdentical` without
/// asking: `before_hash`/`after_hash` are each node's precomputed full-content hash (kind + text +
/// children's hashes, folded bottom-up -- see `code::hash::hash_code`), so two subtrees hash equal
/// iff they're byte-identical. Missing hashes (shouldn't happen once `load_case` has run
/// `ensure_parsed`) are treated conservatively as not identical.
fn subtree_match_operation(
    before_id: usize,
    after_id: usize,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    let identical = matches!(
        (before_hash.get(&before_id), after_hash.get(&after_id)),
        (Some(b), Some(a)) if b == a
    );
    if identical {
        HumanOperation::Identical
    } else {
        HumanOperation::MatchButNotIdentical
    }
}

/// Classifies a multi-map selection's operation the same way [`subtree_match_operation`]
/// classifies a single pair - by full-content hash - generalized to a set: `Identical` only if
/// *every* selected node, on both sides, shares the exact same hash (the whole selection really is
/// N interchangeable copies of one subtree), `MatchButNotIdentical` otherwise. Never `Update` -
/// see `MultiMapGroup::operation`'s own doc comment for why a group has no single fixed pair to
/// call a text edit against.
fn multi_map_group_operation(
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    let mut hashes = before_ids
        .iter()
        .filter_map(|id| before_hash.get(id).copied())
        .chain(
            after_ids
                .iter()
                .filter_map(|id| after_hash.get(id).copied()),
        );
    let first = hashes.next();
    if first.is_some() && hashes.all(|h| Some(h) == first) {
        HumanOperation::Identical
    } else {
        HumanOperation::MatchButNotIdentical
    }
}

/// Commits `before_ids`/`after_ids` (a confirmed multi-map selection - see `App::before_multi_select`/
/// `after_multi_select`) as a new [`MultiMapGroup`], clearing out any prior plain entry or group
/// that touched one of these nodes first (the group-level equivalent of [`apply_match_entry`]'s own
/// "replace whatever was there" behavior for a single pair).
fn commit_multi_map_group(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    operation: HumanOperation,
    with_children: bool,
) -> Result<String> {
    let mut before_nodes = before_ids
        .iter()
        .map(|&id| {
            find_node_anywhere(before_root, id)
                .context("A selected Before node could no longer be found in the tree")
        })
        .collect::<Result<Vec<Node>>>()?;
    let mut after_nodes = after_ids
        .iter()
        .map(|&id| {
            find_node_anywhere(after_root, id)
                .context("A selected After node could no longer be found in the tree")
        })
        .collect::<Result<Vec<Node>>>()?;
    // Deterministic, parse-stable order - not the arena-id order iterating a `BTreeSet<usize>`
    // would otherwise produce (see the project's benchmark-determinism-fix lesson on node ids as
    // ordering keys), and the same order `representative_entries` sorts its own pairing by.
    before_nodes.sort_by_key(|n| n.start_byte());
    after_nodes.sort_by_key(|n| n.start_byte());

    let before_id_set: std::collections::HashSet<usize> = before_ids.iter().copied().collect();
    let after_id_set: std::collections::HashSet<usize> = after_ids.iter().copied().collect();
    remove_entries_touching(
        &mut mapping.entries,
        &before_id_set,
        &after_id_set,
        before_root,
        after_root,
    );
    remove_groups_touching(
        &mut mapping.groups,
        &before_id_set,
        &after_id_set,
        before_root,
        after_root,
    );
    if with_children {
        // A member's descendants must stay free to close over whichever specific pair codediff's
        // own diff actually realizes (see `check_subtree_maps_within`) - a leftover pre-existing
        // entry on one would otherwise pin a pairing the group deliberately leaves open, or flatly
        // contradict a leftover member's "this whole subtree must be removed" requirement. Same
        // "with-children can't coexist with a descendant mark" invariant `d`/`i`'s own
        // `clear_before_descendants`/`clear_after_descendants` calls already enforce.
        for &node in &before_nodes {
            clear_before_descendants(&mut mapping.entries, node, before_root);
        }
        for &node in &after_nodes {
            clear_after_descendants(&mut mapping.entries, node, after_root);
        }
    }

    let (before_count, after_count) = (before_nodes.len(), after_nodes.len());
    mapping.groups.push(MultiMapGroup {
        before_paths: before_nodes.into_iter().map(path_for_node).collect(),
        after_paths: after_nodes.into_iter().map(path_for_node).collect(),
        operation,
        with_children,
    });

    Ok(format!(
        "Committed multi-map group: {} before, {} after node(s), {:?}{}",
        before_count,
        after_count,
        operation,
        if with_children { " with children" } else { "" }
    ))
}

/// What `m`/`M` does when the multi-map selection (`App::before_multi_select`/`after_multi_select`)
/// is non-empty: infers the group's operation (see [`multi_map_group_operation`]), then either
/// commits it directly (every selected node shares one AST kind) or raises
/// `Modal::ConfirmMultiMapGroup` first - the group-level counterpart of [`kind_mismatch_modal`].
#[allow(clippy::too_many_arguments)]
fn action_commit_multi_map_group(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    caches: &Caches,
    with_children: bool,
) -> Result<ActionOutcome> {
    if before_ids.is_empty() || after_ids.is_empty() {
        bail!(
            "Multi-map group needs at least one selected node on both sides (x to select, c to clear)"
        );
    }

    // Same precondition `action_match`/`action_match_subtree` enforce for a single pair: a member
    // sitting under an ancestor already marked deleted/inserted-with-children can't also be
    // committed into a group without producing a self-contradictory mapping.
    for &id in before_ids {
        if let Some(node) = find_node_anywhere(before_root, id)
            && is_inherited_removed(node, &caches.before_removed)
        {
            bail!(
                "Before node '{}' is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)",
                node.kind()
            );
        }
    }
    for &id in after_ids {
        if let Some(node) = find_node_anywhere(after_root, id)
            && is_inherited_removed(node, &caches.after_removed)
        {
            bail!(
                "After node '{}' is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)",
                node.kind()
            );
        }
    }

    let operation = multi_map_group_operation(before_ids, after_ids, before_hash, after_hash);

    let kinds: Vec<String> = before_ids
        .iter()
        .filter_map(|&id| find_node_anywhere(before_root, id))
        .chain(
            after_ids
                .iter()
                .filter_map(|&id| find_node_anywhere(after_root, id)),
        )
        .map(|n| n.kind().to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if kinds.len() > 1 {
        return Ok(ActionOutcome::NeedsModal(Box::new(
            Modal::ConfirmMultiMapGroup {
                before_ids: before_ids.iter().copied().collect(),
                after_ids: after_ids.iter().copied().collect(),
                operation,
                with_children,
                kinds,
            },
        )));
    }

    let msg = commit_multi_map_group(
        mapping,
        before_root,
        after_root,
        before_ids,
        after_ids,
        operation,
        with_children,
    )?;
    Ok(ActionOutcome::Done(msg))
}

/// Builds the `NeedsModal` outcome for a before/after cursor pair whose kinds don't match -
/// shared by every action that requires same-kind nodes before it can proceed. `recursive`
/// distinguishes a single-node match attempt (`m`, `false`) from a whole-subtree one (`M`, `true`)
/// - see [`Modal::ConfirmKindMismatch`].
fn kind_mismatch_modal(before_node: Node, after_node: Node, recursive: bool) -> ActionOutcome {
    ActionOutcome::NeedsModal(Box::new(Modal::ConfirmKindMismatch {
        before_id: before_node.id(),
        after_id: after_node.id(),
        before_kind: before_node.kind().to_string(),
        after_kind: after_node.kind().to_string(),
        recursive,
    }))
}

/// Classifies a same-kind cursor pair as it would be auto-classified by a single `m` press:
/// `Identical`/`Update` by raw text for a leaf pair, or via [`subtree_match_operation`] (content
/// hash) for a pair with children. Shared by [`action_match`] and [`action_match_to_end`], which
/// both just need the resulting operation before continuing their own, differing follow-up logic
/// - unlike [`action_match_subtree`]'s leaf case, which resolves and returns immediately instead
///   of continuing, so it classifies its own leaf pairs inline rather than sharing this helper.
fn classify_match_operation(
    before_node: Node,
    after_node: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        if node_values_equal(before_node, after_node, before_src, after_src) {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        }
    } else {
        subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash)
    }
}

#[allow(clippy::too_many_arguments)]
fn action_match(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> Result<ActionOutcome> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    if before_node.kind() != after_node.kind() {
        return Ok(kind_mismatch_modal(before_node, after_node, false));
    }

    let operation = classify_match_operation(
        before_node,
        after_node,
        before_src,
        after_src,
        before_hash,
        after_hash,
    );
    apply_match_entry(
        mapping,
        before_root,
        after_root,
        before_node,
        after_node,
        operation,
    );
    Ok(ActionOutcome::Done(format!(
        "Matched '{}' <-> '{}' as {:?}",
        before_node.kind(),
        after_node.kind(),
        operation
    )))
}

/// Implements `f`: repeats exactly what a single `m` press does -- match the Before and After
/// cursor nodes (auto-classified Identical/Update for leaves, or by content hash for nodes with
/// children, precisely like [`action_match`]), then advance both cursors to their own next
/// `Unmarked` node -- over and over, as if `m` were being pressed by hand again and again.
///
/// Stops in exactly the two places a human doing that would have to stop too: once neither cursor
/// has an `Unmarked` node left to advance to (there's nothing left to pair up -- end of file), or
/// the moment the next pair has different kinds, which is precisely when a real `m` press would
/// raise [`Modal::ConfirmKindMismatch`] instead of matching outright. Every match applied before
/// that point is kept; pressing `f` again after resolving the mismatch (or manually) resumes the
/// sweep from the new cursor position.
///
/// Deliberately doesn't go through [`apply_match_entry`]/[`rebuild_caches`]/[`path_for_node`] on
/// every single node the way a literal "call `action_match` in a loop" implementation would:
/// those are built for a human's pace (one call per keypress, cost spread over real time), and
/// each costs O(current entry count) or O(sibling count) -- fine for a single `m` press, but this
/// loop can run once per AST node, so paying that on every iteration turns an O(n) sweep into
/// O(n^2). Two real fixtures exposed this: a ~5,500-node real-world file took 26s to reach 740
/// matches and climbing (`rebuild_caches`/`apply_match_entry`'s O(entries)-per-call cost), and a
/// large flat JSON-array-shaped tree took 52s even after that fix (`path_for_node`'s O(siblings)
/// occurrence-counting, paid per node, on a level with thousands of same-kind children). Both are
/// worked around here: `caches` is built once and updated incrementally in place; entries are
/// appended directly, skipping `apply_match_entry`'s dedup scan (provably a no-op here, since
/// `status_before`/`status_after` having just reported `Unmarked` means neither node has an
/// existing entry to remove); cursors are tracked as plain indices into `before_flat`/`after_flat`
/// so advancing never re-scans from the start; and every node's path is looked up in a table
/// built by one O(n) pass per tree ([`precompute_paths`]) instead of walked fresh from each node.
#[allow(clippy::too_many_arguments)]
fn action_match_to_end(
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> Result<ActionOutcome> {
    let mut matched = 0usize;
    let mut caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let before_paths = precompute_paths(before_root);
    let after_paths = precompute_paths(after_root);

    let mut before_idx = before_flat
        .index_of(app.before.cursor_id)
        .context("Before cursor node not found")?;
    let mut after_idx = after_flat
        .index_of(app.after.cursor_id)
        .context("After cursor node not found")?;

    loop {
        let before_node = before_flat[before_idx].0;
        let after_node = after_flat[after_idx].0;

        // Nothing left to pair up on at least one side: reached the end of what `f` can do. (This
        // also covers a node under an inherited delete/insert-with-children mark, since
        // `status_before`/`status_after` never report those as `Unmarked`.)
        if status_before(before_node, &caches) != NodeStatus::Unmarked
            || status_after(after_node, &caches) != NodeStatus::Unmarked
        {
            break;
        }

        if before_node.kind() != after_node.kind() {
            app.before.cursor_id = before_node.id();
            app.after.cursor_id = after_node.id();
            return Ok(kind_mismatch_modal(before_node, after_node, false));
        }

        let operation = classify_match_operation(
            before_node,
            after_node,
            before_src,
            after_src,
            before_hash,
            after_hash,
        );
        app.mapping.entries.push(HumanMappingEntry {
            operation,
            before_path: before_paths.get(&before_node.id()).cloned(),
            after_path: after_paths.get(&after_node.id()).cloned(),
        });
        caches
            .before_match
            .insert(before_node.id(), after_node.id());
        caches.after_match.insert(after_node.id(), before_node.id());
        app.dirty = true;
        matched += 1;

        let next_before = next_unmarked_index(before_idx + 1, before_flat, &caches, status_before);
        let next_after = next_unmarked_index(after_idx + 1, after_flat, &caches, status_after);
        if let Some(idx) = next_before {
            before_idx = idx;
        }
        if let Some(idx) = next_after {
            after_idx = idx;
        }
        if next_before.is_none() || next_after.is_none() {
            break;
        }
    }

    app.before.cursor_id = before_flat[before_idx].0.id();
    app.after.cursor_id = after_flat[after_idx].0.id();

    Ok(ActionOutcome::Done(if matched == 0 {
        "Nothing left to match".to_string()
    } else {
        format!("Matched {matched} pair(s) up to end of file")
    }))
}

/// The first index at or after `start` whose node is `Unmarked`, or `None` if there isn't one.
/// Callers that advance `start` monotonically across repeated calls (as `action_match_to_end`
/// does) get amortized O(n) total work rather than O(n) *per call* -- each slot in `flat` is only
/// ever examined once across the whole sweep.
fn next_unmarked_index(
    start: usize,
    flat: &[(Node, usize)],
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> Option<usize> {
    (start..flat.len()).find(|&i| status_fn(flat[i].0, caches) == NodeStatus::Unmarked)
}

#[allow(clippy::too_many_arguments)]
fn action_match_subtree(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> Result<ActionOutcome> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    if before_node.kind() != after_node.kind() {
        return Ok(kind_mismatch_modal(before_node, after_node, true));
    }

    // A leaf top pair has no children to auto-fill, so resolve it immediately like `m` does.
    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        let identical = node_values_equal(before_node, after_node, before_src, after_src);
        let operation = if identical {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        };
        apply_match_entry(
            mapping,
            before_root,
            after_root,
            before_node,
            after_node,
            operation,
        );
        return Ok(ActionOutcome::Done(format!(
            "Matched '{}' <-> '{}' as {}",
            before_node.kind(),
            after_node.kind(),
            if identical { "Identical" } else { "Update" }
        )));
    }

    let operation =
        subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash);
    let msg = apply_modal_choice(
        mapping,
        before_flat,
        after_flat,
        before_root,
        after_root,
        caches,
        before_src,
        after_src,
        before_node.id(),
        after_node.id(),
        operation,
        true,
        before_collapsed,
        after_collapsed,
    );
    Ok(ActionOutcome::Done(msg))
}

/// Auto-matches `b` <-> `a` and all descendants, with no prompting: leaves are classified
/// Identical/Update by comparing text; container nodes are classified Identical only if every
/// descendant came back Identical too, otherwise MatchButNotIdentical. Recursion stops (without
/// matching further) the moment a level's child-kind sequences diverge, or a node is already
/// covered by an unrelated ancestor mark. Returns whether the whole subtree matched Identically.
///
/// Used to bulk-fill the rest of an `M` (recursive match) after the top-level pair's own operation
/// has already been decided (via `subtree_match_operation` or a confirmed `Modal::ConfirmKindMismatch`)
/// -- classifying each descendant individually by hash keeps this fast for a tree of any size.
///
/// Any pair (this one or a descendant) that ends up classified `Identical` and has children is
/// also collapsed in both panels, so a whole-unchanged subtree doesn't clutter the view -- this is
/// the main payoff of `M` over doing the same matches one at a time with `m`.
///
/// Rather than pushing straight into `mapping.entries` (via `apply_match_entry`, which costs
/// O(current entry count) per call through its dedup scan -- recursing over a subtree of size k
/// would turn that into O(k^2), exactly the hang `action_match_to_end` (`f`) had before it was
/// fixed the same way, and `M` hits it too since this function is its recursive workhorse), this
/// buffers new entries into `new_entries` and records every node id it actually decides on into
/// `touched_before`/`touched_after`. The caller ([`apply_modal_choice`]) removes pre-existing
/// entries for exactly those touched ids in one batch pass *after* recursion finishes, then
/// appends `new_entries` -- cheaper than a scan per node, and correct in a way that eagerly
/// collecting a subtree's ids up front isn't: recursion can bail out of a node early (kind
/// mismatch or a shape mismatch) without visiting its descendants at all, and a descendant that
/// was never visited must keep whatever pre-existing entry it had, not have it wiped because it
/// happened to be nested under the node `M` was pressed on.
///
/// Paths are looked up from `before_paths`/`after_paths` (each precomputed once, in
/// `apply_modal_choice`, by [`precompute_paths`]) instead of calling `path_for_node` fresh per
/// node, for the same reason.
#[allow(clippy::too_many_arguments)]
fn auto_match_pair(
    new_entries: &mut Vec<HumanMappingEntry>,
    touched_before: &mut std::collections::HashSet<usize>,
    touched_after: &mut std::collections::HashSet<usize>,
    caches: &Caches,
    b: Node,
    a: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_paths: &HashMap<usize, Vec<String>>,
    after_paths: &HashMap<usize, Vec<String>>,
    matched: &mut usize,
    skipped: &mut usize,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> bool {
    if is_inherited_removed(b, &caches.before_removed)
        || is_inherited_removed(a, &caches.after_removed)
    {
        *skipped += 1;
        return false;
    }

    let push = |new_entries: &mut Vec<HumanMappingEntry>,
                touched_before: &mut std::collections::HashSet<usize>,
                touched_after: &mut std::collections::HashSet<usize>,
                operation: HumanOperation| {
        new_entries.push(HumanMappingEntry {
            operation,
            before_path: before_paths.get(&b.id()).cloned(),
            after_path: after_paths.get(&a.id()).cloned(),
        });
        touched_before.insert(b.id());
        touched_after.insert(a.id());
    };

    if b.kind() != a.kind() {
        // Shouldn't happen for children reached via the same_shape check below, but the very
        // first call into this function (the top pair's children) hasn't been shape-checked yet.
        push(
            new_entries,
            touched_before,
            touched_after,
            HumanOperation::MatchButNotIdentical,
        );
        *matched += 1;
        return false;
    }

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();

    if b_children.is_empty() && a_children.is_empty() {
        let identical = node_values_equal(b, a, before_src, after_src);
        push(
            new_entries,
            touched_before,
            touched_after,
            if identical {
                HumanOperation::Identical
            } else {
                HumanOperation::Update
            },
        );
        *matched += 1;
        return identical;
    }

    let same_shape = b_children.len() == a_children.len()
        && b_children
            .iter()
            .zip(&a_children)
            .all(|(x, y)| x.kind() == y.kind());

    if !same_shape {
        push(
            new_entries,
            touched_before,
            touched_after,
            HumanOperation::MatchButNotIdentical,
        );
        *matched += 1;
        return false;
    }

    let mut all_identical = true;
    for (b_child, a_child) in b_children.into_iter().zip(a_children) {
        let child_identical = auto_match_pair(
            new_entries,
            touched_before,
            touched_after,
            caches,
            b_child,
            a_child,
            before_src,
            after_src,
            before_paths,
            after_paths,
            matched,
            skipped,
            before_collapsed,
            after_collapsed,
        );
        all_identical &= child_identical;
    }

    push(
        new_entries,
        touched_before,
        touched_after,
        if all_identical {
            HumanOperation::Identical
        } else {
            HumanOperation::MatchButNotIdentical
        },
    );
    *matched += 1;
    if all_identical {
        before_collapsed.insert(b.id());
        after_collapsed.insert(a.id());
    }
    all_identical
}

/// Applies `operation` to the top pair -- whether decided by `subtree_match_operation` (`M`) or a
/// confirmed `Modal::ConfirmKindMismatch` -- and if `recursive`, auto-fills the rest of the subtree
/// via [`auto_match_pair`].
#[allow(clippy::too_many_arguments)]
fn apply_modal_choice(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_id: usize,
    after_id: usize,
    operation: HumanOperation,
    recursive: bool,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> String {
    let (Some(b), Some(a)) = (
        before_flat.node_for_id(before_id),
        after_flat.node_for_id(after_id),
    ) else {
        return "Node no longer available (tree changed?)".to_string();
    };

    apply_match_entry(mapping, before_root, after_root, b, a, operation);

    if !recursive {
        return format!(
            "Matched '{}' <-> '{}' as {:?}",
            b.kind(),
            a.kind(),
            operation
        );
    }

    if operation == HumanOperation::Identical {
        before_collapsed.insert(b.id());
        after_collapsed.insert(a.id());
    }

    let mut matched = 1usize;
    let mut skipped = 0usize;

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();
    let same_shape = b_children.len() == a_children.len()
        && b_children
            .iter()
            .zip(&a_children)
            .all(|(x, y)| x.kind() == y.kind());

    if same_shape {
        let before_paths = precompute_paths(before_root);
        let after_paths = precompute_paths(after_root);
        let mut new_entries = Vec::new();
        let mut touched_before = std::collections::HashSet::new();
        let mut touched_after = std::collections::HashSet::new();

        for (b_child, a_child) in b_children.into_iter().zip(a_children) {
            auto_match_pair(
                &mut new_entries,
                &mut touched_before,
                &mut touched_after,
                caches,
                b_child,
                a_child,
                before_src,
                after_src,
                &before_paths,
                &after_paths,
                &mut matched,
                &mut skipped,
                before_collapsed,
                after_collapsed,
            );
        }

        // Batched equivalent of what `apply_match_entry`'s per-node dedup scan would otherwise do
        // node by node inside `auto_match_pair` (an O(existing entries) scan for every node in the
        // subtree, which goes quadratic over a big one -- see `auto_match_pair`'s doc comment):
        // clear out, in one pass, any pre-existing entry that touches a node the recursion above
        // actually decided on, *then* append what it produced. Using the ids `auto_match_pair`
        // actually touched (rather than every id in the subtree) matters: a node the recursion
        // bailed out of without visiting keeps whatever pre-existing entry it had.
        remove_entries_touching(
            &mut mapping.entries,
            &touched_before,
            &touched_after,
            before_root,
            after_root,
        );
        mapping.entries.extend(new_entries);
    }

    if skipped > 0 {
        format!(
            "Matched {} node pair(s) under '{}' <-> '{}'; skipped {} node(s) already covered by an unrelated ancestor mark",
            matched,
            b.kind(),
            a.kind(),
            skipped
        )
    } else {
        format!(
            "Matched {} node pair(s) under '{}' <-> '{}'",
            matched,
            b.kind(),
            a.kind()
        )
    }
}

fn action_delete(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    before_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Node is already covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }

    remove_direct_entries_for(
        &mut mapping.entries,
        Some(before_node.id()),
        None,
        before_root,
        after_root,
    );
    if with_children {
        clear_before_descendants(&mut mapping.entries, before_node, before_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::DeleteWithChildren
        } else {
            HumanOperation::Delete
        },
        before_path: Some(path_for_node(before_node)),
        after_path: None,
    });

    Ok(format!(
        "Marked '{}' deleted{}",
        before_node.kind(),
        if with_children {
            " (with children)"
        } else {
            ""
        }
    ))
}

fn action_insert(
    mapping: &mut HumanMapping,
    after_flat: &FlatIndex,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "Node is already covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    remove_direct_entries_for(
        &mut mapping.entries,
        None,
        Some(after_node.id()),
        before_root,
        after_root,
    );
    if with_children {
        clear_after_descendants(&mut mapping.entries, after_node, after_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::InsertWithChildren
        } else {
            HumanOperation::Insert
        },
        before_path: None,
        after_path: Some(path_for_node(after_node)),
    });

    Ok(format!(
        "Marked '{}' inserted{}",
        after_node.kind(),
        if with_children {
            " (with children)"
        } else {
            ""
        }
    ))
}

// Each parameter is genuinely distinct context (the mapping, focus, both sides' flattened node
// lists, both cursors, both roots, the caches) - a params struct here would just relocate the
// same fields, not reduce them.
#[allow(clippy::too_many_arguments)]
fn action_unmark(
    mapping: &mut HumanMapping,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (id, node, removed, group) = match focus {
        Focus::Before => (
            before_cursor,
            before_flat
                .node_for_id(before_cursor)
                .context("Before cursor node not found")?,
            &caches.before_removed,
            caches.before_group.get(&before_cursor).copied(),
        ),
        Focus::After => (
            after_cursor,
            after_flat
                .node_for_id(after_cursor)
                .context("After cursor node not found")?,
            &caches.after_removed,
            caches.after_group.get(&after_cursor).copied(),
        ),
    };

    // A group member (whichever specific pair `representative_entries` realized, or a leftover)
    // isn't recorded as its own `mapping.entries` item at all - the whole group is one
    // `MultiMapGroup` in `mapping.groups` - so `u` here removes that entire group rather than
    // trying (and failing) to find a single direct entry to drop.
    if let Some(group_idx) = group
        && group_idx < mapping.groups.len()
    {
        let removed_group = mapping.groups.remove(group_idx);
        return Ok(format!(
            "Removed multi-map group ({} before, {} after node(s))",
            removed_group.before_paths.len(),
            removed_group.after_paths.len()
        ));
    }

    let before_id = if focus == Focus::Before {
        Some(id)
    } else {
        None
    };
    let after_id = if focus == Focus::After {
        Some(id)
    } else {
        None
    };

    let before_len = mapping.entries.len();
    remove_direct_entries_for(
        &mut mapping.entries,
        before_id,
        after_id,
        before_root,
        after_root,
    );

    if mapping.entries.len() < before_len {
        return Ok(format!("Unmarked '{}'", node.kind()));
    }

    if is_inherited_removed(node, removed) {
        bail!(
            "This node is only covered via an ancestor's with-children mark; clear the ancestor instead"
        );
    }

    Ok(format!("'{}' was not marked", node.kind()))
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Side {
    Before,
    After,
}

fn node_label(node: Node, src: &[u8]) -> String {
    if node.child_count() == 0 {
        let text = node.utf8_text(src).unwrap_or("");
        let truncated: String = text.chars().take(40).collect();
        let ellipsis = if text.chars().count() > 40 { "..." } else { "" };
        format!("{} {:?}{}", node.kind(), truncated, ellipsis)
    } else {
        node.kind().to_string()
    }
}

fn status_glyph_and_style(status: NodeStatus) -> (&'static str, Style) {
    match status {
        NodeStatus::Unmarked => (" ", Style::default().fg(Color::Gray)),
        NodeStatus::Matched => ("M", Style::default().fg(Color::Cyan)),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: false,
            inherited: false,
        } => ("-", Style::default().fg(Color::Red)),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: true,
            inherited: false,
        } => (
            "-",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            inherited: true,
            ..
        } => (
            "-",
            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: false,
            inherited: false,
        } => ("+", Style::default().fg(Color::Green)),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: true,
            inherited: false,
        } => (
            "+",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            inherited: true,
            ..
        } => (
            "+",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    flat: &FlatIndex,
    panel: &mut PanelState,
    caches: &Caches,
    side: Side,
    src: &[u8],
    focused: bool,
    algo_diff: Option<&ASTDiff>,
    show_reason: bool,
    total_unmarked: usize,
    multi_selected: &std::collections::BTreeSet<usize>,
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    panel.viewport_height = inner_height;
    let cursor_idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    ensure_visible(&mut panel.scroll, cursor_idx, inner_height);

    // Only the rows actually on screen get built into `ListItem`s and have their status computed
    // -- `total_unmarked` (the header's "N unmarked" count) is the caller's `FrameState`'s, built
    // once per `compute_frame_state` call rather than by scanning all of `flat` here on every draw.
    let visible_end = (panel.scroll + inner_height.max(1)).min(flat.len());
    let mut items: Vec<ListItem> = Vec::with_capacity(inner_height.max(1));
    for (idx, &(node, depth)) in flat.iter().enumerate().take(visible_end).skip(panel.scroll) {
        let status = match side {
            Side::Before => status_before(node, caches),
            Side::After => status_after(node, caches),
        };

        let (glyph, mut style) = status_glyph_and_style(status);
        // A "g" suffix marks a node whose match/delete/insert outcome came from a `MultiMapGroup`
        // rather than a plain entry - `caches.before_group`/`after_group` cover every group
        // member (matched *and* leftover), not just whichever pair `representative_entries`
        // realized, so this is accurate for both.
        let in_group = match side {
            Side::Before => caches.before_group.contains_key(&node.id()),
            Side::After => caches.after_group.contains_key(&node.id()),
        };
        let group_marker = if in_group { "g" } else { "" };
        let (algo_glyph, disagrees) = algo_diff
            .map(|diff_ast| {
                let algo_status = match side {
                    Side::Before => algo_status_before(node, diff_ast),
                    Side::After => algo_status_after(node, diff_ast),
                };
                let disagrees = match side {
                    Side::Before => algo_disagrees_before(node, caches, diff_ast),
                    Side::After => algo_disagrees_after(node, caches, diff_ast),
                };
                let reason_suffix = if show_reason {
                    let reason = match side {
                        Side::Before => algo_reason_before(node, diff_ast),
                        Side::After => algo_reason_after(node, diff_ast),
                    };
                    reason
                        .map(|r| format!(" {}", reason_detail(r)))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                (
                    format!("({}{})", algo_status_glyph(algo_status), reason_suffix),
                    disagrees,
                )
            })
            .unwrap_or_default();
        let indent = "  ".repeat(depth);
        let marker = if disagrees { " *" } else { "" };
        let text = format!(
            "{}{}{}{} {}{}",
            indent,
            glyph,
            group_marker,
            algo_glyph,
            node_label(node, src),
            marker
        );

        // Pending multi-map selection (`x`, not yet committed by `m`/`M`) - a distinct color so
        // it reads as "about to become a group", separate from any already-committed status.
        if multi_selected.contains(&node.id()) {
            style = style.fg(Color::Magenta).add_modifier(Modifier::BOLD);
        }

        if idx == cursor_idx {
            style = style
                .bg(if focused {
                    Color::Yellow
                } else {
                    Color::DarkGray
                })
                .fg(Color::Black);
        }

        items.push(ListItem::new(Line::from(Span::styled(text, style))));
    }

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} — {} nodes, {} unmarked",
            title,
            flat.len(),
            total_unmarked
        ))
        .border_style(border_style);

    frame.render_widget(List::new(items).block(block), area);
}

/// Below this terminal width, `draw_ui` shows only the focused Before/After panel at full width
/// instead of splitting the screen 50/50 - two half-width panels wrap almost every line and
/// become unreadable on a narrow terminal. Shared with the main TUI's `DiffViewer`, which faces
/// the same readability constraint.
const SINGLE_PANEL_WIDTH_THRESHOLD: u16 =
    codediff::tui::components::diff_viewer::SINGLE_PANEL_THRESHOLD;

// Each parameter is genuinely distinct rendering context (the frame, app state, both sides'
// flattened node lists, the caches, both raw sources, both unmarked counts) - a params struct
// here would just relocate the same fields, not reduce them.
#[allow(clippy::too_many_arguments)]
fn draw_ui(
    frame: &mut Frame,
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_unmarked: usize,
    after_unmarked: usize,
    name: &str,
) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(size);

    let dataset_tag = match &app.origin {
        CaseOrigin::Diffs => case_dataset(name).unwrap_or_else(|| "?".to_string()),
        CaseOrigin::Sample(_) => "sample".to_string(),
        CaseOrigin::GitCommitFile { .. } => "git".to_string(),
    };
    frame.render_widget(
        Paragraph::new(format!(" human_solver — {} [{}] ", name, dataset_tag))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    // Below `SINGLE_PANEL_WIDTH_THRESHOLD` columns, two 50%-wide panels wrap every line and become
    // unreadable, so show only the focused panel at full width instead - `Tab` (which already
    // toggles `app.focus`) becomes the way to see the other side.
    let single_panel = size.width < SINGLE_PANEL_WIDTH_THRESHOLD;

    if single_panel {
        let panel_area = chunks[1];
        let (title, flat, panel, side, src, total_unmarked, multi_selected) = match app.focus {
            Focus::Before => (
                "Before",
                before_flat,
                &mut app.before,
                Side::Before,
                before_src,
                before_unmarked,
                &app.before_multi_select,
            ),
            Focus::After => (
                "After",
                after_flat,
                &mut app.after,
                Side::After,
                after_src,
                after_unmarked,
                &app.after_multi_select,
            ),
        };
        render_panel(
            frame,
            panel_area,
            title,
            flat,
            panel,
            caches,
            side,
            src,
            true,
            app.algo_diff.as_ref(),
            app.show_reason,
            total_unmarked,
            multi_selected,
        );
    } else {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        render_panel(
            frame,
            panels[0],
            "Before",
            before_flat,
            &mut app.before,
            caches,
            Side::Before,
            before_src,
            app.focus == Focus::Before,
            app.algo_diff.as_ref(),
            app.show_reason,
            before_unmarked,
            &app.before_multi_select,
        );
        render_panel(
            frame,
            panels[1],
            "After",
            after_flat,
            &mut app.after,
            caches,
            Side::After,
            after_src,
            app.focus == Focus::After,
            app.algo_diff.as_ref(),
            app.show_reason,
            after_unmarked,
            &app.after_multi_select,
        );
    }

    let footer = format!(
        "{}{}{}\nm/M match[+children]  x select for multi-map  c clear selection  f match to EOF  d/D delete[+children]  i/I insert[+children]  a/A align (human/codediff)  p run codediff  r toggle reason  n/N next/prev mismatch  t text view  T unix diff  H hide solved  u unmark  h/l ←/→ collapse/expand  j/k ↑/↓ move  g/G top/bottom  Tab switch  s save  ? help  q quit",
        app.status.clone().unwrap_or_default(),
        if app.dirty { "  [UNSAVED]" } else { "" },
        if caches.unresolved > 0 {
            format!(
                "  [{} mapping entries could not be resolved against the current tree and were ignored]",
                caches.unresolved
            )
        } else {
            String::new()
        },
    );
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: true }), chunks[2]);

    if let Some(modal) = &app.modal {
        render_modal(
            frame,
            size,
            modal,
            name,
            promote_target_dataset(&app.origin),
            std::str::from_utf8(before_src).unwrap_or(""),
            std::str::from_utf8(after_src).unwrap_or(""),
            &app.mapping,
            &app.text_solution,
            app.text_overlay,
            app.algo_text_spans.as_ref(),
            app.diff_completeness.as_ref(),
            app.diff_text_painted.as_ref(),
            app.diff_comments.as_ref(),
        );
    }
}

/// A `percent_x` x `percent_y` box centered within `area`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Like `centered_rect`, but never shrinks the popup below `min_width`/`min_height` (still capped
/// to `area` itself, since a terminal can be smaller than the popup's actual content needs) -
/// effectively reducing the percentage-based margin/padding on a small terminal instead of just
/// letting the content not fit. Real, not hypothetical: on a small terminal (an SSH client on a
/// phone is the motivating case), `render_text_modal`'s `centered_rect(60, 30, area)` could come
/// out short enough that the `> {input}` line - well past the first couple of lines of
/// instructions - scrolled out of the visible area entirely, with no scroll indicator to hint why,
/// since a plain `Paragraph` has no "not everything fit" affordance of its own.
fn centered_rect_at_least(
    percent_x: u16,
    percent_y: u16,
    min_width: u16,
    min_height: u16,
    area: Rect,
) -> Rect {
    let base = centered_rect(percent_x, percent_y, area);
    let width = base.width.max(min_width).min(area.width);
    let height = base.height.max(min_height).min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_modal(
    frame: &mut Frame,
    area: Rect,
    modal: &Modal,
    current_name: &str,
    promote_dataset: Option<&str>,
    before_src: &str,
    after_src: &str,
    mapping: &HumanMapping,
    text_solution: &str,
    text_overlay: TextOverlay,
    algo_text_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    diff_completeness: Option<&std::collections::HashMap<String, bool>>,
    diff_text_painted: Option<&std::collections::HashMap<String, bool>>,
    diff_comments: Option<&std::collections::HashMap<String, String>>,
) {
    match modal {
        Modal::ConfirmKindMismatch {
            before_kind,
            after_kind,
            ..
        } => render_text_modal(
            frame,
            area,
            "Node kinds do not match!",
            &format!(
                "Before: {}\nAfter:  {}\n\nAre you sure you want to add this mapping? (y/n)",
                before_kind, after_kind
            ),
        ),
        Modal::ConfirmMultiMapGroup {
            before_ids,
            after_ids,
            operation,
            with_children,
            kinds,
        } => render_text_modal(
            frame,
            area,
            "Multi-map group has mixed node kinds!",
            &format!(
                "{} Before node(s), {} After node(s), kinds: {}\nWill be recorded as {:?}{}.\n\nAre you sure you want to add this group? (y/n)",
                before_ids.len(),
                after_ids.len(),
                kinds.join(", "),
                operation,
                if *with_children { " with children" } else { "" }
            ),
        ),
        Modal::OpenDiffPicker {
            options,
            selected,
            dataset_filter,
            hide_complete,
            hide_painted,
        } => {
            render_open_diff_picker(
                frame,
                area,
                options,
                *selected,
                *dataset_filter,
                *hide_complete,
                diff_completeness,
                *hide_painted,
                diff_text_painted,
                diff_comments,
            );
        }
        Modal::OpenSamplePicker {
            options,
            selected,
            hide_solved,
            sort_order,
        } => {
            render_open_sample_picker(frame, area, options, *selected, *hide_solved, *sort_order);
        }
        Modal::ConfirmDiscardUnsaved { target, can_save } => render_text_modal(
            frame,
            area,
            "Unsaved changes",
            &if *can_save {
                format!(
                    "'{}' has unsaved changes.\n\nSave before opening '{}'?\n\n[s] Save & Open    [d] Discard & Open    [Esc] Cancel",
                    current_name,
                    target.name()
                )
            } else {
                format!(
                    "'{}' has unsaved changes (not a real test case yet; promote it with 's' from the main view to save it).\n\nOpen '{}' anyway?\n\n[d] Discard & Open    [Esc] Cancel",
                    current_name,
                    target.name()
                )
            },
        ),
        Modal::PromptPromoteName { input, error } => render_text_modal(
            frame,
            area,
            "Promote to test case",
            &format!(
                "Enter a name for src/test/data/diffs/{}/<name>/\n(letters, digits, - and _; must not already exist)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                promote_dataset.unwrap_or("?"),
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptRejectReason { input, error } => render_text_modal(
            frame,
            area,
            "Reject sample",
            &format!(
                "Enter a reason this sample is being rejected (recorded as-is in sample.csv)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptComment { input, error } => render_text_modal(
            frame,
            area,
            "Sample comment",
            &format!(
                "Enter or edit a comment for this sample (recorded as-is in sample.csv;\nempty clears it; written into the generated test stub if present at promote time)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptSearch { input } => render_text_modal(
            frame,
            area,
            "Search node text",
            &format!(
                "Find the next leaf node (in the focused panel) whose own\ntext contains this (plain substring, no regex)\n\n> {}\n\n[Enter] find next   [Esc] cancel",
                input,
            ),
        ),
        Modal::TextView { state } => {
            render_text_view_modal(
                frame,
                area,
                before_src,
                after_src,
                mapping,
                text_solution,
                text_overlay,
                algo_text_spans,
                state,
            );
        }
        Modal::SolutionPicker {
            names,
            selected,
            saving,
            new_name,
            confirm_delete,
            ..
        } => {
            render_solution_picker(
                frame,
                area,
                names,
                *selected,
                text_solution,
                *saving,
                new_name.as_deref(),
                confirm_delete.as_deref(),
                mapping,
            );
        }
        Modal::UnixDiffView { output, scroll } => {
            render_unix_diff_modal(frame, area, output, *scroll);
        }
        Modal::Help { scroll } => {
            render_help_modal(frame, area, *scroll);
        }
        Modal::OpenCommitPicker { commits, selected } => {
            render_open_commit_picker(frame, area, commits, *selected);
        }
        Modal::OpenCommitFilePicker {
            summary,
            files,
            selected,
            ..
        } => {
            render_open_commit_file_picker(frame, area, summary, files, *selected);
        }
    }
}

fn render_text_modal(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    // +2 on each for the block's own top/bottom and left/right borders; the width also leaves a
    // little breathing room (+2 more) so text isn't set flush against the border, and considers
    // the title too, since a title longer than the popup is silently truncated by ratatui.
    let min_height = body.lines().count() as u16 + 2;
    let min_width = body
        .lines()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(0)
        .max(title.chars().count() as u16)
        + 4;
    let popup_area = centered_rect_at_least(60, 30, min_width, min_height, area);
    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    frame.render_widget(
        Paragraph::new(body.to_string())
            .block(block)
            .wrap(Wrap { trim: true }),
        popup_area,
    );
}

/// Every painted span on one side, with the verdict its entry resolves to - the four operations a
/// renderer needs, derived from the three a human paints (see `HumanTextEntry::verdict`).
///
/// A malformed entry is skipped rather than failing the render: the view is how a human would
/// notice and fix it, so refusing to draw would take away the only tool for the job.
fn painted_spans(
    mapping: &HumanMapping,
    solution: &str,
    side: usize,
    before_src: &str,
    after_src: &str,
) -> Vec<(HumanTextSpan, HumanTextVerdict)> {
    solution_entries(mapping, solution)
        .iter()
        .filter_map(|entry| {
            let verdict = entry.verdict(before_src, after_src).ok()?;
            let spans = if side == 0 {
                &entry.before
            } else {
                &entry.after
            };
            Some(spans.iter().map(move |span| (*span, verdict)))
        })
        .flatten()
        .collect()
}

/// The four operation colours, taken from the shared overlay palette rather than hardcoded - so a
/// painted range here looks exactly like the same range does in the `codediff` TUI.
fn verdict_style(verdict: HumanTextVerdict) -> Style {
    let palette = overlay_palette();
    let color = match verdict {
        HumanTextVerdict::Move => palette.move_bg,
        HumanTextVerdict::Update => palette.update_bg,
        HumanTextVerdict::Delete => palette.delete_bg,
        HumanTextVerdict::Insert => palette.insert_bg,
    };
    Style::default().bg(color).fg(palette.overlay_fg)
}

/// What one byte of a row should be drawn as. Ordered so the highest-precedence class wins a
/// simple `max`: the cursor must stay findable on top of a selection, and a selection on top of
/// whatever is already painted underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PaintClass {
    Plain,
    Painted(HumanTextVerdict),
    /// A range banked with `x`, waiting to be committed. Ranked below the live selection so the
    /// one being edited right now stays the one that stands out.
    Banked,
    Selected,
    Cursor,
}

/// Renders one side's source as styled lines, with painted spans, the active selection and the
/// cursor drawn on top of each other in that order.
///
/// Built byte-class-first rather than by splitting on span boundaries: spans, selection and cursor
/// overlap freely, and resolving that as a per-byte precedence is the only version that stays
/// correct when they do. Iterating `char_indices` then groups the classes back into runs, so a
/// multi-byte character is styled as one unit and never split.
fn render_paint_side(
    source: &str,
    spans: &[(HumanTextSpan, HumanTextVerdict)],
    state: &TextPaintState,
    side: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let selection = state.selection(side, source);
    let banked = &state.pending[side];
    let (cursor_row, cursor_column) = state.cursor[side];
    let focused = state.side == side;
    let top = state.scroll[side];

    let lines: Vec<&str> = source.split('\n').collect();
    let gutter_width = lines.len().to_string().len().max(3);

    // Bucketed by row once, rather than scanning every span for every byte of every visible row.
    // A file with a few hundred painted ranges made that inner loop the render's whole cost.
    let mut spans_by_row: HashMap<usize, Vec<&(HumanTextSpan, HumanTextVerdict)>> = HashMap::new();
    for entry in spans {
        for row in entry.0.start_row..=entry.0.end_row {
            spans_by_row.entry(row).or_default().push(entry);
        }
    }
    let no_spans: Vec<&(HumanTextSpan, HumanTextVerdict)> = Vec::new();

    let class_at = |row: usize, column: usize, line: &str| -> PaintClass {
        let mut class = PaintClass::Plain;
        for (span, verdict) in spans_by_row.get(&row).unwrap_or(&no_spans) {
            if span_covers(*span, row, column, line.len()) {
                class = class.max(PaintClass::Painted(*verdict));
            }
        }
        for span in banked {
            if span_covers(*span, row, column, line.len()) {
                class = class.max(PaintClass::Banked);
            }
        }
        if let Some(selection) = selection {
            if span_covers(selection, row, column, line.len()) {
                class = class.max(PaintClass::Selected);
            }
        }
        if focused && row == cursor_row && column == cursor_column {
            class = PaintClass::Cursor;
        }
        class
    };

    let mut out = Vec::with_capacity(height);
    for (row, line) in lines
        .iter()
        .enumerate()
        .take((top + height).min(lines.len()))
        .skip(top)
    {
        let line = *line;
        let mut spans_out = vec![Span::styled(
            format!("{:>width$} ", row + 1, width = gutter_width),
            Style::default().fg(Color::DarkGray),
        )];

        let mut run = String::new();
        let mut run_class: Option<PaintClass> = None;
        let push_run =
            |run: &mut String, class: Option<PaintClass>, out: &mut Vec<Span<'static>>| {
                if run.is_empty() {
                    return;
                }
                out.push(Span::styled(
                    std::mem::take(run),
                    class.map(paint_class_style).unwrap_or_default(),
                ));
            };

        for (offset, ch) in line.char_indices() {
            let class = class_at(row, offset, line);
            if run_class != Some(class) {
                push_run(&mut run, run_class, &mut spans_out);
                run_class = Some(class);
            }
            run.push(ch);
        }
        push_run(&mut run, run_class, &mut spans_out);

        // An empty row still needs to show a cursor or a selection sitting on it, which no
        // character run can carry - draw one space for it.
        let end_class = class_at(row, line.len(), line);
        if end_class != PaintClass::Plain {
            spans_out.push(Span::styled(" ".to_string(), paint_class_style(end_class)));
        }

        out.push(Line::from(spans_out));
    }
    out
}

fn paint_class_style(class: PaintClass) -> Style {
    match class {
        PaintClass::Plain => Style::default(),
        PaintClass::Painted(verdict) => verdict_style(verdict),
        // Dimmer than the live selection, and the same hue: banked and selected are the same kind
        // of thing at different stages, not two unrelated states.
        PaintClass::Banked => Style::default()
            .bg(overlay_palette().cross_highlight_bg)
            .add_modifier(Modifier::DIM),
        // The same colour the TUI paints a cursor's counterpart with: both mean "this is the
        // region you are pointing at", one live and one committed.
        PaintClass::Selected => {
            let palette = overlay_palette();
            Style::default()
                .bg(palette.cross_highlight_bg)
                .fg(palette.overlay_fg)
        }
        PaintClass::Cursor => Style::default().add_modifier(Modifier::REVERSED),
    }
}

/// Whether `span` covers `(row, column)`, with `row_len` used to decide whether a span that ends
/// on a later row runs to the end of this one.
fn span_covers(span: HumanTextSpan, row: usize, column: usize, row_len: usize) -> bool {
    if row < span.start_row || row > span.end_row {
        return false;
    }
    let start = if row == span.start_row {
        span.start_column
    } else {
        0
    };
    let end = if row == span.end_row {
        span.end_column
    } else {
        // Past the last character, so a multi-row span also covers this row's newline.
        row_len + 1
    };
    column >= start && column < end
}

#[allow(clippy::too_many_arguments)]
/// Renders the `t` text-painting modal: both sides' source, side by side, with the human's painted
/// ranges on top and an independent cursor, selection and scroll per side.
fn render_text_view_modal(
    frame: &mut Frame,
    area: Rect,
    before_src: &str,
    after_src: &str,
    mapping: &HumanMapping,
    solution: &str,
    overlay: TextOverlay,
    algo_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    state: &TextPaintState,
) {
    let popup_area = centered_rect(96, 92, area);
    frame.render_widget(Clear, popup_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(popup_area);

    // Two rows go to the block's own borders.
    let height = popup_area.height.saturating_sub(2) as usize;

    let painted = solution_entries(mapping, solution).len();
    let others = mapping.text_mappings.len().saturating_sub(1);

    // Built once for both panels: the disagreement overlay needs each side's human *and* algo
    // spans together, so it can't be derived per panel inside the loop below.
    let human_spans = [
        painted_spans(mapping, solution, 0, before_src, after_src),
        painted_spans(mapping, solution, 1, before_src, after_src),
    ];
    let empty = [Vec::new(), Vec::new()];
    let algo = algo_spans.unwrap_or(&empty);
    let shown = match overlay {
        TextOverlay::Human => human_spans,
        TextOverlay::CodeDiff => algo.clone(),
        TextOverlay::Disagreements => {
            overlay_disagreement_spans(&human_spans, algo, before_src, after_src)
        }
    };

    for (side, source, title) in [
        (0usize, before_src, {
            let pending =
                state.committable(0, before_src).len() + state.committable(1, after_src).len();
            let banked = if pending > 0 {
                format!(
                    " — {}:{} pending",
                    state.committable(0, before_src).len(),
                    state.committable(1, after_src).len()
                )
            } else {
                String::new()
            };
            format!(
                "Before [{solution}] {painted} painted{banked} — showing {} (p cycles)",
                overlay.label()
            )
        }),
        (
            1usize,
            after_src,
            match (&state.line_prompt, state.side) {
                (Some(typed), 1) => format!("After — jump to line: {typed}_"),
                _ if others > 0 => {
                    format!("After — s save-as, L load ({others} other) — u/Tab/Esc")
                }
                _ => "After — v sel/i ins/u unmark, s save-as, : jump, Tab, Esc".to_string(),
            },
        ),
    ] {
        let lines = render_paint_side(source, &shown[side], state, side, height);
        let border_style = if state.side == side {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            ),
            columns[side],
        );
    }
}

#[allow(clippy::too_many_arguments)]
/// Renders the solution picker raised by `s`/`L` inside the text view: which named painting to
/// save the current ranges under, or to switch to editing.
fn render_solution_picker(
    frame: &mut Frame,
    area: Rect,
    names: &[String],
    selected: usize,
    current: &str,
    saving: bool,
    new_name: Option<&str>,
    confirm_delete: Option<&str>,
    mapping: &HumanMapping,
) {
    let popup_area = centered_rect(56, 50, area);
    frame.render_widget(Clear, popup_area);

    let mut items: Vec<ListItem> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let count = solution_entries(mapping, name).len();
            let exists = mapping
                .text_mappings
                .iter()
                .any(|named| named.name == *name);
            let label = if !exists {
                format!("{name}  (new)")
            } else if name == current {
                format!("{name}  ({count} range(s), editing now)")
            } else {
                format!("{name}  ({count} range(s))")
            };
            let style = if confirm_delete == Some(name.as_str()) {
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if index == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else if name == current {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    // The free-form entry always sits last, so its index is `names.len()` - the one position the
    // key handler treats specially.
    let typing = new_name.is_some();
    let free_form = new_name.unwrap_or("");
    let free_label = if typing {
        format!("New name: {free_form}_")
    } else {
        "New name...".to_string()
    };
    items.push(ListItem::new(Line::from(Span::styled(
        free_label,
        if selected == names.len() {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else {
            Style::default()
        },
    ))));

    let title = if let Some(doomed) = confirm_delete {
        format!("Delete '{doomed}'? D again confirms, any other key cancels")
    } else if typing {
        "Type a name — Enter confirm, Esc back".to_string()
    } else if saving {
        format!("Branch '{current}' to — Enter copy, e empty, D delete, Esc cancel")
    } else {
        "Switch to painting — j/k, Enter, D delete, Esc cancel".to_string()
    };

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(
                    Style::default()
                        .fg(if confirm_delete.is_some() {
                            Color::Red
                        } else {
                            Color::Yellow
                        })
                        .add_modifier(Modifier::BOLD),
                ),
        ),
        popup_area,
    );
}

/// Renders the `T` (unix diff) modal: the already-computed output of `diff -u` between the before
/// and after content, with `+`/`-` lines colored to match the rest of the UI's insert/delete
/// convention and `@@` hunk headers highlighted.
fn render_unix_diff_modal(frame: &mut Frame, area: Rect, output: &str, scroll: u16) {
    let popup_area = centered_rect(92, 90, area);
    frame.render_widget(Clear, popup_area);

    // `diff -u` reports positions only in its `@@ -a,b +c,d @@` headers, so a reader counting to
    // the line a hunk mentions has to do it by hand. Tracking the two counters across the hunk and
    // printing them per row turns that into reading. A deleted line has no after-side number and
    // an inserted one has no before-side number, which the blank half says directly.
    let mut before_line = 0usize;
    let mut after_line = 0usize;
    let lines: Vec<Line> = output
        .lines()
        .map(|line| {
            let (style, gutter) = if line.starts_with("+++") || line.starts_with("---") {
                (Style::default().add_modifier(Modifier::BOLD), String::new())
            } else if let Some(rest) = line.strip_prefix("@@") {
                (Style::default().fg(Color::Cyan), {
                    // `@@ -a,b +c,d @@` - the two starting positions, which reset both counters.
                    let mut numbers = rest.split_whitespace();
                    for (target, sign) in [(&mut before_line, '-'), (&mut after_line, '+')] {
                        if let Some(start) = numbers.next().and_then(|token| {
                            token
                                .strip_prefix(sign)?
                                .split(',')
                                .next()?
                                .parse::<usize>()
                                .ok()
                        }) {
                            *target = start;
                        }
                    }
                    String::new()
                })
            } else if line.starts_with('+') {
                let g = format!("{:>6} {:>6} ", "", after_line);
                after_line += 1;
                (Style::default().fg(Color::Green), g)
            } else if line.starts_with('-') {
                let g = format!("{:>6} {:>6} ", before_line, "");
                before_line += 1;
                (Style::default().fg(Color::Red), g)
            } else {
                let g = format!("{:>6} {:>6} ", before_line, after_line);
                before_line += 1;
                after_line += 1;
                (Style::default(), g)
            };
            Line::from(vec![
                Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                Span::styled(line.to_string(), style),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("unix `diff -u` — before/after line numbers — j/k scroll, t text view, Esc close")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup_area,
    );
}

/// Renders the `?` help modal: a static reference sheet of every keybinding (`HELP_TEXT`).
fn render_help_modal(frame: &mut Frame, area: Rect, scroll: u16) {
    let popup_area = centered_rect(90, 90, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Keybindings — j/k scroll, ? or Esc to close")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(
        Paragraph::new(HELP_TEXT).block(block).scroll((scroll, 0)),
        popup_area,
    );
}

/// Renders the `o`/`O` (open) pickers: a scrollable list of names (test cases for `o`, samples
/// for `O`, per `kind`), with `selected` highlighted. Scroll position is recomputed fresh each
/// frame from `selected` (no persisted state needed) by roughly centering it in the viewport,
/// clamped to the list's extent.
/// The `o` picker. Like `render_open_sample_picker`, the dataset-filtered view
/// (`visible_diff_options`) is recomputed here from `options`/`dataset_filter` rather than
/// carried on the modal itself, so the two can never drift out of sync.
#[allow(clippy::too_many_arguments)]
fn render_open_diff_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[(String, &'static str)],
    selected: usize,
    dataset_filter: Option<&'static str>,
    hide_complete: bool,
    completeness: Option<&std::collections::HashMap<String, bool>>,
    hide_painted: bool,
    text_painted: Option<&std::collections::HashMap<String, bool>>,
    comments: Option<&std::collections::HashMap<String, String>>,
) {
    let visible = visible_diff_options(
        options,
        dataset_filter,
        hide_complete,
        completeness,
        hide_painted,
        text_painted,
    );

    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);

    // The note of whatever row is selected gets its own strip along the bottom. A note is
    // free-form prose and the names here already run to sixty-odd characters, so there is no room
    // to show one inline - the list carries a marker saying a note exists, and this says what it
    // is for the one row the reader is actually on.
    let note = comments.and_then(|map| visible.get(selected).and_then(|name| map.get(name)));
    let (list_area, note_area) = if note.is_some() {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(4)])
            .split(popup_area);
        (split[0], Some(split[1]))
    } else {
        (popup_area, None)
    };

    // Derived from `list_area`, not `popup_area`: the footer takes rows away from the list, and
    // scrolling computed against the full popup would push the selected row off the bottom by
    // exactly the footer's height.
    let inner_height = list_area.height.saturating_sub(2) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, name)| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            // Marked rather than shown: which cases carry a note is the part worth seeing at a
            // glance, and it is also the part that survives a narrow terminal.
            let marked = match comments {
                Some(map) if map.contains_key(name) => format!("* {name}"),
                Some(_) => format!("  {name}"),
                None => name.clone(),
            };
            ListItem::new(Line::from(Span::styled(marked, style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open diff [{}]{}{} ({}/{}) — j/k, d dataset, H incomplete-only, X unpainted-only, Enter, Esc",
            dataset_filter.unwrap_or("all"),
            if hide_complete { " [incomplete only]" } else { "" },
            if hide_painted { " [unpainted only]" } else { "" },
            selected + 1,
            visible.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), list_area);

    if let (Some(note_area), Some(note)) = (note_area, note) {
        frame.render_widget(
            Paragraph::new(note.as_str())
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("description.md — e to edit")
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            note_area,
        );
    }
}

/// Like `render_open_diff_picker`, but for `O`'s sample picker: handled (already-promoted or
/// -rejected) entries are shown in green (" - SOLVED") or red (" - REJECTED"), both left out of
/// the list entirely when `hide_solved` is set, and ordered per `sort_order` (cycled by `s` - see
/// `SampleSortOrder`). Each entry also shows its `sample_diff_line_count` in parentheses, so the
/// effect of switching to a diff-size order is visible directly, not just trusted.
fn render_open_sample_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[(String, SampleTriageStatus, usize)],
    selected: usize,
    hide_solved: bool,
    sort_order: SampleSortOrder,
) {
    let visible = visible_sample_options(options, hide_solved, sort_order);

    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, (name, status, size))| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                match status {
                    SampleTriageStatus::Promoted => Style::default().fg(Color::Green),
                    SampleTriageStatus::Rejected => Style::default().fg(Color::Red),
                    SampleTriageStatus::Sampled => Style::default(),
                }
            };
            let suffix = match status {
                SampleTriageStatus::Promoted => " - SOLVED",
                SampleTriageStatus::Rejected => " - REJECTED",
                SampleTriageStatus::Sampled => "",
            };
            let label = format!("{name} ({size}){suffix}");
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let handled_count = options
        .iter()
        .filter(|(_, status, _)| status.is_handled())
        .count();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open sample ({}/{}) — j/k move, Enter open, H {} solved/rejected ({} total), s sort: {}, Esc cancel",
            if visible.is_empty() { 0 } else { selected + 1 },
            visible.len(),
            if hide_solved { "show" } else { "hide" },
            handled_count,
            sort_order.label(),
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

/// Renders the `C` picker's first step: pick a commit from this repository's own `git log`
/// (`list_repo_commits`'s `(hash, summary)` pairs, newest first).
fn render_open_commit_picker(
    frame: &mut Frame,
    area: Rect,
    commits: &[(String, String)],
    selected: usize,
) {
    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = commits.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = commits
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, (hash, summary))| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{} {}", short_hash(hash), summary),
                style,
            )))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open commit ({}/{}) — j/k move, Enter pick a file it changed, Esc cancel",
            if commits.is_empty() { 0 } else { selected + 1 },
            commits.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

/// Renders the `C` picker's second step: pick which of `summary`'s changed files (only ones with
/// a supported language - see `list_commit_files`) to open.
fn render_open_commit_file_picker(
    frame: &mut Frame,
    area: Rect,
    summary: &str,
    files: &[String],
    selected: usize,
) {
    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = files.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, path)| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} ({}/{}) — j/k move, Enter open, Esc cancel",
            summary,
            if files.is_empty() { 0 } else { selected + 1 },
            files.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

// ---------------------------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------------------------

/// Everything derived from the current trees, mapping, and collapse/hide state that drawing a
/// frame or interpreting a keystroke needs. Rebuilding this is the expensive part of the loop (a
/// handful of whole-tree passes); see `run_event_loop`'s `needs_redraw` for why it only happens
/// once per keystroke rather than on every idle poll timeout.
struct FrameState<'a> {
    before_root: Node<'a>,
    after_root: Node<'a>,
    before_src: &'a [u8],
    after_src: &'a [u8],
    caches: Caches,
    before_flat: FlatIndex<'a>,
    after_flat: FlatIndex<'a>,
    /// Counts of `Unmarked` nodes in `before_flat`/`after_flat`, for `render_panel`'s "N unmarked"
    /// header. Computed once here rather than by scanning all of `flat` on every single draw call
    /// (see `render_panel`), since on a large case that scan -- calling `status_before`/
    /// `status_after` on every node, not just the visible ones -- was itself a real cost paid every
    /// frame for no reason: it only changes when this `FrameState` does.
    before_unmarked: usize,
    after_unmarked: usize,
}

/// The number of `flat`'s nodes with `NodeStatus::Unmarked`, per `status_fn`.
fn count_unmarked(
    flat: &[(Node, usize)],
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> usize {
    flat.iter()
        .filter(|(n, _)| status_fn(*n, caches) == NodeStatus::Unmarked)
        .count()
}

fn compute_frame_state<'a>(before: &'a Code, after: &'a Code, app: &App) -> Result<FrameState<'a>> {
    let before_root = before
        .ast
        .as_ref()
        .context("Before code has no AST")?
        .root_node();
    let after_root = after
        .ast
        .as_ref()
        .context("After code has no AST")?
        .root_node();
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);

    // Recomputed fresh whenever frame state is rebuilt, so `H` can't show a subtree as hidden
    // after it's actually been un-marked, or vice versa.
    let before_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(before_root, &caches, status_before));
    let after_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(after_root, &caches, status_after));

    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &app.before.collapsed,
        before_hidden.as_ref(),
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &app.after.collapsed,
        after_hidden.as_ref(),
    ));

    let before_unmarked = count_unmarked(&before_flat, &caches, status_before);
    let after_unmarked = count_unmarked(&after_flat, &caches, status_after);

    Ok(FrameState {
        before_root,
        after_root,
        before_src,
        after_src,
        caches,
        before_flat,
        after_flat,
        before_unmarked,
        after_unmarked,
    })
}

/// What a case session (`run_case_session`) ended on: either the user quit, or a modal asked to
/// switch to a different case (`o`/`O`'s pickers, or a discard-unsaved confirmation).
enum SessionEnd {
    Quit,
    Open(OpenTarget),
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut before: Code,
    mut after: Code,
) -> Result<()> {
    loop {
        // `run_case_session` only ever reads `before`/`after` (never reassigns them), so it's free
        // to cache state that borrows from them for as long as the whole session runs, with none of
        // the self-referential-across-a-mutation problem that caching across *this* loop's own
        // iterations would run into: those iterations are exactly the ones that reassign `before`/
        // `after` below.
        match run_case_session(terminal, app, &before, &after)? {
            SessionEnd::Quit => break,
            SessionEnd::Open(OpenTarget::Diffs(name)) => match load_case(&name) {
                Ok((new_before, new_after)) => {
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
                    before = new_before;
                    after = new_after;
                    app.mapping = human_mapping::load(&name).unwrap_or_default();
                    app.name = name;
                    app.origin = CaseOrigin::Diffs;
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                    app.status = Some(format!("Opened '{}'", app.name));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening '{}': {:#}", name, err));
                }
            },
            SessionEnd::Open(OpenTarget::Sample(name)) => match load_sample(&name) {
                Ok((new_before, new_after, source)) => {
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
                    before = new_before;
                    after = new_after;
                    app.mapping = HumanMapping::default();
                    app.name = name;
                    app.origin = CaseOrigin::Sample(source);
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                    app.status = Some(format!(
                        "Opened sample '{}' (press s to promote it into a test case)",
                        app.name
                    ));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening sample '{}': {:#}", name, err));
                }
            },
            SessionEnd::Open(OpenTarget::GitCommitFile {
                hash,
                summary,
                path,
            }) => match load_git_commit_file(&hash, &path) {
                Ok((new_before, new_after)) => {
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
                    before = new_before;
                    after = new_after;
                    app.mapping = HumanMapping::default();
                    app.name = format!("{path}@{}", short_hash(&hash));
                    app.status = Some(format!(
                        "Opened '{}' from commit {} \"{}\" (press s to promote it into a test \
                         case)",
                        path,
                        short_hash(&hash),
                        summary
                    ));
                    app.origin = CaseOrigin::GitCommitFile { path };
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                }
                Err(err) => {
                    app.status = Some(format!(
                        "Error opening '{}' from commit {}: {:#}",
                        path,
                        short_hash(&hash),
                        err
                    ));
                }
            },
        }
    }
    Ok(())
}

/// Keys `handle_key` (only reachable when no modal is open -- see `is_state_preserving_key`)
/// processes without touching `App::mapping`, either panel's `collapsed` set, or
/// `App::hide_solved`: the three things `run_case_session`'s cached `FrameState` depends on. Pure
/// cursor movement (`j`/`k`/arrows/`Tab`/`g`/`G`/`n`/`N`), the multi-map selection (`x`/`c`, read
/// directly off `App` by `render_panel`, not through `FrameState`), and view/display toggles
/// (`p`/`r`/`t`/`T`/`/`/`?`) whose own state (`algo_diff`, `show_reason`) is likewise read
/// straight off `App`. On a large case, rebuilding `FrameState` for every one of these -- which is
/// what browsing a case mostly consists of -- used to mean paying `rebuild_caches_for_mapping`
/// (documented up to ~2s on a heavily-annotated fixture), two `fully_solved_nodes` walks, and two
/// `flatten_visible` walks on every single keystroke, whether or not anything `FrameState` derives
/// from had actually changed.
///
/// Deliberately conservative: `h`/`l`/`a`/`A` (which sometimes mutate a `collapsed` set, depending
/// on where the cursor already is) and `s`/`R`/`o`/`O`/`C` (which open a modal or save, and are
/// rare enough that the existing full-rebuild cost isn't worth the extra classification surface)
/// are NOT included here, even though some of their branches don't actually need a rebuild either
/// -- see `handle_key` for the exact effect of every key this list omits.
fn is_navigation_or_display_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Char('q')
            | KeyCode::Esc
            | KeyCode::Char('?')
            | KeyCode::Tab
            | KeyCode::Up
            | KeyCode::Char('k')
            | KeyCode::Down
            | KeyCode::Char('j')
            | KeyCode::Char('g')
            | KeyCode::Char('G')
            | KeyCode::Char('x')
            | KeyCode::Char('c')
            | KeyCode::Char('p')
            | KeyCode::Char('n')
            | KeyCode::Char('N')
            | KeyCode::Char('/')
            | KeyCode::Char('t')
            | KeyCode::Char('T')
            | KeyCode::Char('r')
    )
}

/// Whether `code`, delivered in the current `modal` state, is guaranteed not to touch the mapping,
/// either panel's collapsed set, or `hide_solved` -- the three things `run_case_session`'s cached
/// `FrameState` depends on.
///
/// With no modal open, this is `is_navigation_or_display_key` (`handle_key`'s own pure-navigation
/// keys). With a modal open, only typing into (or backspacing out of) `PromptSearch`'s,
/// `PromptPromoteName`'s, `PromptRejectReason`'s, or `PromptComment`'s own `input` string
/// qualifies: those modals only ever mutate that string in response to these keys, everything else
/// about the case is untouched. Every other key while a modal is open -- including Enter/Esc on
/// these same four modals, which can search-and-move-the-cursor, promote/reject/comment/save, or
/// close the modal -- is treated conservatively as "might have changed something", so the cache is
/// thrown away and rebuilt fresh, exactly as if this function didn't exist.
fn is_state_preserving_key(modal: Option<&Modal>, code: KeyCode) -> bool {
    match modal {
        None => is_navigation_or_display_key(code),
        Some(Modal::PromptSearch { .. })
        | Some(Modal::PromptPromoteName { .. })
        | Some(Modal::PromptRejectReason { .. })
        | Some(Modal::PromptComment { .. }) => {
            matches!(code, KeyCode::Char(_) | KeyCode::Backspace)
        }
        // The text-painting views cannot invalidate the cached `FrameState`, so *every* key in
        // them preserves it - including the ones that write.
        //
        // `FrameState` caches the flattened trees and the `Caches` built from them, and
        // `rebuild_caches_for_mapping` reads only `entries`/`groups`. Painting writes
        // `text_mappings`, a separate ground truth that no tree state is derived from (see
        // `HumanTextMapping`), so nothing the `t` view does can make the cache wrong.
        //
        // This is a correctness observation with a large performance consequence. Falling through
        // to `false` re-flattened both ASTs and rebuilt every cache on each cursor keystroke; on a
        // ~900 KB fixture that is a full walk of a few hundred thousand nodes per keypress, which
        // is what made the view unusable on big files. The painting view is exactly where a reader
        // holds down `j`.
        Some(Modal::TextView { .. })
        | Some(Modal::SolutionPicker { .. })
        | Some(Modal::UnixDiffView { .. }) => true,
        Some(_) => false,
    }
}

/// Runs the event loop for a single case (before/after AST pair) until the user quits or asks to
/// switch to a different one. Split out from `run_event_loop` specifically so the `state` cache
/// below -- which borrows from `before`/`after` -- never has to coexist with a reassignment of
/// them: `before`/`after` are `&Code` here, immutable for this whole call, so the cache is free to
/// survive across as many keystrokes as it likes with no lifetime conflict. A case switch is
/// reported back to the caller as a `SessionEnd::Open` instead of being handled in place.
fn run_case_session(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    before: &Code,
    after: &Code,
) -> Result<SessionEnd> {
    // Whether the on-screen frame reflects the current state. Set whenever something might have
    // changed (a key was handled, the terminal was resized) and cleared right after redrawing. On
    // a pure idle poll timeout -- the common case, since the TUI just sits there most of the time
    // -- nothing is redrawn at all.
    let mut needs_redraw = true;

    // Cached result of `compute_frame_state` -- rebuilding both caches and re-flattening both
    // (possibly multi-thousand-node) trees is real work, so it's only redone when something that
    // could actually change it happens, not on every idle tick or every keystroke. `None` forces a
    // fresh (potentially expensive) recompute; set back to `None` only by a key that could
    // plausibly touch the mapping, a collapsed set, or `hide_solved` -- see
    // `is_state_preserving_key`/`is_navigation_or_display_key` for the exact set that's exempted.
    // That set is deliberately generous: browsing a case (moving the cursor, jumping between
    // mismatches, toggling `p`/`r`/`t`/`T` display) is the overwhelming majority of keys pressed in
    // a session, and none of it needs a rebuild, so those keys reuse this `FrameState` untouched.
    // Only the keys that actually mutate the mapping or a collapsed set (`m`/`M`/`f`/`d`/`D`/`i`/
    // `I`/`u`/`h`/`l`/`a`/`A`/`H`, plus text-modal Enter/Esc) still pay the full recompute.
    let mut state: Option<FrameState> = None;

    loop {
        if state.is_none() {
            state = Some(compute_frame_state(before, after, app)?);
        }
        let frame_state = state.as_ref().expect("just populated above if empty");

        if needs_redraw {
            // Cloned rather than borrowed from `app`: draw_ui also takes `app: &mut App`, and
            // passing both `app` and `&app.name` as separate arguments to the same call would
            // conflict.
            let current_name = app.name.clone();
            terminal.draw(|f| {
                draw_ui(
                    f,
                    app,
                    &frame_state.before_flat,
                    &frame_state.after_flat,
                    &frame_state.caches,
                    frame_state.before_src,
                    frame_state.after_src,
                    frame_state.before_unmarked,
                    frame_state.after_unmarked,
                    &current_name,
                )
            })?;
            needs_redraw = false;
        }

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let event = event::read()?;
        let Event::Key(key) = event else {
            // e.g. a resize: nothing to recompute, just redraw at the (possibly new) size.
            needs_redraw = true;
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // `load_case` runs `ensure_parsed` on both sides, so full-content hashes are always
        // available here; used by `m`/`M` to decide Identical vs MatchButNotIdentical for nodes
        // with children without asking (see `subtree_match_operation`).
        let before_hash = &before
            .metadata
            .ast_metadata
            .as_ref()
            .context("Before code has no AST metadata")?
            .node_to_full_hash;
        let after_hash = &after
            .metadata
            .ast_metadata
            .as_ref()
            .context("After code has no AST metadata")?
            .node_to_full_hash;

        let state_preserving = is_state_preserving_key(app.modal.as_ref(), key.code);

        let mut open_request: Option<OpenTarget> = None;

        if app.modal.is_some() {
            open_request = handle_modal_key(
                app,
                key.code,
                &frame_state.before_flat,
                &frame_state.after_flat,
                frame_state.before_root,
                frame_state.after_root,
                &frame_state.caches,
                frame_state.before_src,
                frame_state.after_src,
                before,
                after,
            );
        } else {
            handle_key(
                app,
                key.code,
                &frame_state.before_flat,
                &frame_state.after_flat,
                frame_state.before_root,
                frame_state.after_root,
                &frame_state.caches,
                frame_state.before_src,
                frame_state.after_src,
                before_hash,
                after_hash,
                before,
                after,
            );
        }

        needs_redraw = true;
        if !state_preserving {
            state = None;
        }

        if let Some(target) = open_request {
            return Ok(SessionEnd::Open(target));
        }

        if app.should_quit {
            return Ok(SessionEnd::Quit);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    before: &Code,
    after: &Code,
) {
    let focus = app.focus;

    let result: Option<Result<String>> = match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('?') => {
            app.modal = Some(Modal::Help { scroll: 0 });
            None
        }
        KeyCode::Tab => {
            app.focus = focus.toggle();
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            move_cursor(panel, flat, -1);
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            move_cursor(panel, flat, 1);
            None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            collapse_or_ascend(panel, flat);
            None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            expand_or_descend(panel, flat);
            None
        }
        KeyCode::Char('g') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            jump_to_edge(panel, flat, true);
            None
        }
        KeyCode::Char('G') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            jump_to_edge(panel, flat, false);
            None
        }
        KeyCode::Char('m') => {
            let outcome = if app.before_multi_select.is_empty() && app.after_multi_select.is_empty()
            {
                action_match(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    app.before.cursor_id,
                    app.after.cursor_id,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_hash,
                    after_hash,
                )
            } else {
                action_commit_multi_map_group(
                    &mut app.mapping,
                    before_root,
                    after_root,
                    &app.before_multi_select,
                    &app.after_multi_select,
                    before_hash,
                    after_hash,
                    caches,
                    false,
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('f') => {
            match action_match_to_end(
                app,
                before_flat,
                after_flat,
                before_root,
                after_root,
                before_src,
                after_src,
                before_hash,
                after_hash,
            ) {
                Ok(ActionOutcome::Done(msg)) => app.status = Some(msg),
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('M') => {
            let outcome = if app.before_multi_select.is_empty() && app.after_multi_select.is_empty()
            {
                action_match_subtree(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    app.before.cursor_id,
                    app.after.cursor_id,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_hash,
                    after_hash,
                    &mut app.before.collapsed,
                    &mut app.after.collapsed,
                )
            } else {
                action_commit_multi_map_group(
                    &mut app.mapping,
                    before_root,
                    after_root,
                    &app.before_multi_select,
                    &app.after_multi_select,
                    before_hash,
                    after_hash,
                    caches,
                    true,
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if focus != Focus::Before {
                Some(Err(anyhow!(
                    "d/D only apply to the Before panel; press Tab to switch"
                )))
            } else {
                let res = action_delete(
                    &mut app.mapping,
                    before_flat,
                    app.before.cursor_id,
                    before_root,
                    after_root,
                    code == KeyCode::Char('D'),
                    caches,
                );
                if res.is_ok() {
                    app.dirty = true;
                    advance_before_to_next_unmarked(app, before_flat, before_root, after_root);
                }
                Some(res)
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if focus != Focus::After {
                Some(Err(anyhow!(
                    "i/I only apply to the After panel; press Tab to switch"
                )))
            } else {
                let res = action_insert(
                    &mut app.mapping,
                    after_flat,
                    app.after.cursor_id,
                    before_root,
                    after_root,
                    code == KeyCode::Char('I'),
                    caches,
                );
                if res.is_ok() {
                    app.dirty = true;
                    advance_after_to_next_unmarked(app, after_flat, before_root, after_root);
                }
                Some(res)
            }
        }
        KeyCode::Char('a') => Some(action_align(app, focus, before_root, after_root, caches)),
        KeyCode::Char('A') => Some(action_align_algo(app, focus, before_root, after_root)),
        KeyCode::Char('u') => {
            let res = action_unmark(
                &mut app.mapping,
                focus,
                before_flat,
                after_flat,
                app.before.cursor_id,
                app.after.cursor_id,
                before_root,
                after_root,
                caches,
            );
            if res.is_ok() {
                app.dirty = true;
            }
            Some(res)
        }
        KeyCode::Char('x') => {
            let (cursor_id, selected) = match focus {
                Focus::Before => (app.before.cursor_id, &mut app.before_multi_select),
                Focus::After => (app.after.cursor_id, &mut app.after_multi_select),
            };
            if !selected.remove(&cursor_id) {
                selected.insert(cursor_id);
            }
            app.status = Some(format!(
                "Multi-map selection: {} before, {} after node(s) (m/M to commit as a group, c to clear)",
                app.before_multi_select.len(),
                app.after_multi_select.len()
            ));
            None
        }
        KeyCode::Char('c') => {
            app.before_multi_select.clear();
            app.after_multi_select.clear();
            app.status = Some("Cleared multi-map selection".to_string());
            None
        }
        KeyCode::Char('p') => {
            let diff = diff_code(before, after);
            app.status = Some(match diff.ast {
                Some(ast_diff) => {
                    let msg = format!(
                        "Ran codediff: {} before-node(s), {} after-node(s) mapped",
                        ast_diff.before_node_map.len(),
                        ast_diff.after_node_map.len()
                    );
                    app.algo_diff = Some(ast_diff);
                    msg
                }
                None => "codediff produced no AST diff".to_string(),
            });
            None
        }
        KeyCode::Char('n') => Some(action_next_mismatch(
            app,
            focus,
            before_flat,
            after_flat,
            caches,
            true,
        )),
        KeyCode::Char('N') => Some(action_next_mismatch(
            app,
            focus,
            before_flat,
            after_flat,
            caches,
            false,
        )),
        KeyCode::Char('/') => {
            app.modal = Some(Modal::PromptSearch {
                input: app.last_search.clone().unwrap_or_default(),
            });
            None
        }
        KeyCode::Char('t') => {
            app.modal = Some(Modal::TextView {
                state: TextPaintState::default(),
            });
            None
        }
        KeyCode::Char('T') => {
            match run_unix_diff(before_src, after_src) {
                Ok(output) => app.modal = Some(Modal::UnixDiffView { output, scroll: 0 }),
                Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
            }
            None
        }
        KeyCode::Char('H') => {
            app.hide_solved = !app.hide_solved;
            app.status = Some(if app.hide_solved {
                "Hiding fully solved subtrees".to_string()
            } else {
                "Showing all nodes".to_string()
            });
            None
        }
        KeyCode::Char('r') => {
            app.show_reason = !app.show_reason;
            app.status = Some(if app.show_reason {
                "Showing ASTMappingReason next to each node's algo verdict".to_string()
            } else {
                "Hiding ASTMappingReason".to_string()
            });
            None
        }
        KeyCode::Char('s') => match &app.origin {
            CaseOrigin::Diffs => {
                let result = action_save(&mut app.mapping, &mut app.dirty, &app.name, None);
                if result.is_ok() {
                    refresh_diff_completeness(app, &app.name.clone());
                    refresh_diff_text_painted(app, &app.name.clone());
                }
                Some(result)
            }
            CaseOrigin::Sample(source) => {
                app.modal = Some(Modal::PromptPromoteName {
                    input: default_promoted_name(source),
                    error: None,
                });
                None
            }
            CaseOrigin::GitCommitFile { path } => {
                app.modal = Some(Modal::PromptPromoteName {
                    input: default_promoted_name_for_path(path),
                    error: None,
                });
                None
            }
        },
        KeyCode::Char('R') => {
            if matches!(app.origin, CaseOrigin::Sample(_)) {
                app.modal = Some(Modal::PromptRejectReason {
                    input: String::new(),
                    error: None,
                });
            } else {
                app.status = Some("Only an open sample (O) can be rejected".to_string());
            }
            None
        }
        KeyCode::Char('e') => {
            if let CaseOrigin::Diffs = &app.origin {
                // Pre-filled from the note if there is one, and otherwise from the sample.csv
                // comment this fixture was promoted with. That second fallback is what keeps
                // "description.md wins" from stranding anything: the old comment appears in the
                // prompt, and confirming it copies it into the file the inventory now prefers.
                let existing = read_note(&app.name)
                    .or_else(|| promoted_sample_comment(&app.name))
                    .unwrap_or_default();
                app.modal = Some(Modal::PromptComment {
                    input: existing,
                    error: None,
                });
            } else if let CaseOrigin::Sample(source) = &app.origin {
                // Pre-fill with whatever's already recorded, so this is an edit, not a blind
                // overwrite - same idea as `PromptPromoteName`'s pre-filled default name.
                let existing = read_sample_csv_rows(&sample_csv_path())
                    .ok()
                    .and_then(|rows| find_sample_row(&rows, source).map(|row| row.comment.clone()))
                    .unwrap_or_default();
                app.modal = Some(Modal::PromptComment {
                    input: existing,
                    error: None,
                });
            } else {
                app.status =
                    Some("Only an open diff (o) or sample (O) can have a comment".to_string());
            }
            None
        }
        KeyCode::Char('o') => {
            // Loaded here, unlike the completeness and painted maps which wait for the key that
            // filters by them: notes are *displayed*, so they have to be present the first time
            // the list is drawn. Affordable exactly because this scan is the cheap one - a stat
            // and a short read per fixture, and most have no note at all.
            if app.diff_comments.is_none() {
                app.diff_comments = Some(compute_diff_comments());
            }
            match list_available_cases() {
                Ok(options) if !options.is_empty() => {
                    app.modal = Some(open_diff_picker_modal(
                        options,
                        &app.name,
                        app.diff_dataset_filter,
                        app.diff_hide_complete,
                        app.diff_completeness.as_ref(),
                        app.diff_hide_painted,
                        app.diff_text_painted.as_ref(),
                    ));
                }
                Ok(_) => {
                    app.status = Some("No test cases found in src/test/data/diffs".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing cases: {:#}", err));
                }
            }
            None
        }
        KeyCode::Char('O') => {
            match list_samples_with_status() {
                Ok(options) if !options.is_empty() => {
                    let options: Vec<(String, SampleTriageStatus, usize)> = options
                        .into_iter()
                        .map(|(name, status)| {
                            let size = sample_diff_line_count(&name);
                            (name, status, size)
                        })
                        .collect();
                    app.modal = Some(open_sample_picker_modal(
                        options,
                        &app.name,
                        app.sample_hide_solved,
                        app.sample_sort_order,
                    ));
                }
                Ok(_) => {
                    app.status = Some("No samples found in src/test/data/samples".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing samples: {:#}", err));
                }
            }
            None
        }
        KeyCode::Char('C') => {
            match list_repo_commits() {
                Ok(commits) if !commits.is_empty() => {
                    app.modal = Some(Modal::OpenCommitPicker {
                        commits,
                        selected: 0,
                    });
                }
                Ok(_) => {
                    app.status = Some("No commits found in this repository".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing commits: {:#}", err));
                }
            }
            None
        }
        _ => None,
    };

    if let Some(res) = result {
        app.status = Some(match res {
            Ok(msg) => msg,
            Err(err) => format!("Error: {:#}", err),
        });
    }
}

/// Routes a keypress while `app.modal` is `Some`. Returns `Some(name)` when the human just
/// confirmed switching to a different test case (via the open picker, possibly after a save/
/// discard decision): the caller is responsible for actually loading it, since that needs
/// mutable access to the owned `Code` values that `run_event_loop` holds, which can't be threaded
/// down here alongside `Node`s borrowed from them.
#[allow(clippy::too_many_arguments)]
fn handle_modal_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    // Needed only by the text view's `p` (running codediff to show its own rendering), but the
    // modal handler is one function, so it takes the pair the same way `handle_key` does.
    before: &Code,
    after: &Code,
) -> Option<OpenTarget> {
    let modal = app.modal.take()?;

    match modal {
        Modal::ConfirmKindMismatch {
            before_id,
            after_id,
            before_kind,
            after_kind,
            recursive,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.dirty = true;
                app.status = Some(apply_modal_choice(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_id,
                    after_id,
                    HumanOperation::MatchButNotIdentical,
                    recursive,
                    &mut app.before.collapsed,
                    &mut app.after.collapsed,
                ));
                advance_both_to_next_unmarked(
                    app,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                );
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.status = Some("Cancelled: node kinds do not match".to_string());
            }
            _ => {
                app.modal = Some(Modal::ConfirmKindMismatch {
                    before_id,
                    after_id,
                    before_kind,
                    after_kind,
                    recursive,
                });
            }
        },
        Modal::ConfirmMultiMapGroup {
            before_ids,
            after_ids,
            operation,
            with_children,
            kinds,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let before_set: std::collections::BTreeSet<usize> =
                    before_ids.iter().copied().collect();
                let after_set: std::collections::BTreeSet<usize> =
                    after_ids.iter().copied().collect();
                app.status = Some(
                    match commit_multi_map_group(
                        &mut app.mapping,
                        before_root,
                        after_root,
                        &before_set,
                        &after_set,
                        operation,
                        with_children,
                    ) {
                        Ok(msg) => {
                            app.dirty = true;
                            msg
                        }
                        Err(err) => format!("Error: {:#}", err),
                    },
                );
                app.before_multi_select.clear();
                app.after_multi_select.clear();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.status = Some("Cancelled: multi-map group has mixed node kinds".to_string());
                app.before_multi_select.clear();
                app.after_multi_select.clear();
            }
            _ => {
                app.modal = Some(Modal::ConfirmMultiMapGroup {
                    before_ids,
                    after_ids,
                    operation,
                    with_children,
                    kinds,
                });
            }
        },
        Modal::OpenDiffPicker {
            options,
            selected,
            dataset_filter,
            hide_complete,
            hide_painted,
        } => {
            let visible = visible_diff_options(
                &options,
                dataset_filter,
                hide_complete,
                app.diff_completeness.as_ref(),
                hide_painted,
                app.diff_text_painted.as_ref(),
            );
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    app.modal = Some(Modal::OpenDiffPicker {
                        selected: selected.saturating_sub(1),
                        options,
                        dataset_filter,
                        hide_complete,
                        hide_painted,
                    });
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.modal = Some(Modal::OpenDiffPicker {
                        selected: (selected + 1).min(visible.len().saturating_sub(1)),
                        options,
                        dataset_filter,
                        hide_complete,
                        hide_painted,
                    });
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    let current_name = visible.get(selected).cloned();
                    let new_filter = next_dataset_filter(dataset_filter);
                    // Persisted on App too (not just this modal instance) - see the sample
                    // picker's `H`/`S` handlers for why: the next `o` should reopen with the same
                    // filter instead of reverting to "all".
                    app.diff_dataset_filter = new_filter;
                    app.modal = Some(open_diff_picker_modal(
                        options,
                        current_name.as_deref().unwrap_or(&app.name),
                        new_filter,
                        hide_complete,
                        app.diff_completeness.as_ref(),
                        hide_painted,
                        app.diff_text_painted.as_ref(),
                    ));
                }
                // Deliberately `H` only, not `h`/`H` the way `O`'s sample picker binds its own
                // (cheap, instant) hide-solved toggle: this one can take several real seconds the
                // first time it's pressed in a session (see the comment below), so an accidental
                // lowercase `h` - reached for as `Left`/`collapse` muscle memory from the main
                // view - shouldn't be able to trigger it.
                KeyCode::Char('H') => {
                    let current_name = visible.get(selected).cloned();
                    // Computed lazily, once per session (see `App::diff_completeness`'s own doc
                    // comment for why this isn't done eagerly on every `o` press) - scanning the
                    // whole corpus takes real wall-clock time, so this key press can take a few
                    // seconds the first time it's pressed in a session.
                    if app.diff_completeness.is_none() {
                        app.diff_completeness = Some(compute_diff_completeness());
                    }
                    let new_hide_complete = !hide_complete;
                    // Persisted on App too - see the `d`/`D` arm just above for why.
                    app.diff_hide_complete = new_hide_complete;
                    app.modal = Some(open_diff_picker_modal(
                        options,
                        current_name.as_deref().unwrap_or(&app.name),
                        dataset_filter,
                        new_hide_complete,
                        app.diff_completeness.as_ref(),
                        hide_painted,
                        app.diff_text_painted.as_ref(),
                    ));
                }
                // `X`, not `x`: same reasoning as `H` just above, and the two are neighbours in
                // spirit - one narrows to fixtures whose *tree* mapping is unfinished, this one to
                // fixtures whose *text* painting hasn't been started. They are separate ground
                // truths (see `HumanTextMapping`), so these are separate queues; turning both on
                // shows only cases that need work on both.
                KeyCode::Char('X') => {
                    let current_name = visible.get(selected).cloned();
                    // Lazily once per session, like `H`'s scan - much cheaper than that one (JSON
                    // skim, no tree-sitter), but not free across the whole corpus.
                    if app.diff_text_painted.is_none() {
                        app.diff_text_painted = Some(compute_diff_text_painted());
                    }
                    let new_hide_painted = !hide_painted;
                    app.diff_hide_painted = new_hide_painted;
                    app.modal = Some(open_diff_picker_modal(
                        options,
                        current_name.as_deref().unwrap_or(&app.name),
                        dataset_filter,
                        hide_complete,
                        app.diff_completeness.as_ref(),
                        new_hide_painted,
                        app.diff_text_painted.as_ref(),
                    ));
                }
                KeyCode::Enter => {
                    if let Some(name) = visible.get(selected) {
                        let target = OpenTarget::Diffs(name.clone());
                        if app.dirty {
                            let can_save = matches!(app.origin, CaseOrigin::Diffs);
                            app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                        } else {
                            return Some(target);
                        }
                    } else {
                        app.modal = Some(Modal::OpenDiffPicker {
                            options,
                            selected,
                            dataset_filter,
                            hide_complete,
                            hide_painted,
                        });
                    }
                }
                KeyCode::Esc => {
                    app.status = Some("Cancelled".to_string());
                }
                _ => {
                    app.modal = Some(Modal::OpenDiffPicker {
                        options,
                        selected,
                        dataset_filter,
                        hide_complete,
                        hide_painted,
                    });
                }
            }
        }
        Modal::OpenSamplePicker {
            options,
            selected,
            hide_solved,
            sort_order,
        } => {
            let visible = visible_sample_options(&options, hide_solved, sort_order);

            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    app.modal = Some(Modal::OpenSamplePicker {
                        selected: selected.saturating_sub(1),
                        options,
                        hide_solved,
                        sort_order,
                    });
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.modal = Some(Modal::OpenSamplePicker {
                        selected: (selected + 1).min(visible.len().saturating_sub(1)),
                        options,
                        hide_solved,
                        sort_order,
                    });
                }
                KeyCode::Enter => {
                    if let Some((name, ..)) = visible.get(selected) {
                        let target = OpenTarget::Sample(name.clone());
                        if app.dirty {
                            let can_save = matches!(app.origin, CaseOrigin::Diffs);
                            app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                        } else {
                            return Some(target);
                        }
                    } else {
                        app.modal = Some(Modal::OpenSamplePicker {
                            options,
                            selected,
                            hide_solved,
                            sort_order,
                        });
                    }
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    let current_name = visible.get(selected).map(|(name, ..)| name.clone());
                    let new_hide_solved = !hide_solved;
                    let new_visible = visible_sample_options(&options, new_hide_solved, sort_order);
                    let new_selected = current_name
                        .and_then(|name| new_visible.iter().position(|(n, ..)| *n == name))
                        .unwrap_or(0)
                        .min(new_visible.len().saturating_sub(1));
                    // Persisted on App too (not just this modal instance), so the next `O` opens
                    // with the same hide-solved state instead of reverting to "show all".
                    app.sample_hide_solved = new_hide_solved;
                    app.modal = Some(Modal::OpenSamplePicker {
                        options,
                        selected: new_selected,
                        hide_solved: new_hide_solved,
                        sort_order,
                    });
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    // Unlike `H`, deliberately does not track the current name across the
                    // re-sort: changing sort order is about jumping to whichever end of the new
                    // order is interesting (e.g. the largest diff), so landing on 1 there is more
                    // useful than staying on whatever was selected under the old order.
                    let new_sort_order = sort_order.next();
                    // Persisted on App too - see the `H` arm just above for why.
                    app.sample_sort_order = new_sort_order;
                    app.modal = Some(Modal::OpenSamplePicker {
                        options,
                        selected: 0,
                        hide_solved,
                        sort_order: new_sort_order,
                    });
                }
                KeyCode::Esc => {
                    app.status = Some("Cancelled".to_string());
                }
                _ => {
                    app.modal = Some(Modal::OpenSamplePicker {
                        options,
                        selected,
                        hide_solved,
                        sort_order,
                    });
                }
            }
        }
        Modal::OpenCommitPicker { commits, selected } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::OpenCommitPicker {
                    selected: selected.saturating_sub(1),
                    commits,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::OpenCommitPicker {
                    selected: (selected + 1).min(commits.len().saturating_sub(1)),
                    commits,
                });
            }
            KeyCode::Enter => {
                if let Some((hash, summary)) = commits.get(selected).cloned() {
                    match list_commit_files(&hash) {
                        Ok(files) if !files.is_empty() => {
                            app.modal = Some(Modal::OpenCommitFilePicker {
                                hash,
                                summary,
                                files,
                                selected: 0,
                            });
                        }
                        Ok(_) => {
                            app.status = Some(format!(
                                "No files with a supported language changed in commit {} \
                                 (a merge commit shows no diff here by default - see \
                                 `list_commit_files`)",
                                short_hash(&hash)
                            ));
                            app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                        }
                        Err(err) => {
                            app.status = Some(format!(
                                "Error listing files for commit {}: {:#}",
                                short_hash(&hash),
                                err
                            ));
                            app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                        }
                    }
                } else {
                    app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::OpenCommitPicker { commits, selected });
            }
        },
        Modal::OpenCommitFilePicker {
            hash,
            summary,
            files,
            selected,
        } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    selected: selected.saturating_sub(1),
                    hash,
                    summary,
                    files,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    selected: (selected + 1).min(files.len().saturating_sub(1)),
                    hash,
                    summary,
                    files,
                });
            }
            KeyCode::Enter => {
                if let Some(path) = files.get(selected).cloned() {
                    let target = OpenTarget::GitCommitFile {
                        hash,
                        summary,
                        path,
                    };
                    if app.dirty {
                        let can_save = matches!(app.origin, CaseOrigin::Diffs);
                        app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                    } else {
                        return Some(target);
                    }
                } else {
                    app.modal = Some(Modal::OpenCommitFilePicker {
                        hash,
                        summary,
                        files,
                        selected,
                    });
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    hash,
                    summary,
                    files,
                    selected,
                });
            }
        },
        Modal::ConfirmDiscardUnsaved { target, can_save } => match code {
            KeyCode::Char('s') | KeyCode::Char('S') if can_save => {
                match action_save(&mut app.mapping, &mut app.dirty, &app.name, None) {
                    Ok(_) => {
                        refresh_diff_completeness(app, &app.name.clone());
                        refresh_diff_text_painted(app, &app.name.clone());
                        return Some(target);
                    }
                    Err(err) => {
                        app.status = Some(format!(
                            "Save failed ({:#}); not opening '{}'.",
                            err,
                            target.name()
                        ));
                    }
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                return Some(target);
            }
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
            }
        },
        Modal::PromptPromoteName {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let new_name = input.trim().to_string();
                match action_promote(app, &new_name, before_src, after_src) {
                    Ok(msg) => app.status = Some(msg),
                    Err(err) => {
                        app.modal = Some(Modal::PromptPromoteName {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
        },
        Modal::PromptRejectReason {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let reason = input.trim().to_string();
                match action_reject(app, &reason) {
                    Ok(msg) => app.status = Some(msg),
                    Err(err) => {
                        app.modal = Some(Modal::PromptRejectReason {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
        },
        Modal::PromptComment {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let comment = input.trim().to_string();
                match action_comment(app, &comment) {
                    Ok(msg) => {
                        refresh_diff_comment(app, &app.name.clone());
                        app.status = Some(msg);
                    }
                    Err(err) => {
                        app.modal = Some(Modal::PromptComment {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
        },
        Modal::PromptSearch { mut input } => match code {
            KeyCode::Enter => {
                let query = input.trim().to_string();
                if query.is_empty() {
                    app.status = Some("Search cancelled: empty query".to_string());
                } else {
                    app.last_search = Some(query.clone());
                    let focus = app.focus;
                    app.status = Some(
                        match action_search(
                            app,
                            focus,
                            before_flat,
                            after_flat,
                            before_src,
                            after_src,
                            &query,
                        ) {
                            Ok(msg) => msg,
                            Err(err) => format!("{:#}", err),
                        },
                    );
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptSearch { input });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptSearch { input });
            }
            _ => {
                app.modal = Some(Modal::PromptSearch { input });
            }
        },
        Modal::TextView { mut state } => {
            // `before_src`/`after_src` are the bytes of a `String` (`Code::contents`), so these
            // conversions cannot fail; the fallback exists so a hypothetical non-UTF-8 source
            // degrades to an empty painting surface rather than panicking mid-session.
            let before_text = std::str::from_utf8(before_src).unwrap_or_default();
            let after_text = std::str::from_utf8(after_src).unwrap_or_default();
            // The viewport height the cursor has to stay inside. The real popup height isn't known
            // outside `render_text_view_modal`, and threading it back here would couple the key
            // handler to the layout for one number - a conservative constant keeps the cursor on
            // screen for any terminal at least this tall and merely over-scrolls on a shorter one.
            const VIEWPORT_ROWS: usize = 20;
            let focused_source = if state.side == 0 {
                before_text
            } else {
                after_text
            };
            let mut close = false;

            // While the `:` prompt is open it takes every keystroke, so a digit is a digit rather
            // than a movement command.
            if let Some(mut typed) = state.line_prompt.take() {
                match code {
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        typed.push(c);
                        state.line_prompt = Some(typed);
                    }
                    KeyCode::Backspace => {
                        typed.pop();
                        state.line_prompt = Some(typed);
                    }
                    KeyCode::Enter => match typed.parse::<usize>() {
                        // 1-based in, 0-based out: the gutter shows 1-based numbers, so that is
                        // what a reader will type.
                        Ok(line) if line >= 1 => {
                            let last = TextPaintState::row_count(focused_source).saturating_sub(1);
                            let row = (line - 1).min(last);
                            state.cursor[state.side] = (row, 0);
                            app.status = Some(format!("Jumped to line {}", row + 1));
                        }
                        _ => app.status = Some(format!("Not a line number: {typed:?}")),
                    },
                    KeyCode::Esc => app.status = Some("Cancelled".to_string()),
                    _ => state.line_prompt = Some(typed),
                }
                state.scroll_into_view(VIEWPORT_ROWS);
                app.modal = Some(Modal::TextView { state });
                return None;
            }

            match code {
                KeyCode::Tab => state.side = 1 - state.side,
                KeyCode::Char(':') => {
                    state.line_prompt = Some(String::new());
                    app.status =
                        Some("Jump to line: type a number, Enter to go, Esc to cancel".to_string());
                }
                KeyCode::Up | KeyCode::Char('k') => state.step_row(-1, focused_source),
                KeyCode::Down | KeyCode::Char('j') => state.step_row(1, focused_source),
                KeyCode::Left | KeyCode::Char('h') => state.step_column(false, focused_source),
                KeyCode::Right | KeyCode::Char('l') => state.step_column(true, focused_source),
                KeyCode::PageUp => state.step_row(-(VIEWPORT_ROWS as isize), focused_source),
                KeyCode::PageDown => state.step_row(VIEWPORT_ROWS as isize, focused_source),
                KeyCode::Char('0') | KeyCode::Home => state.cursor[state.side].1 = 0,
                KeyCode::Char('$') | KeyCode::End => {
                    let row = state.cursor[state.side].0;
                    state.cursor[state.side].1 =
                        TextPaintState::row_text(focused_source, row).len();
                }
                KeyCode::Char('g') => state.cursor[state.side] = (0, 0),
                KeyCode::Char('G') => {
                    let last = TextPaintState::row_count(focused_source).saturating_sub(1);
                    state.cursor[state.side] = (last, 0);
                }
                KeyCode::Char('v') => {
                    state.anchor[state.side] = match state.anchor[state.side] {
                        Some(_) => None,
                        None => Some(state.cursor[state.side]),
                    };
                    app.status = Some(match state.anchor[state.side] {
                        Some(_) => "Selecting - move, then d/i/m".to_string(),
                        None => "Selection cleared".to_string(),
                    });
                }
                // Same pair the tree panels use for their own multi-map selection: `x` banks what
                // is selected so another range can be selected on the same side, `c` clears both
                // sides' banks. This is what makes an N:M match reachable - one live selection can
                // only ever describe one range.
                KeyCode::Char('x') => match state.selection(state.side, focused_source) {
                    Some(span) => {
                        state.pending[state.side].push(span);
                        state.anchor[state.side] = None;
                        let banked = state.pending[state.side].len();
                        app.status = Some(format!(
                            "Banked {banked} range(s) on this side - select another, then d/i/m"
                        ));
                    }
                    None => {
                        app.status = Some("Nothing selected to bank - press v first".to_string())
                    }
                },
                KeyCode::Char('c') => {
                    state.pending = [Vec::new(), Vec::new()];
                    state.anchor = [None; 2];
                    app.status = Some("Cleared banked ranges on both sides".to_string());
                }
                KeyCode::Char('m') => action_paint_match(app, &mut state, before_text, after_text),
                KeyCode::Char('d') => action_paint_one_sided(
                    app,
                    &mut state,
                    HumanTextOperation::Delete,
                    before_text,
                    after_text,
                ),
                KeyCode::Char('i') => action_paint_one_sided(
                    app,
                    &mut state,
                    HumanTextOperation::Insert,
                    before_text,
                    after_text,
                ),
                KeyCode::Char('u') => action_paint_unmark(app, &state, before_text, after_text),
                KeyCode::Char('Z') => action_paint_mark_empty(app),
                KeyCode::Char('p') => {
                    let next = app.text_overlay.next();
                    // Computed on the first cycle away from `Human` and kept for the rest of the
                    // case: running codediff is real work on a large fixture, and the default view
                    // never needs it.
                    if next != TextOverlay::Human && app.algo_text_spans.is_none() {
                        app.algo_text_spans = Some(codediff_text_spans(before, after));
                    }
                    app.text_overlay = next;
                    app.status = Some(match next {
                        TextOverlay::Human => "Showing your painting".to_string(),
                        TextOverlay::CodeDiff => "Showing codediff's own diff".to_string(),
                        TextOverlay::Disagreements => {
                            let differing: usize = app
                                .algo_text_spans
                                .as_ref()
                                .map(|_| {
                                    overlay_disagreement_spans(
                                        &[
                                            painted_spans(
                                                &app.mapping,
                                                &app.text_solution,
                                                0,
                                                before_text,
                                                after_text,
                                            ),
                                            painted_spans(
                                                &app.mapping,
                                                &app.text_solution,
                                                1,
                                                before_text,
                                                after_text,
                                            ),
                                        ],
                                        app.algo_text_spans.as_ref().expect("just computed"),
                                        before_text,
                                        after_text,
                                    )
                                    .iter()
                                    .map(Vec::len)
                                    .sum()
                                })
                                .unwrap_or(0);
                            if differing == 0 {
                                "You and codediff agree everywhere".to_string()
                            } else {
                                format!("Showing {differing} disagreeing range(s)")
                            }
                        }
                    });
                }
                // `s` and `L` both raise the same picker; `saving` is the only difference, and it
                // decides only what Enter does with the chosen name.
                KeyCode::Char('s') | KeyCode::Char('L') => {
                    let saving = matches!(code, KeyCode::Char('s'));
                    app.modal = Some(Modal::SolutionPicker {
                        names: solution_picker_names(&app.mapping),
                        selected: 0,
                        saving,
                        new_name: None,
                        confirm_delete: None,
                        state,
                    });
                    return None;
                }
                KeyCode::Char('T') => match run_unix_diff(before_src, after_src) {
                    Ok(output) => {
                        app.modal = Some(Modal::UnixDiffView { output, scroll: 0 });
                        return None;
                    }
                    Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
                },
                KeyCode::Esc => {
                    // Esc backs out one step at a time, so an accidental `v` - or a half-built
                    // N:M group - doesn't cost the whole view. Only an Esc with nothing pending
                    // closes.
                    if state.anchor[state.side].is_some() {
                        state.anchor[state.side] = None;
                        app.status = Some("Selection cleared".to_string());
                    } else if !state.pending[state.side].is_empty() {
                        state.pending[state.side].clear();
                        app.status = Some("Banked ranges cleared on this side".to_string());
                    } else {
                        close = true;
                    }
                }
                _ => {}
            }

            if close {
                app.status = Some("Closed text view".to_string());
            } else {
                state.scroll_into_view(VIEWPORT_ROWS);
                app.modal = Some(Modal::TextView { state });
            }
        }
        Modal::SolutionPicker {
            names,
            selected,
            saving,
            new_name,
            confirm_delete,
            state,
        } => {
            let reopen = |app: &mut App, names, selected, new_name, confirm_delete| {
                app.modal = Some(Modal::SolutionPicker {
                    names,
                    selected,
                    saving,
                    new_name,
                    confirm_delete,
                    state: state.clone(),
                });
            };
            // The free-form row always sits one past the named ones.
            let free_form_index = names.len();

            match (new_name, code) {
                // ── typing a new name ────────────────────────────────────────────────────────
                (Some(mut typed), KeyCode::Char(c)) => {
                    typed.push(c);
                    reopen(app, names, selected, Some(typed), None);
                }
                (Some(mut typed), KeyCode::Backspace) => {
                    typed.pop();
                    reopen(app, names, selected, Some(typed), None);
                }
                (Some(typed), KeyCode::Enter) => {
                    if saving {
                        action_save_solution_as(app, &typed, true);
                    } else {
                        action_load_solution(app, typed.trim());
                    }
                    app.modal = Some(Modal::TextView { state });
                }
                // Esc backs out of typing to the list rather than closing outright, so a mistyped
                // name costs one key, not the whole picker.
                (Some(_), KeyCode::Esc) => reopen(app, names, selected, None, None),
                (Some(typed), _) => reopen(app, names, selected, Some(typed), None),

                // ── choosing from the list ───────────────────────────────────────────────────
                (None, KeyCode::Up | KeyCode::Char('k')) => {
                    reopen(app, names, selected.saturating_sub(1), None, None)
                }
                (None, KeyCode::Down | KeyCode::Char('j')) => {
                    let next = (selected + 1).min(free_form_index);
                    reopen(app, names, next, None, None)
                }
                (None, KeyCode::Enter) => {
                    if selected == free_form_index {
                        reopen(app, names, selected, Some(String::new()), None);
                    } else {
                        let chosen = names[selected].clone();
                        if saving {
                            action_save_solution_as(app, &chosen, true);
                        } else {
                            action_load_solution(app, &chosen);
                        }
                        app.modal = Some(Modal::TextView { state });
                    }
                }
                // `e` is the one-key alternative to Enter: start the chosen name from nothing
                // instead of from a copy of what is currently painted.
                (None, KeyCode::Char('e')) if saving && selected < free_form_index => {
                    let chosen = names[selected].clone();
                    action_save_solution_as(app, &chosen, false);
                    app.modal = Some(Modal::TextView { state });
                }
                // `D` twice deletes the highlighted painting. Capital, and twice, because there
                // is no undo: the first press names what is about to go in the picker's title, the
                // second acts on that name rather than on whatever row the cursor reached in
                // between. Any other key clears the pending confirmation.
                (None, KeyCode::Char('D')) if selected < free_form_index => {
                    let chosen = names[selected].clone();
                    let exists = app
                        .mapping
                        .text_mappings
                        .iter()
                        .any(|named| named.name == chosen);
                    if !exists {
                        app.status = Some(format!(
                            "'{chosen}' is only a suggestion - nothing to delete"
                        ));
                        reopen(app, names, selected, None, None);
                    } else if confirm_delete.as_deref() == Some(chosen.as_str()) {
                        action_delete_solution(app, &chosen);
                        let names = solution_picker_names(&app.mapping);
                        let selected = selected.min(names.len());
                        reopen(app, names, selected, None, None);
                    } else {
                        app.status = Some(format!("Press D again to delete '{chosen}'"));
                        reopen(app, names, selected, None, Some(chosen));
                    }
                }
                (None, KeyCode::Esc) => {
                    app.status = Some("Cancelled".to_string());
                    app.modal = Some(Modal::TextView { state });
                }
                (None, _) => reopen(app, names, selected, None, None),
            }
        }
        Modal::UnixDiffView { output, scroll } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_sub(1),
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_add(1),
                });
            }
            KeyCode::PageUp => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_sub(10),
                });
            }
            KeyCode::PageDown => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_add(10),
                });
            }
            KeyCode::Char('t') => {
                app.modal = Some(Modal::TextView {
                    state: TextPaintState::default(),
                });
            }
            KeyCode::Esc => {
                app.status = Some("Closed diff view".to_string());
            }
            _ => {
                app.modal = Some(Modal::UnixDiffView { output, scroll });
            }
        },
        Modal::Help { scroll } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::Help {
                    scroll: scroll.saturating_sub(1),
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::Help {
                    scroll: scroll.saturating_add(1),
                });
            }
            KeyCode::Esc | KeyCode::Char('?') => {
                app.status = Some("Closed help".to_string());
            }
            _ => {
                app.modal = Some(Modal::Help { scroll });
            }
        },
    }

    None
}

/// `comment` is only ever `Some` from `action_promote` (a sample's recorded `Modal::PromptComment`
/// text, if any) - the plain save path (`s` on an already-real `CaseOrigin::Diffs` case) always
/// passes `None`, since only samples have a comment to carry forward. Only takes effect when
/// `ensure_stub_test` is *creating* the stub file for the first time; a comment added or edited
/// after promotion has no generated file left to write into.
fn action_save(
    mapping: &mut HumanMapping,
    dirty: &mut bool,
    name: &str,
    comment: Option<&str>,
) -> Result<String> {
    human_mapping::save(name, mapping)?;
    let created = ensure_stub_test(name, comment)?;
    // Only once there is something to score: an unpainted fixture has no painting for the test to
    // compare against, and a stub for it would fail rather than report a distance.
    if !mapping.text_mappings.is_empty() {
        ensure_painting_stub_test(name)?;
    }
    *dirty = false;
    Ok(if created {
        format!(
            "Saved human_mapping.json and created optimal_solutions/{}.rs",
            module_name(name)
        )
    } else {
        "Saved human_mapping.json".to_string()
    })
}

/// Rust keywords (2015 through 2024 edition, strict and reserved). `module_name` turns a case
/// name directly into a module identifier (`-` -> `_`), so a name that collides with one of these
/// would produce a stub that fails to compile -- caught here instead, before anything is written.
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// A name must be non-empty, start with a letter (so `module_name` -- which just swaps `-` for
/// `_` -- produces a valid Rust identifier) and contain only characters safe to use directly as
/// a directory name.
fn validate_new_case_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Name cannot be empty");
    }
    if !name.chars().next().unwrap().is_ascii_alphabetic() {
        bail!("Name must start with a letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Name may only contain letters, digits, '-' and '_'");
    }
    if RUST_KEYWORDS.contains(&module_name(name).as_str()) {
        bail!(
            "'{}' becomes the Rust keyword '{}' as a module name; pick another name",
            name,
            module_name(name)
        );
    }
    Ok(())
}

/// Promotes the currently open sample or git-commit-sourced case (`app.origin` must be
/// `CaseOrigin::Sample` or `CaseOrigin::GitCommitFile`) into a real test case under
/// `src/test/data/diffs/<dataset>/<new_name>/` (`dataset` per `promote_target_dataset`: a
/// sample's own recorded `source.dataset`, or always `"handmade"` for a git-commit-sourced case):
/// copies the before/after content sitting in `before_src`/`after_src` (the same bytes currently
/// on screen), saves human_mapping.json and the optimal_solutions stub via the normal
/// `action_save` path, and -- for a sample only, since a git-commit-sourced case has no
/// sample.csv row to update -- records `new_name` against the matching row in sample.csv. On
/// success, `app` is switched over to the new diffs/ case so subsequent `s` presses behave like a
/// normal save.
fn action_promote(
    app: &mut App,
    new_name: &str,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<String> {
    let origin = app.origin.clone();
    let (path, sample_source): (String, Option<SampleSource>) = match &origin {
        CaseOrigin::Sample(source) => (source.path.clone(), Some(source.clone())),
        CaseOrigin::GitCommitFile { path } => (path.clone(), None),
        CaseOrigin::Diffs => bail!("Current case is not a sample or a git-commit-sourced case"),
    };
    let dataset = promote_target_dataset(&origin)
        .expect("just matched Sample or GitCommitFile above, both of which return Some")
        .to_string();

    validate_new_case_name(new_name)?;

    if let Some(source) = &sample_source {
        // The sample's own recorded provenance decides the target dataset folder, not a hardcoded
        // guess - see `SampleSource::dataset`. Checked against `DIFF_DATASETS` rather than trusted
        // outright: a bad value here (e.g. a hand-edited source.json, or a `--dataset` typo when
        // the sample was originally materialized) would otherwise silently create a fourth diffs/
        // folder that nothing else in this codebase knows to look in.
        if !DIFF_DATASETS.contains(&source.dataset.as_str()) {
            bail!(
                "sample's recorded dataset '{}' is not one of {:?} - check source.json under \
                 src/test/data/samples/{}/",
                source.dataset,
                DIFF_DATASETS,
                app.name
            );
        }
    }

    // Collision check spans every dataset folder (`diffs_case_dir` searches `DIFF_DATASETS`) - the
    // flat-name lookup every other case name resolution in this file relies on breaks the moment
    // two different datasets can hold the same name.
    if diffs_case_dir(new_name).is_some() {
        bail!("'{}' already exists in src/test/data/diffs", new_name);
    }
    let dir = diffs_root().join(&dataset).join(new_name);

    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("path {} has no extension", path))?;

    fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
    fs::write(dir.join(format!("before.{ext}.test")), before_src)?;
    fs::write(dir.join(format!("after.{ext}.test")), after_src)?;

    // Carries the sample's provenance/license attribution (written by `materialize_test_diffs`,
    // via `codediff::stats::license`) forward into `diffs/` - the before/after content is
    // someone else's code, not codediff's own, so that attribution needs to survive promotion,
    // not just live in `samples/` until this directory gets cleaned up once triage finishes (see
    // the `d285097` commit that deleted the small dataset's fully-triaged `samples/`). Only a
    // `Sample` origin has a README.md to copy; `GitCommitFile` promotions are sourced from this
    // repo's own commits, not a third-party one.
    let readme_note = if sample_source.is_some() {
        let readme_src = samples_root().join(&app.name).join("README.md");
        match fs::copy(&readme_src, dir.join("README.md")) {
            Ok(_) => String::new(),
            Err(err) => format!(
                " (failed to copy README.md from {:?}: {:#})",
                readme_src, err
            ),
        }
    } else {
        String::new()
    };

    // A sample's recorded comment (if any) rides along into the generated stub test - see
    // `action_save`'s doc comment for why this is fetched here rather than threaded further down.
    let comment = match &sample_source {
        Some(source) => sample_comment(source)?,
        None => None,
    };
    let save_msg = action_save(
        &mut app.mapping,
        &mut app.dirty,
        new_name,
        comment.as_deref(),
    )?;
    refresh_diff_completeness(app, new_name);
    refresh_diff_text_painted(app, new_name);

    let csv_note = match &sample_source {
        Some(source) => match update_sample_csv(source, new_name) {
            Ok(true) => String::new(),
            Ok(false) => " (source row not found in sample.csv; not updated)".to_string(),
            Err(err) => format!(" (failed to update sample.csv: {:#})", err),
        },
        None => String::new(),
    };

    app.name = new_name.to_string();
    app.origin = CaseOrigin::Diffs;

    Ok(format!(
        "Promoted to '{}'. {}{}{}",
        new_name, save_msg, csv_note, readme_note
    ))
}

/// Rejects the currently open sample instead of promoting it: records `reason` verbatim in its
/// sample.csv row (`comment`, with `status` set to `REJECTED`) and leaves everything else -- the
/// sample directory, `promoted_to` -- untouched. Only a sample has a sample.csv row to update; a
/// git-commit-sourced case (`CaseOrigin::GitCommitFile`) has nothing to reject.
fn action_reject(app: &App, reason: &str) -> Result<String> {
    let CaseOrigin::Sample(source) = &app.origin else {
        bail!("Only a sample (opened via O) can be rejected");
    };

    let reason = reason.trim();
    if reason.is_empty() {
        bail!("Rejection reason cannot be empty");
    }

    match reject_sample(source, reason)? {
        true => Ok(format!("Rejected '{}': {}", app.name, reason)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

/// Records or clears the currently open sample's sample.csv `comment` column, without touching
/// `status`/`promoted_to` -- unlike `R`'s reject flow (`action_reject`), this works regardless of
/// whether the sample is still unreviewed, already promoted, or already rejected, and an empty
/// `comment` is valid (clears any previously-recorded one, unlike a rejection reason, which can't
/// be empty). Only a sample has a sample.csv row to update; a git-commit-sourced case
/// (`CaseOrigin::GitCommitFile`) has nothing to comment on.
fn action_comment(app: &App, comment: &str) -> Result<String> {
    let comment = comment.trim();

    // A promoted or handmade fixture keeps its note in its own directory, as `description.md`,
    // because there is no sample.csv row to hold one - a handmade case was never sampled at all.
    // Written here and now rather than on the next `w`: `app.dirty` means "human_mapping.json has
    // unsaved edits", and letting one keystroke ride a flag that saves a different file is how a
    // comment gets lost to a quit-without-saving. The sample branch below has always written
    // immediately too, so this keeps `e` meaning one thing.
    if let CaseOrigin::Diffs = &app.origin {
        write_note(&app.name, comment)?;
        return Ok(if comment.is_empty() {
            format!("Cleared note for '{}' (description.md removed)", app.name)
        } else {
            format!("Wrote description.md for '{}'", app.name)
        });
    }

    let CaseOrigin::Sample(source) = &app.origin else {
        bail!("Only a diff (o) or a sample (O) can have a comment");
    };
    match set_sample_comment(source, comment)? {
        true if comment.is_empty() => Ok(format!("Cleared comment for '{}'", app.name)),
        true => Ok(format!("Set comment for '{}': {}", app.name, comment)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

fn sample_csv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("sample.csv")
}

struct SampleCsvRow {
    language: String,
    repository: String,
    commit: String,
    path: String,
    promoted_to: String,
    dataset: String,
    /// One of `SAMPLED`/`PROMOTED`/`REJECTED` - see `sample_test_diffs::Row::status`.
    status: String,
    /// Free-form note about this sample, verbatim from `Modal::PromptComment`'s input - independent
    /// of `status`: settable (and editable) whether the row is still SAMPLED, already PROMOTED, or
    /// REJECTED. `action_reject` also writes here (the rejection reason *is* the comment, not a
    /// separate column) - see that function and `Modal::PromptRejectReason`. Empty if never set.
    comment: String,
}

/// Same backfill `sample_test_diffs::default_status` uses for a sample.csv row written before
/// `status` existed: duplicated rather than shared across the two binaries, same as the
/// `dataset` fallback ("small") a few lines below already is.
fn default_sample_status(promoted_to: &str) -> &'static str {
    if promoted_to.is_empty() {
        "SAMPLED"
    } else {
        "PROMOTED"
    }
}

fn read_sample_csv_rows(path: &Path) -> Result<Vec<SampleCsvRow>> {
    let mut reader = csv::Reader::from_path(path).with_context(|| format!("reading {:?}", path))?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let promoted_to = record.get(4).unwrap_or("").to_string();
        let status = match record.get(6) {
            Some(status) if !status.is_empty() => status.to_string(),
            _ => default_sample_status(&promoted_to).to_string(),
        };
        rows.push(SampleCsvRow {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to,
            // Same historical fallback as `legacy_dataset()`/`sample_test_diffs::LEGACY_DATASET`.
            dataset: record.get(5).unwrap_or("small").to_string(),
            status,
            comment: record.get(7).unwrap_or("").to_string(),
        });
    }
    Ok(rows)
}

fn write_sample_csv_rows(path: &Path, rows: &[SampleCsvRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path).with_context(|| format!("writing {:?}", path))?;
    writer.write_record([
        "language",
        "repository",
        "commit",
        "path",
        "promoted_to",
        "dataset",
        "status",
        "comment",
    ])?;
    for row in rows {
        writer.write_record([
            &row.language,
            &row.repository,
            &row.commit,
            &row.path,
            &row.promoted_to,
            &row.dataset,
            &row.status,
            &row.comment,
        ])?;
    }
    writer.flush()?;
    Ok(())
}

/// Finds the sample.csv row matching `source`'s identity (language/repository/commit/path) -
/// shared by every write path that needs to locate exactly one row to update
/// (`update_sample_csv_at`, `reject_sample_csv_at`, `set_sample_comment_at`), plus the read-only
/// `sample_comment_at`/the `e` keybinding's prefill lookup. `source` uniquely identifies a row by
/// construction (`sample_test_diffs` never writes two rows for the same commit+path), so "first
/// match" is never ambiguous in practice.
/// The `sample.csv` comment recorded against the sample this fixture was promoted from, if any.
///
/// Joined on `promoted_to`, the same key `diff_inventory` uses. Only ever a *fallback*: it fills
/// the `e` prompt for a diff that has no `description.md` yet, so a comment written before
/// promotion is one Enter away from becoming the note the inventory now prefers. A handmade
/// fixture was never sampled and has no row at all, which is exactly why `description.md` exists.
fn promoted_sample_comment(name: &str) -> Option<String> {
    let rows = read_sample_csv_rows(&sample_csv_path()).ok()?;
    rows.iter()
        .find(|row| row.promoted_to == name)
        .map(|row| row.comment.clone())
        .filter(|comment| !comment.trim().is_empty())
}

fn find_sample_row<'a>(
    rows: &'a [SampleCsvRow],
    source: &SampleSource,
) -> Option<&'a SampleCsvRow> {
    rows.iter().find(|row| {
        row.language == source.language
            && row.repository == source.repository
            && row.commit == source.commit
            && row.path == source.path
    })
}

/// Mutable counterpart of `find_sample_row`, for the write paths.
fn find_sample_row_mut<'a>(
    rows: &'a mut [SampleCsvRow],
    source: &SampleSource,
) -> Option<&'a mut SampleCsvRow> {
    rows.iter_mut().find(|row| {
        row.language == source.language
            && row.repository == source.repository
            && row.commit == source.commit
            && row.path == source.path
    })
}

fn update_sample_csv(source: &SampleSource, new_name: &str) -> Result<bool> {
    update_sample_csv_at(&sample_csv_path(), source, new_name)
}

/// Marks the sample.csv row matching `source` as promoted to `new_name`, preserving every other
/// row and column (including `comment`) untouched. Returns `Ok(false)` (not an error) if no row
/// matches -- e.g. the sample was placed under samples/ by hand rather than by
/// `sample_test_diffs` -- since that shouldn't undo a promotion that has already otherwise
/// succeeded.
fn update_sample_csv_at(path: &Path, source: &SampleSource, new_name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.promoted_to = new_name.to_string();
    row.status = "PROMOTED".to_string();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

fn reject_sample(source: &SampleSource, reason: &str) -> Result<bool> {
    reject_sample_csv_at(&sample_csv_path(), source, reason)
}

/// Marks the sample.csv row matching `source` as rejected, recording `reason` in its `comment`
/// column -- the reject counterpart of `update_sample_csv_at`. `promoted_to` is deliberately left
/// as-is (empty, in practice: `action_reject` only ever runs against a case that's still
/// `CaseOrigin::Sample`, which a promotion would have already moved past) since a rejected sample
/// was never promoted. Returns `Ok(false)` (not an error) if no row matches, same reasoning as
/// `update_sample_csv_at`.
fn reject_sample_csv_at(path: &Path, source: &SampleSource, reason: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.comment = reason.to_string();
    row.status = "REJECTED".to_string();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

fn set_sample_comment(source: &SampleSource, comment: &str) -> Result<bool> {
    set_sample_comment_at(&sample_csv_path(), source, comment)
}

/// Records `comment` verbatim in the sample.csv row matching `source`'s `comment` column,
/// preserving every other column -- including `status`/`promoted_to` -- untouched. Unlike
/// `reject_sample_csv_at`, this never changes `status`, so it works the same whether the row is
/// still SAMPLED, already PROMOTED, or REJECTED; an empty `comment` is valid and clears any
/// previously-recorded one. Returns `Ok(false)` (not an error) if no row matches, same reasoning
/// as `update_sample_csv_at`.
fn set_sample_comment_at(path: &Path, source: &SampleSource, comment: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.comment = comment.to_string();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

fn sample_comment(source: &SampleSource) -> Result<Option<String>> {
    sample_comment_at(&sample_csv_path(), source)
}

/// The `comment` column value for `source`'s row in sample.csv, if the row exists and its comment
/// is non-empty (after trimming) - `None` either way otherwise. `action_promote`'s own way of
/// asking "should the generated stub test get a leading explanatory comment".
fn sample_comment_at(path: &Path, source: &SampleSource) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let rows = read_sample_csv_rows(path)?;
    Ok(find_sample_row(&rows, source)
        .map(|row| row.comment.trim().to_string())
        .filter(|c| !c.is_empty()))
}

// ---------------------------------------------------------------------------------------------
// optimal_solutions test stub generation
// ---------------------------------------------------------------------------------------------

const LICENSE_HEADER: &str = "/*  This file is part of the CodeDiff code diffing tool.
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
";

fn module_name(name: &str) -> String {
    name.replace('-', "_")
}

/// `optimal_solutions/` mirrors `diffs/`'s split by dataset (see `DIFF_DATASETS`): `dataset`'s
/// fixtures get their stub test files here, alongside `optimal_solutions/<dataset>.rs`'s mod-list
/// (see `optimal_solutions_mod_file`).
fn optimal_solutions_dir(dataset: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions")
        .join(dataset)
}

fn optimal_solutions_mod_file(dataset: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions")
        .join(format!("{dataset}.rs"))
}

/// Creates `optimal_solutions/<dataset>/<name>.rs` if it doesn't already exist, and makes sure
/// it's registered in `optimal_solutions/<dataset>.rs`. Returns whether the stub `.rs` file was
/// newly created. `dataset` is resolved from `name`'s actual location under `diffs/`
/// (`case_dataset`) - every caller (an already-open existing case, or `action_promote`, which
/// creates the diffs/ directory before calling this) runs after that directory already exists, so
/// there's always a real dataset to resolve, no separate parameter needed.
///
/// `comment`, if non-empty once trimmed, is word-wrapped (`wrap_comment_lines`) into a leading `//`
/// block right before the `assert_matches_human_mapping` call - only when the file is actually
/// being created here for the first time; an already-existing stub is never rewritten, so a
/// comment added or edited after promotion has no effect.
fn ensure_stub_test(name: &str, comment: Option<&str>) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let dir = optimal_solutions_dir(&dataset);
    let stub_path = dir.join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        // `handmade`/`small`/`full` predate this dataset's `optimal_solutions/<dataset>/`
        // directory existing at all, so this was never exercised until `stratified` (or any
        // future dataset) needed it fresh on first promotion - real gap, not defensive
        // programming against something that can't happen.
        fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
        fs::write(&stub_path, stub_test_contents(name, comment))
            .with_context(|| format!("writing stub test to {:?}", stub_path))?;
        true
    };

    insert_mod_declaration(&dataset, &module)?;

    Ok(created)
}

/// Builds the full contents of a freshly-created `optimal_solutions/<dataset>/<name>.rs` stub -
/// split out from `ensure_stub_test` as a pure string-building function (no filesystem access) so
/// it's directly unit-testable without writing into the real repo's `src/test/optimal_solutions/`.
fn stub_test_contents(name: &str, comment: Option<&str>) -> String {
    let comment_block = match comment.map(str::trim) {
        Some(c) if !c.is_empty() => wrap_comment_lines(c),
        _ => String::new(),
    };
    format!(
        "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test;\n\n#[test]\nfn optimal_solution() -> Result<()> {{\n{comment_block}    test::helper::human_mapping::assert_matches_human_mapping(\"{name}\")\n}}\n"
    )
}

/// Word-wraps `comment` into `    // <text>\n` lines - 4-space indent matching the generated
/// stub's function body, `//` since this precedes a `#[test]` fn's own statement, not documenting
/// an item (a `///` doc comment there would attach to nothing). Wraps at a width matching this
/// codebase's own prose-comment convention (~96 columns including the prefix). `comment` is
/// assumed already trimmed and non-empty - see `ensure_stub_test`'s only caller.
fn wrap_comment_lines(comment: &str) -> String {
    const WIDTH: usize = 96;
    const PREFIX: &str = "    // ";
    let max_content = WIDTH.saturating_sub(PREFIX.len());

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in comment.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if candidate_len > max_content && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines
        .into_iter()
        .map(|line| format!("{PREFIX}{line}\n"))
        .collect()
}

/// Creates `src/test/painting_agreement/<name>.rs` and lists it, the painting counterpart of
/// [`ensure_stub_test`]. Returns whether a new file was written.
///
/// Only called once a fixture actually has a painting: an unpainted one has nothing to score, and
/// `assert_matches_human_painting_within_limit` would fail with "paint it first" rather than
/// report a distance. That is why this is not folded into `ensure_stub_test` - the two ground
/// truths are finished at different times, and a fixture routinely has a tree mapping months
/// before anyone paints it.
///
/// The stub is clamped at 100.0 rather than 0.0. Nothing in the corpus agrees exactly yet (the
/// rates run from hundredths of a percent to about 60%), so a fresh 0.0 would fail on the first
/// run and the writer would have to loosen it - which is exactly the "clamp moved for a reason
/// that was not a measurement" habit the per-file layout exists to prevent. 100.0 always passes,
/// and the comment says in plain words that it means nothing until measured.
fn ensure_painting_stub_test(name: &str) -> Result<bool> {
    let module = module_name(name);
    let dir = painting_agreement_dir();
    let stub_path = dir.join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
        fs::write(&stub_path, painting_stub_contents(name))
            .with_context(|| format!("writing painting stub to {:?}", stub_path))?;
        true
    };

    insert_painting_mod_declaration(&module)?;
    Ok(created)
}

/// The contents of a fresh painting stub - pure string building, no filesystem access, so it is
/// unit-testable without writing into the real `src/test/painting_agreement/`.
fn painting_stub_contents(name: &str) -> String {
    format!(
        "{LICENSE_HEADER}use anyhow::Result;\n\n\
         use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;\n\n\
         #[test]\nfn painting_agreement() -> Result<()> {{\n\
         \x20   // Not measured yet: 100.0 passes unconditionally. Run this test, read the rate it\n\
         \x20   // reports for both modes, and record that instead.\n\
         \x20   assert_matches_human_painting_within_limit(\"{name}\", 100.0)\n}}\n"
    )
}

/// Adds `mod <module>;` to `src/test/painting_agreement.rs`, keeping the list sorted and the
/// module doc above it untouched. A module already listed is left alone, so this is idempotent.
fn insert_painting_mod_declaration(module: &str) -> Result<()> {
    let mod_file = painting_agreement_mod_file();
    let content =
        fs::read_to_string(&mod_file).with_context(|| format!("reading {:?}", mod_file))?;

    let mut lines = content.lines().peekable();
    let mut header_lines = Vec::new();
    while let Some(&line) = lines.peek() {
        if line.trim() == "#[cfg(test)]" {
            break;
        }
        header_lines.push(line.to_string());
        lines.next();
    }

    let mut entries: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mod_line = lines.next().with_context(|| {
            format!(
                "'#[cfg(test)]' not followed by a mod line in {:?}",
                mod_file
            )
        })?;
        entries.push(mod_line.trim().to_string());
    }

    let wanted = format!("mod {module};");
    if !entries.contains(&wanted) {
        entries.push(wanted);
    }
    entries.sort();
    entries.dedup();

    let mut out = header_lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    for entry in entries {
        out.push_str("#[cfg(test)]\n");
        out.push_str(&entry);
        out.push('\n');
    }
    fs::write(&mod_file, out).with_context(|| format!("writing {:?}", mod_file))
}

fn painting_agreement_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("painting_agreement")
}

fn painting_agreement_mod_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("painting_agreement.rs")
}

/// Adds `#[cfg(test)]\nmod <module>;` to `optimal_solutions/<dataset>.rs`, keeping the list
/// sorted, unless it's already present.
fn insert_mod_declaration(dataset: &str, module: &str) -> Result<()> {
    let mod_file = optimal_solutions_mod_file(dataset);
    let content =
        fs::read_to_string(&mod_file).with_context(|| format!("reading {:?}", mod_file))?;

    let mut lines = content.lines().peekable();
    let mut header_lines = Vec::new();
    while let Some(&line) = lines.peek() {
        if line.trim() == "#[cfg(test)]" {
            break;
        }
        header_lines.push(line.to_string());
        lines.next();
    }

    let mut entries: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mod_line = lines.next().with_context(|| {
            format!(
                "'#[cfg(test)]' not followed by a mod line in {:?}",
                mod_file
            )
        })?;
        let trimmed = mod_line.trim();
        let mod_name = trimmed
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .with_context(|| {
                format!(
                    "unexpected line after '#[cfg(test)]' in {:?}: {:?}",
                    mod_file, mod_line
                )
            })?;
        entries.push(mod_name.to_string());
    }

    if !entries.iter().any(|e| e == module) {
        entries.push(module.to_string());
        entries.sort();
    }

    let mut out = header_lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    for entry in &entries {
        out.push_str("#[cfg(test)]\n");
        out.push_str(&format!("mod {entry};\n"));
    }

    fs::write(&mod_file, out).with_context(|| format!("writing {:?}", mod_file))?;
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
            "fn optimal_solution() -> Result<()> {\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
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
            "fn optimal_solution() -> Result<()> {\n    // A short note.\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
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
                    None,
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
                    None,
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
    /// The `X` filter narrows to cases with no painted text mapping, and fails *open* on a case
    /// the scan never reached - the opposite default from `hide_complete`, and deliberately so:
    /// hiding something never confirmed as painted would silently drop it out of the work queue.
    #[test]
    fn visible_diff_options_can_narrow_to_unpainted_cases() {
        let options = vec![
            ("painted".to_string(), "handmade"),
            ("unpainted".to_string(), "handmade"),
            ("never-scanned".to_string(), "handmade"),
        ];
        let painted = std::collections::HashMap::from([
            ("painted".to_string(), true),
            ("unpainted".to_string(), false),
        ]);

        assert_eq!(
            visible_diff_options(&options, None, false, None, false, Some(&painted)),
            vec!["painted", "unpainted", "never-scanned"],
            "the filter off shows everything"
        );
        assert_eq!(
            visible_diff_options(&options, None, false, None, true, Some(&painted)),
            vec!["unpainted", "never-scanned"],
            "on, it hides only cases confirmed painted"
        );
    }

    /// The two filters are separate queues over separate ground truths, so turning both on shows
    /// only what needs work on both.
    #[test]
    fn the_incomplete_and_unpainted_filters_combine_as_an_and() {
        let options = vec![
            ("both".to_string(), "handmade"),
            ("tree-only".to_string(), "handmade"),
            ("text-only".to_string(), "handmade"),
            ("done".to_string(), "handmade"),
        ];
        let incomplete = std::collections::HashMap::from([
            ("both".to_string(), true),
            ("tree-only".to_string(), true),
            ("text-only".to_string(), false),
            ("done".to_string(), false),
        ]);
        let painted = std::collections::HashMap::from([
            ("both".to_string(), false),
            ("tree-only".to_string(), true),
            ("text-only".to_string(), false),
            ("done".to_string(), true),
        ]);

        assert_eq!(
            visible_diff_options(
                &options,
                None,
                true,
                Some(&incomplete),
                true,
                Some(&painted)
            ),
            vec!["both"],
            "only the case needing both an unmarked node and a painting survives"
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

    /// The overlay cycles through all three and back, so `p` is always the only key needed.
    #[test]
    fn the_text_overlay_cycles_human_codediff_disagreements() {
        assert_eq!(TextOverlay::default(), TextOverlay::Human);
        assert_eq!(TextOverlay::Human.next(), TextOverlay::CodeDiff);
        assert_eq!(TextOverlay::CodeDiff.next(), TextOverlay::Disagreements);
        assert_eq!(TextOverlay::Disagreements.next(), TextOverlay::Human);
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

    /// The painting stub is clamped at 100.0, not 0.0. Nothing in the corpus agrees exactly yet,
    /// so a fresh 0.0 would fail on the first run and have to be loosened - which is the "clamp
    /// moved for a reason that was not a measurement" habit the per-file layout exists to prevent.
    #[test]
    fn a_fresh_painting_stub_passes_and_says_it_means_nothing_yet() {
        let contents = painting_stub_contents("rust-add-if");

        assert!(
            contents
                .contains(r#"assert_matches_human_painting_within_limit("rust-add-if", 100.0)"#),
            "got: {contents}"
        );
        assert!(contents.contains("Not measured yet"), "got: {contents}");
        assert!(
            contents.contains("fn painting_agreement()"),
            "the test name has to match the module's convention: {contents}"
        );
        // Same licence header every generated file in this repository carries.
        assert!(contents.starts_with("/*  This file is part of the CodeDiff"));
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

        let span = state.selection(0, source).expect("a selection");

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
        assert_eq!(
            forward.selection(0, source).unwrap(),
            backward.selection(0, source).unwrap()
        );
    }

    /// Byte columns, not character columns - a multi-byte character before the selection must not
    /// shift it. The same trap `span_text`'s own test pins from the other side.
    #[test]
    fn a_selection_past_a_multibyte_character_lands_on_the_right_text() {
        let source = "let é = foo;\n";
        // "let é = " is 9 bytes ('é' costs two), so `foo` starts at byte 9 and its last character
        // starts at byte 11.
        let state = paint_state_at(0, (0, 11), Some((0, 9)));

        let span = state.selection(0, source).expect("a selection");

        assert_eq!(
            codediff::test::helper::human_mapping::span_text(source, span),
            Some("foo")
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
            visible_diff_options(&options, None, false, None, false, None),
            vec!["alpha", "bravo", "charlie", "delta"],
            "no filter should show every dataset"
        );
        assert_eq!(
            visible_diff_options(&options, Some("handmade"), false, None, false, None),
            vec!["alpha", "charlie"]
        );
        assert_eq!(
            visible_diff_options(&options, Some("full"), false, None, false, None),
            vec!["delta"]
        );
    }

    #[test]
    fn visible_diff_options_hide_complete_excludes_only_cases_the_map_marks_complete() {
        let options = vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
            ("charlie".to_string(), "handmade"),
        ];
        let mut completeness = std::collections::HashMap::new();
        completeness.insert("alpha".to_string(), true); // incomplete
        completeness.insert("bravo".to_string(), false); // complete
        // "charlie" deliberately absent - not yet scanned.

        assert_eq!(
            visible_diff_options(&options, None, true, Some(&completeness), false, None),
            vec!["alpha", "charlie"],
            "complete should be hidden, incomplete and unscanned should both stay"
        );
        assert_eq!(
            visible_diff_options(&options, None, false, Some(&completeness), false, None),
            vec!["alpha", "bravo", "charlie"],
            "hide_complete=false should show everything regardless of the map"
        );
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

        // "charlie" is index 2 in `options`' own order, but index 1 once filtered to just
        // "handmade" - proves `selected` is computed against the filtered view, not raw options.
        let modal = open_diff_picker_modal(
            options,
            "charlie",
            Some("handmade"),
            false,
            None,
            false,
            None,
        );

        match modal {
            Modal::OpenDiffPicker {
                selected,
                dataset_filter,
                ..
            } => {
                assert_eq!(selected, 1);
                assert_eq!(dataset_filter, Some("handmade"));
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
        // "alpha" is the currently open case, but it's a "handmade" fixture and the filter below
        // is "small" - alpha isn't in the filtered view at all, so this must fall back to the
        // first visible entry instead of panicking or landing out of bounds.
        let modal =
            open_diff_picker_modal(options, "alpha", Some("small"), false, None, false, None);
        match modal {
            Modal::OpenDiffPicker { selected, .. } => assert_eq!(selected, 0),
            other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
        }
    }

    #[test]
    fn open_diff_picker_d_persists_dataset_filter_on_app() {
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
        app.modal = Some(Modal::OpenDiffPicker {
            options: vec![("alpha".to_string(), "handmade")],
            selected: 0,
            dataset_filter: None,
            hide_complete: false,
            hide_painted: false,
        });
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        handle_modal_key(
            &mut app,
            KeyCode::Char('d'),
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
            app.diff_dataset_filter,
            Some(DIFF_DATASETS[0]),
            "d's new filter must persist on App too, so the next o reopens with it"
        );
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
    fn tree_has_unmarked_node_is_false_only_once_every_node_is_marked() {
        let tree = parse_rust("fn main() {\n    a();\n}\n");
        let root = tree.root_node();

        assert!(
            tree_has_unmarked_node(root, &Caches::default(), status_before),
            "nothing marked at all should be reported as having an unmarked node"
        );

        let mut caches = Caches::default();
        mark_subtree_matched(root, &mut caches);
        assert!(
            !tree_has_unmarked_node(root, &caches, status_before),
            "every node (including unnamed tokens) is marked, so none should be left unmarked"
        );

        // Unmark just the deepest leaf again - a single hole anywhere should be enough to flip
        // the result back.
        let stmt = find_first(root, "expression_statement").unwrap();
        let call = find_first(stmt, "call_expression").unwrap();
        caches.before_match.remove(&call.id());
        assert!(
            tree_has_unmarked_node(root, &caches, status_before),
            "one unmarked node anywhere in the tree should be enough"
        );
    }

    #[test]
    fn diff_case_is_incomplete_returns_some_for_a_real_case_on_disk() {
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
            diff_case_is_incomplete(name).is_some(),
            "a real, on-disk case should always resolve to Some(_), not None"
        );
    }

    #[test]
    fn open_diff_picker_h_toggles_hide_complete_using_the_cached_completeness_map() {
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
        // Pre-seeded, so this exercises only the toggle - not the lazy full-corpus scan (see
        // `open_diff_picker_h_computes_completeness_lazily_when_not_yet_cached` for that).
        let mut completeness = std::collections::HashMap::new();
        completeness.insert("alpha".to_string(), true);
        completeness.insert("bravo".to_string(), false);
        app.diff_completeness = Some(completeness);

        app.modal = Some(Modal::OpenDiffPicker {
            options: vec![
                ("alpha".to_string(), "handmade"),
                ("bravo".to_string(), "handmade"),
            ],
            selected: 0,
            dataset_filter: None,
            hide_complete: false,
            hide_painted: false,
        });
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
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
            app.diff_hide_complete,
            "H should persist hide_complete on App too, so the next o reopens with it"
        );
        match &app.modal {
            Some(Modal::OpenDiffPicker {
                hide_complete,
                options,
                ..
            }) => {
                assert!(*hide_complete);
                assert_eq!(
                    options.len(),
                    2,
                    "the full options list itself is untouched"
                );
            }
            other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
        }
        assert_eq!(
            app.diff_completeness.as_ref().unwrap().len(),
            2,
            "an already-cached map should not be recomputed"
        );
    }

    #[test]
    fn open_diff_picker_h_computes_completeness_lazily_when_not_yet_cached() {
        // Full integrated path against whatever's actually under src/test/data/diffs/ - skips if
        // there's nothing to scan, same convention as
        // `diff_case_is_incomplete_returns_some_for_a_real_case_on_disk` above.
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
        assert!(app.diff_completeness.is_none());

        app.modal = Some(Modal::OpenDiffPicker {
            options: options.clone(),
            selected: 0,
            dataset_filter: None,
            hide_complete: false,
            hide_painted: false,
        });
        let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
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

        let map = app
            .diff_completeness
            .as_ref()
            .expect("H should compute completeness lazily when it wasn't cached yet");
        assert_eq!(map.len(), options.len());
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
