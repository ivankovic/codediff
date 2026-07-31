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
* `src/test/data/diffs/` (e.g. "rust-add-if"). If `<name>` is omitted, it opens the `o` case picker
* directly so you can choose one. It opens a Ratatui TUI showing the TreeSitter ASTs
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
*                  sample (opened via `O`): prompt for a name and promote it -- copies the sample's
*                  before/after content into src/test/data/diffs/<name>/, saves
*                  human_mapping.json and the test stub there, and records <name> against the
*                  matching row in sample.csv. Re-prompts if the name is empty, contains anything
*                  other than letters/digits/-/_, or a diffs/ case with that name already exists
*   o              open a different test case: lists every directory under src/test/data/diffs/,
*                  j/k to move, Enter to open, Esc to cancel. If the current mapping has unsaved
*                  changes, asks first whether to save (only offered for a real test case; see `s`
*                  above) or discard them before switching
*   O              like `o`, but lists sampled candidates under src/test/data/samples/ instead --
*                  see `s` above for what happens when one of these is saved. Samples already
*                  promoted (per sample.csv's `promoted_to` column) are marked " - SOLVED"; press
*                  `H` inside this picker to hide them, or `s` inside this picker to cycle its sort
*                  order: alphabetical, reverse alphabetical, smallest text diff first, largest
*                  text diff first (by changed-line count in a raw `diff -u`, not AST size) --
*                  unlike `H`, changing sort order always jumps selection to the first (1st) entry
*                  in the new order, rather than tracking the previously selected name
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

use codediff::code::Code;
use codediff::diff::{ASTDiff, ASTMappingReason, diff_code};
use codediff::test::helper::human_mapping::{
    self, HumanMapping, HumanMappingEntry, HumanOperation,
};
use codediff::test::helper::{code_pair_from_dir, node_for_path, path_for_node};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Interactively build a human ground-truth AST mapping for an optimal_solutions test case"
)]
struct Args {
    /// Name of the test case, i.e. the directory name under src/test/data/diffs/ (e.g.
    /// "rust-add-if"). Always starts with a language prefix. If omitted, opens the case picker
    /// directly.
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

m / M          match cursor nodes (M also recurses into matching children)
f              repeat m until end of file or a kind mismatch needs your input
d / D          mark Before node deleted / deleted with subtree
i / I          mark After node inserted / inserted with subtree
u              unmark the focused cursor node
a / A          align other panel to the human mapping / to codediff's mapping
p              run codediff's own diff, show its verdict next to each node
r              toggle showing the ASTMappingReason (which pass matched it) next
                 to each node's algo verdict
n / N          jump to next / previous mismatch (`*`) vs. codediff's verdict

t              view raw before/after text (not the AST tree)
T              view the output of unix `diff -u`
                 (t/T switch between these two views while either is open)
H              toggle hiding fully solved subtrees (unmarked nodes and their
                 ancestors always stay visible)

s              save -- or, on a sample, prompt for a name and promote it
o              open a different test case (src/test/data/diffs/)
O              open a sampled candidate (src/test/data/samples/); already-promoted
                 samples are marked \" - SOLVED\" -- press H inside this picker to
                 hide/show them, or s to cycle its sort order (A-Z, Z-A,
                 smallest/largest text diff first)

?              toggle this help
q / Esc        quit
";

/// Loads and parses the before/after code for a test case, by name. Parses only this one case's
/// directory (rather than every directory under src/test/data/diffs/, as a naive
/// `handmade_test_code_pairs`-style lookup would) since this runs on every `o`-picker open, not
/// just at startup.
fn load_case(name: &str) -> Result<(Code, Code)> {
    let dir = diffs_case_dir(name);
    if !dir.is_dir() {
        let available = list_available_cases().unwrap_or_default();
        bail!(
            "No test case named '{}' found in src/test/data/diffs.\nAvailable: {}",
            name,
            available.join(", ")
        );
    }

    let mut parser = tree_sitter::Parser::new();
    let (mut before, mut after) = code_pair_from_dir(&dir, &mut parser)
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

fn diffs_case_dir(name: &str) -> PathBuf {
    diffs_root().join(name)
}

/// Names of every directory directly under `root` (not just the ones that successfully parse
/// into a Code pair), for the `o`/`O` open pickers.
fn list_dir_names(root: &Path) -> Result<Vec<String>> {
    // Not an error: `samples/` in particular won't exist at all until `materialize_test_diffs`
    // has been run once.
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

fn list_available_cases() -> Result<Vec<String>> {
    list_dir_names(&diffs_root())
}

/// Every sample directory name under src/test/data/samples/, paired with whether it has already
/// been promoted into src/test/data/diffs/. A sample counts as promoted when its `source.json`
/// (language, repository, commit, path) matches a sample.csv row whose `promoted_to` column is
/// non-empty -- the same join `action_promote`/`update_sample_csv` use, so it stays correct even
/// if the promoted diffs/ case was later renamed or the sample directory has a numbered suffix.
fn list_samples_with_status() -> Result<Vec<(String, bool)>> {
    let names = list_dir_names(&samples_root())?;
    let promoted = promoted_sample_sources()?;

    Ok(names
        .into_iter()
        .map(|name| {
            let solved = source_json_for_sample(&name)
                .map(|source| {
                    promoted.contains(&(
                        source.language,
                        source.repository,
                        source.commit,
                        source.path,
                    ))
                })
                .unwrap_or(false);
            (name, solved)
        })
        .collect())
}

/// Reads just the provenance out of a sample's `source.json`, without parsing its before/after
/// code (unlike `load_sample`) -- cheap enough to call once per sample when listing.
fn source_json_for_sample(name: &str) -> Option<SampleSource> {
    let contents = fs::read_to_string(samples_root().join(name).join("source.json")).ok()?;
    serde_json::from_str(&contents).ok()
}

fn promoted_sample_sources() -> Result<std::collections::HashSet<(String, String, String, String)>>
{
    promoted_sample_sources_at(&sample_csv_path())
}

/// The (language, repository, commit, path) of every row in the sample.csv at `path` whose
/// `promoted_to` column is non-empty. Returns an empty set, not an error, if `path` doesn't exist.
fn promoted_sample_sources_at(
    path: &Path,
) -> Result<std::collections::HashSet<(String, String, String, String)>> {
    if !path.exists() {
        return Ok(std::collections::HashSet::new());
    }

    let mut reader = csv::Reader::from_path(path).with_context(|| format!("reading {:?}", path))?;
    let mut promoted = std::collections::HashSet::new();
    for record in reader.records() {
        let record = record?;
        if !record.get(4).unwrap_or("").is_empty() {
            promoted.insert((
                record[0].to_string(),
                record[1].to_string(),
                record[2].to_string(),
                record[3].to_string(),
            ));
        }
    }
    Ok(promoted)
}

/// Provenance recorded by `materialize_test_diffs` alongside each `src/test/data/samples/<name>/`
/// fixture (`source.json`): the exact `sample.csv` row it came from. Used, when the sample is
/// promoted, to find and update that row without having to reverse-engineer it from the
/// (lossy: lowercased, 8-char commit) directory name.
#[derive(Debug, Clone, Deserialize)]
struct SampleSource {
    language: String,
    repository: String,
    commit: String,
    path: String,
}

/// Loads and parses the before/after code for a sampled candidate under
/// `src/test/data/samples/<name>/`, along with its recorded provenance.
fn load_sample(name: &str) -> Result<(Code, Code, SampleSource)> {
    let dir = samples_root().join(name);

    let mut parser = tree_sitter::Parser::new();
    let (mut before, mut after) = code_pair_from_dir(&dir, &mut parser)
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
    options: &[(String, bool, usize)],
    hide_solved: bool,
    sort_order: SampleSortOrder,
) -> Vec<(String, bool, usize)> {
    let mut visible: Vec<(String, bool, usize)> = options
        .iter()
        .filter(|(_, solved, _)| !hide_solved || !*solved)
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

fn main() -> Result<()> {
    let args = Args::parse();

    // Loading a case requires an initial, valid AST pair to render behind the picker modal, so
    // when no name is given on the command line, fall back to the first available case and
    // immediately open the picker on top of it rather than restructuring the rest of the app to
    // tolerate no case being loaded at all.
    let (name, initial_picker_options) = match args.name {
        Some(name) => (name, None),
        None => {
            let options = list_available_cases()?;
            let first = options
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("No test cases found in src/test/data/diffs"))?;
            (first, Some(options))
        }
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

    if let Some(options) = initial_picker_options {
        app.modal = Some(Modal::OpenDiffPicker {
            options,
            selected: 0,
        });
    }

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
    /// Raised by `o`: pick a test case (a directory under src/test/data/diffs/) to open.
    OpenDiffPicker {
        options: Vec<String>,
        selected: usize,
    },
    /// Raised by `O`: pick a sampled candidate (a directory under src/test/data/samples/) to
    /// open. Each option is paired with whether it has already been promoted into
    /// src/test/data/diffs/ (per sample.csv's `promoted_to` column) -- shown as " - SOLVED" and,
    /// when `hide_solved` is set, left out of the list entirely -- and with its
    /// `sample_diff_line_count` (computed once when the picker opens, not on every `s` press).
    /// `selected` indexes into `visible_sample_options(&options, hide_solved, sort_order)`, not
    /// `options` itself.
    OpenSamplePicker {
        options: Vec<(String, bool, usize)>,
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
    /// Raised by `t`: shows the raw before/after source side by side, for reading the actual code
    /// instead of navigating the AST tree. `T` while open switches to `UnixDiffView` instead.
    TextView { scroll: u16 },
    /// Raised by `T`: shows the output of running the system `diff -u` between the before and
    /// after content -- a plain line-based diff, as a point of comparison against codediff's own
    /// AST-based diff (`p`). `t` while open switches to `TextView` instead.
    UnixDiffView { output: String, scroll: u16 },
    /// Raised by `?`: lists every keybinding. `?` or `Esc` while open closes it.
    Help { scroll: u16 },
}

/// Which open picker (`o` or `O`) a pending switch came from, and the name selected.
#[derive(Debug, Clone)]
enum OpenTarget {
    Diffs(String),
    Sample(String),
}

impl OpenTarget {
    fn name(&self) -> &str {
        match self {
            OpenTarget::Diffs(name) | OpenTarget::Sample(name) => name,
        }
    }
}

/// Where the currently open case's content lives: a committed test case, or a not-yet-promoted
/// sample. Determines what `s` does (see `Modal::PromptPromoteName`) and what `o`/`O` need to know
/// before switching away with unsaved changes.
#[derive(Debug, Clone)]
enum CaseOrigin {
    Diffs,
    Sample(SampleSource),
}

struct App {
    /// Name of the currently open case: a directory under src/test/data/diffs/ (if `origin` is
    /// `Diffs`) or src/test/data/samples/ (if `origin` is `Sample`). Can change at runtime via
    /// the `o`/`O` (open) pickers, or via promoting a sample with `s`.
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
}

impl App {
    fn new(
        name: String,
        origin: CaseOrigin,
        before_root_id: usize,
        after_root_id: usize,
        mapping: HumanMapping,
    ) -> Self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkKind {
    Deleted,
    Inserted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeStatus {
    Unmarked,
    Matched,
    /// `inherited` is true when this node isn't marked directly but an ancestor is marked
    /// with `with_children`, implying this node too.
    Marked {
        kind: MarkKind,
        with_children: bool,
        inherited: bool,
    },
}

/// Resolved node IDs for every human-authored entry, used to look up a node's status in O(1)
/// (plus a bounded ancestor walk for inheritance) while rendering.
#[derive(Default)]
struct Caches {
    before_match: HashMap<usize, usize>,
    after_match: HashMap<usize, usize>,
    before_removed: HashMap<usize, bool>,
    after_removed: HashMap<usize, bool>,
    /// Number of entries that couldn't be resolved against the current trees (e.g. a
    /// hand-edited or stale mapping file). Surfaced in the footer rather than treated as fatal,
    /// so a bad mapping file doesn't prevent the TUI from even opening.
    unresolved: usize,
}

fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

/// Builds lookup caches from `entries`, skipping (and counting) any entry that doesn't resolve
/// against the current trees rather than failing outright.
fn rebuild_caches(entries: &[HumanMappingEntry], before_root: Node, after_root: Node) -> Caches {
    let mut caches = Caches::default();

    for entry in entries {
        let resolved = match entry.operation {
            HumanOperation::Identical
            | HumanOperation::Update
            | HumanOperation::MatchButNotIdentical => (|| {
                let before_path = entry.before_path.as_ref()?;
                let after_path = entry.after_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches.before_match.insert(b.id(), a.id());
                caches.after_match.insert(a.id(), b.id());
                Some(())
            })(),
            HumanOperation::Delete | HumanOperation::DeleteWithChildren => (|| {
                let before_path = entry.before_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                caches.before_removed.insert(
                    b.id(),
                    entry.operation == HumanOperation::DeleteWithChildren,
                );
                Some(())
            })(),
            HumanOperation::Insert | HumanOperation::InsertWithChildren => (|| {
                let after_path = entry.after_path.as_ref()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches.after_removed.insert(
                    a.id(),
                    entry.operation == HumanOperation::InsertWithChildren,
                );
                Some(())
            })(),
        };

        if resolved.is_none() {
            caches.unresolved += 1;
        }
    }

    caches
}

/// True if some strict ancestor of `node` is marked with `with_children = true` in `removed`.
fn is_inherited_removed(node: Node, removed: &HashMap<usize, bool>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if removed.get(&parent.id()) == Some(&true) {
            return true;
        }
        current = parent;
    }
    false
}

fn status_before(node: Node, caches: &Caches) -> NodeStatus {
    if caches.before_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.before_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.before_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

fn status_after(node: Node, caches: &Caches) -> NodeStatus {
    if caches.after_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.after_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.after_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
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

fn move_cursor(panel: &mut PanelState, flat: &[(Node, usize)], delta: i32) {
    if flat.is_empty() {
        return;
    }
    let idx = flat
        .iter()
        .position(|(n, _)| n.id() == panel.cursor_id)
        .unwrap_or(0);
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
    flat: &[(Node, usize)],
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) {
    let Some(idx) = flat.iter().position(|(n, _)| n.id() == panel.cursor_id) else {
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
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the Before panel: used after a
/// delete, which only touches the Before side, so only that cursor should step forward.
fn advance_before_to_next_unmarked(
    app: &mut App,
    before_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the After panel: used after an
/// insert, which only touches the After side, so only that cursor should step forward.
fn advance_after_to_next_unmarked(
    app: &mut App,
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Moves `panel`'s cursor to the next (`forward`) or previous node where `disagrees_fn` is true,
/// relative to its current position, wrapping around the ends of `flat` like a `vim` `n`/`N`
/// search. Returns the node landed on, or `None` if `disagrees_fn` is false for every node.
fn advance_to_next_mismatch<'a>(
    panel: &mut PanelState,
    flat: &[(Node<'a>, usize)],
    caches: &Caches,
    diff_ast: &ASTDiff,
    disagrees_fn: fn(Node, &Caches, &ASTDiff) -> bool,
    forward: bool,
) -> Option<Node<'a>> {
    if flat.is_empty() {
        return None;
    }
    let len = flat.len();
    let idx = flat
        .iter()
        .position(|(n, _)| n.id() == panel.cursor_id)
        .unwrap_or(0);
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
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
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

fn expand_or_descend(panel: &mut PanelState, flat: &[(Node, usize)]) {
    let Some((node, _)) = flat.iter().find(|(n, _)| n.id() == panel.cursor_id) else {
        return;
    };
    let node = *node;
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

fn collapse_or_ascend(panel: &mut PanelState, flat: &[(Node, usize)]) {
    let Some((node, _)) = flat.iter().find(|(n, _)| n.id() == panel.cursor_id) else {
        return;
    };
    let node = *node;
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

    let was_visible = flatten_visible(other_root, &other.collapsed, None)
        .iter()
        .position(|(n, _)| n.id() == target_id)
        .is_some_and(|idx| {
            idx >= other.scroll && idx < other.scroll + other.viewport_height.max(1)
        });

    let target_node = find_node_by_id_anywhere(other_root, target_id)
        .context("Matched node not found in tree")?;
    expand_ancestors(&mut other.collapsed, target_node);
    other.cursor_id = target_id;

    if !was_visible {
        let flat = flatten_visible(other_root, &other.collapsed, None);
        let idx = flat
            .iter()
            .position(|(n, _)| n.id() == target_id)
            .unwrap_or(0);
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

fn find_node_by_id<'a>(flat: &[(Node<'a>, usize)], id: usize) -> Option<Node<'a>> {
    flat.iter().find(|(n, _)| n.id() == id).map(|(n, _)| *n)
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
    NeedsModal(Modal),
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

#[allow(clippy::too_many_arguments)]
fn action_match(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
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
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

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
        return Ok(ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
            before_id: before_node.id(),
            after_id: after_node.id(),
            before_kind: before_node.kind().to_string(),
            after_kind: after_node.kind().to_string(),
            recursive: false,
        }));
    }

    let operation = if before_node.child_count() == 0 && after_node.child_count() == 0 {
        if node_values_equal(before_node, after_node, before_src, after_src) {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        }
    } else {
        subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash)
    };
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
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> Result<ActionOutcome> {
    let mut matched = 0usize;
    let mut caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    let before_paths = precompute_paths(before_root);
    let after_paths = precompute_paths(after_root);

    let mut before_idx = before_flat
        .iter()
        .position(|(n, _)| n.id() == app.before.cursor_id)
        .context("Before cursor node not found")?;
    let mut after_idx = after_flat
        .iter()
        .position(|(n, _)| n.id() == app.after.cursor_id)
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
            return Ok(ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
                before_id: before_node.id(),
                after_id: after_node.id(),
                before_kind: before_node.kind().to_string(),
                after_kind: after_node.kind().to_string(),
                recursive: false,
            }));
        }

        let operation = if before_node.child_count() == 0 && after_node.child_count() == 0 {
            if node_values_equal(before_node, after_node, before_src, after_src) {
                HumanOperation::Identical
            } else {
                HumanOperation::Update
            }
        } else {
            subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash)
        };
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

/// Every node's path, in the same `"{kind}:{occurrence}"`-per-level format [`path_for_node`]
/// produces, computed in a single top-down O(n) pass over `root` instead of `path_for_node`'s
/// per-node O(sibling count) backward walk. That per-node cost is invisible for a single lookup,
/// but a node with many same-kind siblings (a big JSON array's elements, a large enum's variants)
/// makes it O(width) *per node at that level*, which a caller that looks up every node's path in a
/// tight loop (like [`action_match_to_end`]) would pay again and again -- this instead assigns
/// each child its 1-indexed occurrence while visiting its parent's children exactly once.
fn precompute_paths(root: Node) -> HashMap<usize, Vec<String>> {
    let mut paths = HashMap::new();
    paths.insert(root.id(), Vec::new());

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let node_path = paths.get(&node.id()).cloned().unwrap_or_default();
        let mut occurrence: HashMap<&str, usize> = HashMap::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let count = occurrence.entry(child.kind()).or_insert(0);
            *count += 1;
            let mut child_path = node_path.clone();
            child_path.push(format!("{}:{}", child.kind(), count));
            paths.insert(child.id(), child_path);
            stack.push(child);
        }
    }

    paths
}

#[allow(clippy::too_many_arguments)]
fn action_match_subtree(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
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
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

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
        return Ok(ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
            before_id: before_node.id(),
            after_id: after_node.id(),
            before_kind: before_node.kind().to_string(),
            after_kind: after_node.kind().to_string(),
            recursive: true,
        }));
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
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
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
        find_node_by_id(before_flat, before_id),
        find_node_by_id(after_flat, after_id),
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
    before_flat: &[(Node, usize)],
    before_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;

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
    after_flat: &[(Node, usize)],
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

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

fn action_unmark(
    mapping: &mut HumanMapping,
    focus: Focus,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (id, node, removed) = match focus {
        Focus::Before => (
            before_cursor,
            find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?,
            &caches.before_removed,
        ),
        Focus::After => (
            after_cursor,
            find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?,
            &caches.after_removed,
        ),
    };

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
    flat: &[(Node, usize)],
    panel: &mut PanelState,
    caches: &Caches,
    side: Side,
    src: &[u8],
    focused: bool,
    algo_diff: Option<&ASTDiff>,
    show_reason: bool,
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    panel.viewport_height = inner_height;
    let cursor_idx = flat
        .iter()
        .position(|(n, _)| n.id() == panel.cursor_id)
        .unwrap_or(0);
    ensure_visible(&mut panel.scroll, cursor_idx, inner_height);

    // Single pass over the whole (potentially multi-thousand-node) flat list: computes each node's
    // status once, both to build the visible page's rows and to total up unmarked nodes for the
    // header, rather than walking `flat` twice and calling status_before/status_after on every
    // node both times.
    let mut total_unmarked = 0usize;
    let mut items: Vec<ListItem> = Vec::with_capacity(inner_height.max(1));
    for (idx, (node, depth)) in flat.iter().enumerate() {
        let status = match side {
            Side::Before => status_before(*node, caches),
            Side::After => status_after(*node, caches),
        };
        if status == NodeStatus::Unmarked {
            total_unmarked += 1;
        }
        if idx < panel.scroll || idx >= panel.scroll + inner_height.max(1) {
            continue;
        }

        let (glyph, mut style) = status_glyph_and_style(status);
        let (algo_glyph, disagrees) = algo_diff
            .map(|diff_ast| {
                let algo_status = match side {
                    Side::Before => algo_status_before(*node, diff_ast),
                    Side::After => algo_status_after(*node, diff_ast),
                };
                let disagrees = match side {
                    Side::Before => algo_disagrees_before(*node, caches, diff_ast),
                    Side::After => algo_disagrees_after(*node, caches, diff_ast),
                };
                let reason_suffix = if show_reason {
                    let reason = match side {
                        Side::Before => algo_reason_before(*node, diff_ast),
                        Side::After => algo_reason_after(*node, diff_ast),
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
        let indent = "  ".repeat(*depth);
        let marker = if disagrees { " *" } else { "" };
        let text = format!(
            "{}{}{} {}{}",
            indent,
            glyph,
            algo_glyph,
            node_label(*node, src),
            marker
        );

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

fn draw_ui(
    frame: &mut Frame,
    app: &mut App,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
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

    frame.render_widget(
        Paragraph::new(format!(" human_solver — {} ", name))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    // Below `SINGLE_PANEL_WIDTH_THRESHOLD` columns, two 50%-wide panels wrap every line and become
    // unreadable, so show only the focused panel at full width instead - `Tab` (which already
    // toggles `app.focus`) becomes the way to see the other side.
    let single_panel = size.width < SINGLE_PANEL_WIDTH_THRESHOLD;

    if single_panel {
        let panel_area = chunks[1];
        let (title, flat, panel, side, src) = match app.focus {
            Focus::Before => (
                "Before",
                before_flat,
                &mut app.before,
                Side::Before,
                before_src,
            ),
            Focus::After => ("After", after_flat, &mut app.after, Side::After, after_src),
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
        );
    }

    let footer = format!(
        "{}{}{}\nm/M match[+children]  f match to EOF  d/D delete[+children]  i/I insert[+children]  a/A align (human/codediff)  p run codediff  r toggle reason  n/N next/prev mismatch  t text view  T unix diff  H hide solved  u unmark  h/l ←/→ collapse/expand  j/k ↑/↓ move  g/G top/bottom  Tab switch  s save  ? help  q quit",
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
            std::str::from_utf8(before_src).unwrap_or(""),
            std::str::from_utf8(after_src).unwrap_or(""),
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

fn render_modal(
    frame: &mut Frame,
    area: Rect,
    modal: &Modal,
    current_name: &str,
    before_src: &str,
    after_src: &str,
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
        Modal::OpenDiffPicker { options, selected } => {
            render_open_picker(frame, area, options, *selected, "diff");
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
                    "'{}' has unsaved changes (it's a sample; promote it with 's' from the main view to save it).\n\nOpen '{}' anyway?\n\n[d] Discard & Open    [Esc] Cancel",
                    current_name,
                    target.name()
                )
            },
        ),
        Modal::PromptPromoteName { input, error } => render_text_modal(
            frame,
            area,
            "Promote sample to test case",
            &format!(
                "Enter a name for src/test/data/diffs/<name>/\n(letters, digits, - and _; must not already exist)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::TextView { scroll } => {
            render_text_view_modal(frame, area, before_src, after_src, *scroll);
        }
        Modal::UnixDiffView { output, scroll } => {
            render_unix_diff_modal(frame, area, output, *scroll);
        }
        Modal::Help { scroll } => {
            render_help_modal(frame, area, *scroll);
        }
    }
}

fn render_text_modal(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    let popup_area = centered_rect(60, 30, area);
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

/// Renders the `t` (text view) modal: the raw before/after source, side by side, as plain text
/// rather than the AST tree -- useful for just reading the code. `scroll` applies to both sides
/// identically, since it's meant for eyeballing roughly-aligned content, not precise per-side
/// navigation.
fn render_text_view_modal(
    frame: &mut Frame,
    area: Rect,
    before_src: &str,
    after_src: &str,
    scroll: u16,
) {
    let popup_area = centered_rect(92, 90, area);
    frame.render_widget(Clear, popup_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(popup_area);

    let block_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new(before_src)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Before (text) — j/k scroll, T diff view, Esc close")
                    .border_style(block_style),
            )
            .scroll((scroll, 0)),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(after_src)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("After (text)")
                    .border_style(block_style),
            )
            .scroll((scroll, 0)),
        columns[1],
    );
}

/// Renders the `T` (unix diff) modal: the already-computed output of `diff -u` between the before
/// and after content, with `+`/`-` lines colored to match the rest of the UI's insert/delete
/// convention and `@@` hunk headers highlighted.
fn render_unix_diff_modal(frame: &mut Frame, area: Rect, output: &str, scroll: u16) {
    let popup_area = centered_rect(92, 90, area);
    frame.render_widget(Clear, popup_area);

    let lines: Vec<Line> = output
        .lines()
        .map(|line| {
            let style = if line.starts_with("+++") || line.starts_with("---") {
                Style::default().add_modifier(Modifier::BOLD)
            } else if line.starts_with('+') {
                Style::default().fg(Color::Green)
            } else if line.starts_with('-') {
                Style::default().fg(Color::Red)
            } else if line.starts_with("@@") {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("unix `diff -u` — j/k scroll, t text view, Esc close")
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
fn render_open_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[String],
    selected: usize,
    kind: &str,
) {
    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = options.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = options
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
            ListItem::new(Line::from(Span::styled(name.clone(), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open {} ({}/{}) — j/k move, Enter open, Esc cancel",
            kind,
            selected + 1,
            options.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

/// Like `render_open_picker`, but for `O`'s sample picker: solved (already-promoted) entries are
/// shown in green with a " - SOLVED" suffix, left out of the list entirely when `hide_solved` is
/// set, and ordered per `sort_order` (cycled by `s` - see `SampleSortOrder`). Each entry also shows
/// its `sample_diff_line_count` in parentheses, so the effect of switching to a diff-size order is
/// visible directly, not just trusted.
fn render_open_sample_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[(String, bool, usize)],
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
        .map(|(i, (name, solved, size))| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else if *solved {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let label = if *solved {
                format!("{name} ({size}) - SOLVED")
            } else {
                format!("{name} ({size})")
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let solved_count = options.iter().filter(|(_, solved, _)| *solved).count();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open sample ({}/{}) — j/k move, Enter open, H {} solved ({} total), s sort: {}, Esc cancel",
            if visible.is_empty() { 0 } else { selected + 1 },
            visible.len(),
            if hide_solved { "show" } else { "hide" },
            solved_count,
            sort_order.label(),
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
    before_flat: Vec<(Node<'a>, usize)>,
    after_flat: Vec<(Node<'a>, usize)>,
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

    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);

    // Recomputed fresh whenever frame state is rebuilt, so `H` can't show a subtree as hidden
    // after it's actually been un-marked, or vice versa.
    let before_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(before_root, &caches, status_before));
    let after_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(after_root, &caches, status_after));

    let before_flat = flatten_visible(before_root, &app.before.collapsed, before_hidden.as_ref());
    let after_flat = flatten_visible(after_root, &app.after.collapsed, after_hidden.as_ref());

    Ok(FrameState {
        before_root,
        after_root,
        before_src,
        after_src,
        caches,
        before_flat,
        after_flat,
    })
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut before: Code,
    mut after: Code,
) -> Result<()> {
    // Whether the on-screen frame reflects the current state. Set whenever something might have
    // changed (a key was handled, a case was switched, the terminal was resized) and cleared right
    // after redrawing. On a pure idle poll timeout -- the common case, since the TUI just sits
    // there most of the time -- nothing is recomputed or redrawn at all, instead of unconditionally
    // rebuilding caches and re-flattening both (possibly multi-thousand-node) trees ~4 times a
    // second regardless of whether the user touched anything. This is also why opening the `o`/`O`
    // picker used to feel slow on a large case: the picker is a modal drawn on top of the same
    // panels, so every idle tick kept recomputing the underlying case's state for no reason even
    // while you were just staring at the picker deciding what to pick.
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            let state = compute_frame_state(&before, &after, app)?;
            // Cloned rather than borrowed from `app`: draw_ui also takes `app: &mut App`, and
            // passing both `app` and `&app.name` as separate arguments to the same call would
            // conflict.
            let current_name = app.name.clone();
            terminal.draw(|f| {
                draw_ui(
                    f,
                    app,
                    &state.before_flat,
                    &state.after_flat,
                    &state.caches,
                    state.before_src,
                    state.after_src,
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

        // Needed to interpret this keystroke against the state as of just before it (cursor
        // position, current marks, etc.); recomputed here rather than reused from the last draw so
        // it's guaranteed fresh even for the very first keystroke after a case switch.
        let state = compute_frame_state(&before, &after, app)?;

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

        let mut open_request: Option<OpenTarget> = None;

        if app.modal.is_some() {
            open_request = handle_modal_key(
                app,
                key.code,
                &state.before_flat,
                &state.after_flat,
                state.before_root,
                state.after_root,
                &state.caches,
                state.before_src,
                state.after_src,
            );
        } else {
            handle_key(
                app,
                key.code,
                &state.before_flat,
                &state.after_flat,
                state.before_root,
                state.after_root,
                &state.caches,
                state.before_src,
                state.after_src,
                before_hash,
                after_hash,
                &before,
                &after,
            );
        }

        needs_redraw = true;

        // Applied after the borrows above (`state`, `before_hash`/`after_hash`, all borrowing from
        // `before`/`after`) have had their last use, so reassigning here is sound: the compiler
        // ends those borrows at last-use, not at the end of the lexical scope.
        match open_request {
            Some(OpenTarget::Diffs(name)) => match load_case(&name) {
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
                    app.status = Some(format!("Opened '{}'", app.name));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening '{}': {:#}", name, err));
                }
            },
            Some(OpenTarget::Sample(name)) => match load_sample(&name) {
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
                    app.status = Some(format!(
                        "Opened sample '{}' (press s to promote it into a test case)",
                        app.name
                    ));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening sample '{}': {:#}", name, err));
                }
            },
            None => {}
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
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
            match action_match(
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
            ) {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(modal),
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
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('M') => {
            match action_match_subtree(
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
            ) {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(modal),
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
        KeyCode::Char('t') => {
            app.modal = Some(Modal::TextView { scroll: 0 });
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
            CaseOrigin::Diffs => Some(action_save(&mut app.mapping, &mut app.dirty, &app.name)),
            CaseOrigin::Sample(_) => {
                app.modal = Some(Modal::PromptPromoteName {
                    input: String::new(),
                    error: None,
                });
                None
            }
        },
        KeyCode::Char('o') => {
            match list_available_cases() {
                Ok(options) if !options.is_empty() => {
                    let selected = options.iter().position(|o| o == &app.name).unwrap_or(0);
                    app.modal = Some(Modal::OpenDiffPicker { options, selected });
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
                    let options: Vec<(String, bool, usize)> = options
                        .into_iter()
                        .map(|(name, solved)| {
                            let size = sample_diff_line_count(&name);
                            (name, solved, size)
                        })
                        .collect();
                    let selected = options
                        .iter()
                        .position(|(name, ..)| name == &app.name)
                        .unwrap_or(0);
                    app.modal = Some(Modal::OpenSamplePicker {
                        options,
                        selected,
                        hide_solved: false,
                        sort_order: SampleSortOrder::Alphabetical,
                    });
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
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
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
        Modal::OpenDiffPicker { options, selected } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::OpenDiffPicker {
                    selected: selected.saturating_sub(1),
                    options,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::OpenDiffPicker {
                    selected: (selected + 1).min(options.len().saturating_sub(1)),
                    options,
                });
            }
            KeyCode::Enter => {
                let target = OpenTarget::Diffs(options[selected].clone());
                if app.dirty {
                    let can_save = matches!(app.origin, CaseOrigin::Diffs);
                    app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                } else {
                    return Some(target);
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::OpenDiffPicker { options, selected });
            }
        },
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
                    app.modal = Some(Modal::OpenSamplePicker {
                        options,
                        selected: 0,
                        hide_solved,
                        sort_order: sort_order.next(),
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
        Modal::ConfirmDiscardUnsaved { target, can_save } => match code {
            KeyCode::Char('s') | KeyCode::Char('S') if can_save => {
                match action_save(&mut app.mapping, &mut app.dirty, &app.name) {
                    Ok(_) => return Some(target),
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
        Modal::TextView { scroll } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::TextView {
                    scroll: scroll.saturating_sub(1),
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::TextView {
                    scroll: scroll.saturating_add(1),
                });
            }
            KeyCode::PageUp => {
                app.modal = Some(Modal::TextView {
                    scroll: scroll.saturating_sub(10),
                });
            }
            KeyCode::PageDown => {
                app.modal = Some(Modal::TextView {
                    scroll: scroll.saturating_add(10),
                });
            }
            KeyCode::Char('T') => match run_unix_diff(before_src, after_src) {
                Ok(output) => app.modal = Some(Modal::UnixDiffView { output, scroll: 0 }),
                Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
            },
            KeyCode::Esc => {
                app.status = Some("Closed text view".to_string());
            }
            _ => {
                app.modal = Some(Modal::TextView { scroll });
            }
        },
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
                app.modal = Some(Modal::TextView { scroll: 0 });
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

fn action_save(mapping: &mut HumanMapping, dirty: &mut bool, name: &str) -> Result<String> {
    human_mapping::save(name, mapping)?;
    let created = ensure_stub_test(name)?;
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

/// A name must be non-empty, start with a letter (so `module_name` -- which just swaps `-` for
/// `_` -- produces a valid Rust identifier) and contain only characters safe to use directly as
/// a directory name.
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

/// Promotes the currently open sample (`app.origin` must be `CaseOrigin::Sample`) into a real
/// test case under `src/test/data/diffs/<new_name>/`: copies the before/after content sitting in
/// `before_src`/`after_src` (the same bytes currently on screen, read once from
/// `src/test/data/samples/<app.name>/` when the sample was opened), saves human_mapping.json and
/// the optimal_solutions stub via the normal `action_save` path, and records `new_name` against
/// the matching row in sample.csv. On success, `app` is switched over to the new diffs/ case so
/// subsequent `s` presses behave like a normal save.
fn action_promote(
    app: &mut App,
    new_name: &str,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<String> {
    let CaseOrigin::Sample(source) = app.origin.clone() else {
        bail!("Current case is not a sample");
    };

    validate_new_case_name(new_name)?;

    let dir = diffs_case_dir(new_name);
    if dir.exists() {
        bail!("'{}' already exists in src/test/data/diffs", new_name);
    }

    let ext = Path::new(&source.path)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("sample path {} has no extension", source.path))?;

    fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
    fs::write(dir.join(format!("before.{ext}.test")), before_src)?;
    fs::write(dir.join(format!("after.{ext}.test")), after_src)?;

    let save_msg = action_save(&mut app.mapping, &mut app.dirty, new_name)?;

    let csv_note = match update_sample_csv(&source, new_name) {
        Ok(true) => String::new(),
        Ok(false) => " (source row not found in sample.csv; not updated)".to_string(),
        Err(err) => format!(" (failed to update sample.csv: {:#})", err),
    };

    app.name = new_name.to_string();
    app.origin = CaseOrigin::Diffs;

    Ok(format!(
        "Promoted to '{}'. {}{}",
        new_name, save_msg, csv_note
    ))
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
}

fn update_sample_csv(source: &SampleSource, new_name: &str) -> Result<bool> {
    update_sample_csv_at(&sample_csv_path(), source, new_name)
}

/// Marks the sample.csv row matching `source` as promoted to `new_name`, preserving every other
/// row and column untouched. Returns `Ok(false)` (not an error) if no row matches -- e.g. the
/// sample was placed under samples/ by hand rather than by `sample_test_diffs` -- since that
/// shouldn't undo a promotion that has already otherwise succeeded.
fn update_sample_csv_at(path: &Path, source: &SampleSource, new_name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut reader = csv::Reader::from_path(path).with_context(|| format!("reading {:?}", path))?;
    let mut rows: Vec<SampleCsvRow> = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(SampleCsvRow {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to: record.get(4).unwrap_or("").to_string(),
        });
    }

    let mut found = false;
    for row in &mut rows {
        if row.language == source.language
            && row.repository == source.repository
            && row.commit == source.commit
            && row.path == source.path
        {
            row.promoted_to = new_name.to_string();
            found = true;
        }
    }

    if !found {
        return Ok(false);
    }

    let mut writer = csv::Writer::from_path(path).with_context(|| format!("writing {:?}", path))?;
    writer.write_record(["language", "repository", "commit", "path", "promoted_to"])?;
    for row in &rows {
        writer.write_record([
            &row.language,
            &row.repository,
            &row.commit,
            &row.path,
            &row.promoted_to,
        ])?;
    }
    writer.flush()?;
    Ok(true)
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

fn optimal_solutions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions")
}

fn optimal_solutions_mod_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions.rs")
}

/// Creates `optimal_solutions/<name>.rs` if it doesn't already exist, and makes sure it's
/// registered in `optimal_solutions.rs`. Returns whether the stub `.rs` file was newly created.
fn ensure_stub_test(name: &str) -> Result<bool> {
    let module = module_name(name);
    let stub_path = optimal_solutions_dir().join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        let contents = format!(
            "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test;\n\n#[test]\nfn optimal_solution() -> Result<()> {{\n    test::helper::human_mapping::assert_matches_human_mapping(\"{name}\")\n}}\n"
        );
        fs::write(&stub_path, contents)
            .with_context(|| format!("writing stub test to {:?}", stub_path))?;
        true
    };

    insert_mod_declaration(&module)?;

    Ok(created)
}

/// Adds `#[cfg(test)]\nmod <module>;` to optimal_solutions.rs, keeping the list sorted, unless
/// it's already present.
fn insert_mod_declaration(module: &str) -> Result<()> {
    let mod_file = optimal_solutions_mod_file();
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
                render_text_view_modal(f, area, "fn old_name() {}", "fn new_name() {}", 0);
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
    fn open_sample_picker_marks_solved_entries_and_can_hide_them() {
        let options = vec![
            ("rust-x-foo-abc12345-a".to_string(), true, 7),
            ("rust-x-foo-def67890-b".to_string(), false, 3),
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
            text.contains("rust-x-foo-def67890-b (3)"),
            "unsolved entry missing: {text}"
        );
        assert!(
            text.contains("1/2"),
            "count should include both entries: {text}"
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
        let options = vec![("a".to_string(), false, 1)];
        let backend = ratatui::backend::TestBackend::new(160, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 160, 24);

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
            ("charlie".to_string(), false, 5),
            ("alpha".to_string(), false, 20),
            ("bravo".to_string(), false, 1),
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
        // with a solved entry hidden, `selected` indexes the *filtered* list, so index 1 here must
        // resolve to "unsolved-two" (the second visible entry), not "unsolved-one" (index 1 in the
        // unfiltered `options`) or the hidden "solved-one".
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
        let flat = flatten_visible(root, &app.before.collapsed, None);
        app.modal = Some(Modal::OpenSamplePicker {
            options: vec![
                ("solved-one".to_string(), true, 0),
                ("unsolved-one".to_string(), false, 0),
                ("unsolved-two".to_string(), false, 0),
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
        let flat = flatten_visible(root, &app.before.collapsed, None);
        app.modal = Some(Modal::OpenSamplePicker {
            options: vec![
                ("alpha".to_string(), false, 5),
                ("bravo".to_string(), false, 1),
                ("charlie".to_string(), false, 20),
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
        let before_flat = flatten_visible(before_root, &app.before.collapsed, None);
        let after_flat = flatten_visible(after_root, &app.after.collapsed, None);
        let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);

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
        let backend = ratatui::backend::TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 140, 40);

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
    fn flatten_visible_skips_hidden_subtree_entirely_but_keeps_siblings() {
        let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
        let root = tree.root_node();
        let (stmt_a, stmt_b) = two_statements(root);

        let mut hidden = std::collections::HashSet::new();
        hidden.insert(stmt_a.id());

        let flat = flatten_visible(root, &std::collections::HashSet::new(), Some(&hidden));

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
        let before_flat = flatten_visible(before_root, &app.before.collapsed, None);
        let after_flat = flatten_visible(after_root, &app.after.collapsed, None);
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
        for (node, _) in &before_flat {
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
        let before_flat = flatten_visible(before_root, &app.before.collapsed, None);
        let after_flat = flatten_visible(after_root, &app.after.collapsed, None);
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
            ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
                before_id,
                after_id,
                before_kind,
                after_kind,
                recursive,
            }) => {
                assert!(
                    !recursive,
                    "f should raise a single-pair mismatch, not a recursive one"
                );
                (before_id, after_id, before_kind, after_kind)
            }
            ActionOutcome::Done(msg) => {
                panic!("expected a kind mismatch modal, action completed instead: {msg}")
            }
            ActionOutcome::NeedsModal(other) => {
                panic!("expected ConfirmKindMismatch, got {other:?}")
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
        let before_flat = flatten_visible(before_root, &app.before.collapsed, None);
        let after_flat = flatten_visible(after_root, &app.after.collapsed, None);
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
            ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
                before_kind,
                after_kind,
                ..
            }) => {
                assert_ne!(before_kind, after_kind);
            }
            ActionOutcome::Done(msg) => panic!(
                "expected the sweep to stop on a kind mismatch once `c();` has nothing left to pair \
                 with, action completed instead: {msg}"
            ),
            ActionOutcome::NeedsModal(other) => {
                panic!("expected ConfirmKindMismatch, got {other:?}")
            }
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

        let before_flat = flatten_visible(before_root, &std::collections::HashSet::new(), None);
        let after_flat = flatten_visible(after_root, &std::collections::HashSet::new(), None);
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

        let before_flat = flatten_visible(before_root, &std::collections::HashSet::new(), None);
        let after_flat = flatten_visible(after_root, &std::collections::HashSet::new(), None);
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
        };

        let function_before = before_root.child(0).unwrap();
        let function_after = after_root.child(0).unwrap();
        let before_flat = flatten_visible(before_root, &std::collections::HashSet::new(), None);
        let after_flat = flatten_visible(after_root, &std::collections::HashSet::new(), None);
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
        };

        let function_before = before_root.child(0).unwrap();
        let function_after = after_root.child(0).unwrap();
        let before_flat = flatten_visible(before_root, &std::collections::HashSet::new(), None);
        let after_flat = flatten_visible(after_root, &std::collections::HashSet::new(), None);
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

    fn write_csv(path: &Path, rows: &[(&str, &str, &str, &str, &str)]) {
        let mut writer = csv::Writer::from_path(path).unwrap();
        writer
            .write_record(["language", "repository", "commit", "path", "promoted_to"])
            .unwrap();
        for (language, repository, commit, row_path, promoted_to) in rows {
            writer
                .write_record([language, repository, commit, row_path, promoted_to])
                .unwrap();
        }
        writer.flush().unwrap();
    }

    fn read_csv(path: &Path) -> Vec<(String, String)> {
        let mut reader = csv::Reader::from_path(path).unwrap();
        reader
            .records()
            .map(|r| {
                let r = r.unwrap();
                (r[3].to_string(), r.get(4).unwrap_or("").to_string())
            })
            .collect()
    }

    #[test]
    fn update_sample_csv_sets_promoted_to_on_the_matching_row_only() {
        let file = NamedTempFile::new().unwrap();
        write_csv(
            file.path(),
            &[
                ("Rust", "repo", "abc123", "src/a.rs", ""),
                ("Rust", "repo", "def456", "src/b.rs", ""),
            ],
        );

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
        };
        let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
        assert!(found);

        let rows = read_csv(file.path());
        assert_eq!(
            rows,
            vec![
                ("src/a.rs".to_string(), "rust-new-case".to_string()),
                ("src/b.rs".to_string(), "".to_string()),
            ]
        );
    }

    #[test]
    fn update_sample_csv_returns_false_when_no_row_matches() {
        let file = NamedTempFile::new().unwrap();
        write_csv(file.path(), &[("Rust", "repo", "abc123", "src/a.rs", "")]);

        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "other-repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
        };
        let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
        assert!(!found);

        // Untouched: no row matched, so nothing should have been rewritten.
        let rows = read_csv(file.path());
        assert_eq!(rows, vec![("src/a.rs".to_string(), "".to_string())]);
    }

    #[test]
    fn update_sample_csv_returns_false_when_file_does_not_exist() {
        let source = SampleSource {
            language: "Rust".to_string(),
            repository: "repo".to_string(),
            commit: "abc123".to_string(),
            path: "src/a.rs".to_string(),
        };
        let found =
            update_sample_csv_at(Path::new("/nonexistent/sample.csv"), &source, "name").unwrap();
        assert!(!found);
    }

    #[test]
    fn promoted_sample_sources_at_only_includes_rows_with_a_non_empty_promoted_to() {
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
                ),
                ("Rust", "repo", "def456", "src/b.rs", ""),
            ],
        );

        let promoted = promoted_sample_sources_at(file.path()).unwrap();
        assert_eq!(promoted.len(), 1);
        assert!(promoted.contains(&(
            "Rust".to_string(),
            "repo".to_string(),
            "abc123".to_string(),
            "src/a.rs".to_string(),
        )));
    }

    #[test]
    fn promoted_sample_sources_at_is_empty_when_file_does_not_exist() {
        let promoted = promoted_sample_sources_at(Path::new("/nonexistent/sample.csv")).unwrap();
        assert!(promoted.is_empty());
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
            reason_label(ASTMappingReason::FullymappingSubtrees),
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
        assert_eq!(reason_label(ASTMappingReason::CommentSibling), "Comment");
        assert_eq!(
            reason_label(ASTMappingReason::BottomUpExpansion),
            "BottomUp"
        );
        assert_eq!(
            reason_label(ASTMappingReason::GreedyAnchorBlock),
            "GreedyAnchor"
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
            reason_detail(ASTMappingReason::BottomUpExpansion),
            "BottomUp"
        );
    }
}
