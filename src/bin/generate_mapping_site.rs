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

//! Renders every fixture's `human_mapping.json` (see `src/bin/human_solver.rs`) as a static,
//! read-only HTML page - two side-by-side before/after trees with the human-authored
//! matched/deleted/inserted annotations baked in as `data-*` attributes, click-to-highlight
//! cross-panel navigation, and a "file an issue" button - all driven by one hand-written vanilla
//! JS file (`assets/mapping_site/viewer.js`), no framework, no server. Meant to be published to
//! GitHub Pages by `.github/workflows/pages.yml`; nothing this binary produces is committed to the
//! repo.
//!
//! This is purely for humans to review and discuss what the ground-truth mapping itself should
//! be - it never runs codediff's own diff or compares against it, unlike `benchmark_optimal_solutions`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tree_sitter::Node;

use codediff::code::{Code, Language};
use codediff::test::helper;
use codediff::test::helper::human_mapping::{
    self, Caches, HumanOperation, MarkKind, NodeStatus, is_identical_after, is_identical_before,
    is_moved_after, is_moved_before, match_operation_after, match_operation_before,
    rebuild_caches_for_mapping, status_after, status_before,
};
// Only used by this file's own test module (`rebuild_caches_for_mapping`, imported above, is the
// one the non-test code path uses).
#[cfg(test)]
use codediff::test::helper::human_mapping::rebuild_caches;

/// `owner/repo`, used both for the "file an issue" link (rewritten client-side in viewer.js) and
/// the "view source" link below (baked in at generation time, since it's static per fixture).
const REPO: &str = "ivankovic/codediff";

#[derive(Parser)]
struct Args {
    /// Directory to write the generated site into. Wiped and recreated on every run.
    #[arg(long, default_value = "site")]
    out: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.out.exists() {
        fs::remove_dir_all(&args.out)
            .with_context(|| format!("removing existing {:?}", args.out))?;
    }
    let fixtures_dir = args.out.join("fixtures");
    let assets_dir = args.out.join("assets");
    fs::create_dir_all(&fixtures_dir)?;
    fs::create_dir_all(&assets_dir)?;

    // Embedded at compile time rather than copied from disk at generation time, so the generator
    // is a single self-contained binary - nothing else needs to ship alongside it in CI.
    fs::write(
        assets_dir.join("style.css"),
        include_str!("../../assets/mapping_site/style.css"),
    )?;
    fs::write(
        assets_dir.join("viewer.js"),
        include_str!("../../assets/mapping_site/viewer.js"),
    )?;
    fs::write(
        assets_dir.join("index.js"),
        include_str!("../../assets/mapping_site/index.js"),
    )?;

    let pairs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<&String> = pairs.keys().collect();
    names.sort();

    let mut index_entries: Vec<IndexEntry> = Vec::new();
    let mut skipped = 0usize;

    for name in names {
        let (before, after) = &pairs[name];
        let mapping = match human_mapping::load(name) {
            Ok(mapping) => mapping,
            // No human_mapping.json yet (e.g. a sample never promoted, or promoted but not yet
            // solved) - not every fixture directory necessarily has one, and that's not an error.
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let page = render_fixture_page(name, before, after, &mapping)?;
        fs::write(fixtures_dir.join(format!("{name}.html")), page)
            .with_context(|| format!("writing page for '{name}'"))?;

        let language = before.metadata.language.unwrap_or_default();
        // Line-level, not the AST-node-level count `assert_matches_human_mapping` checks -
        // see `human_mapping::line_mismatches_for`'s own doc comment for why: it's the only
        // granularity Unix `diff` (which has no notion of an AST node) can be scored at all, so
        // it's what lets these two columns sit side by side and mean the same thing.
        // `_for_mapping`, not `line_mismatches_for(name, ...)`: `mapping` is already loaded above
        // for `render_fixture_page` - re-loading (and re-JSON-parsing) the same file a second time
        // per fixture would be pure waste across the ~175-fixture corpus.
        let mismatches = human_mapping::line_mismatches_for_mapping(&mapping, before, after)
            .with_context(|| format!("computing line mismatches for '{name}'"))?;
        index_entries.push(IndexEntry {
            name: name.clone(),
            language,
            codediff_mismatches: mismatches.codediff,
            unix_diff_mismatches: mismatches.unix_diff,
            total_lines: mismatches.total_lines,
        });
    }

    fs::write(
        args.out.join("index.html"),
        render_index_page(&index_entries),
    )?;

    println!(
        "Generated {} fixture page(s) into {:?} ({} skipped, no human_mapping.json)",
        index_entries.len(),
        args.out,
        skipped
    );
    Ok(())
}

fn render_fixture_page(
    name: &str,
    before: &Code,
    after: &Code,
    mapping: &human_mapping::HumanMapping,
) -> Result<String> {
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

    let caches = rebuild_caches_for_mapping(mapping, before_root, after_root);
    // Most fixtures' human_mapping.json only annotates a few hundred nodes out of many thousands
    // (see human_mapping_cost's own doc comment) - the rest is untouched code the human considered
    // unchanged - but a few fixtures (e.g. auto-generated files matched near-exhaustively) instead
    // carry an explicit `Matched` entry for nearly every node. Either way, rendering every node in
    // full made the biggest fixtures' pages multi-megabyte (measured up to 16MB) for no reason:
    // `render_node` uses these sizes to omit large "quiet" subtrees - no deletion or insertion
    // anywhere inside them, whether unannotated or explicitly confirmed matched - behind a
    // placeholder (small ones still render in full, just closed by default), keeping the
    // actually-interesting (edited) parts front and center. Every node's path (used only by the
    // "file an issue" button) is deliberately *not* precomputed and baked in here - `viewer.js`
    // derives it lazily, client-side, only for whichever single node gets clicked, since baking a
    // `data-path` string into every node measurably added to that same page-size problem (over a
    // third of a node's own markup on a representative fixture).
    let before_quiet_sizes =
        fully_quiet_subtree_sizes(before_root, &caches, status_before, is_identical_before);
    let after_quiet_sizes =
        fully_quiet_subtree_sizes(after_root, &caches, status_after, is_identical_after);

    let before_html = render_node(
        before_root,
        before.contents.as_bytes(),
        'b',
        &caches,
        &before_quiet_sizes,
        true,
    );
    let after_html = render_node(
        after_root,
        after.contents.as_bytes(),
        'a',
        &caches,
        &after_quiet_sizes,
        true,
    );

    let language = before.metadata.language.unwrap_or_default();
    // `diffs_case_dir` resolves which of `handmade`/`small`/`full` this fixture actually lives
    // under (see `helper::DIFF_DATASETS`) - the URL needs that segment, even though every other
    // parameter here is already in memory and doesn't otherwise touch disk.
    let dataset = helper::diffs_case_dir(name)
        .and_then(|dir| {
            dir.parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "small".to_string());
    let source_url = format!(
        "https://github.com/{REPO}/tree/main/src/test/data/diffs/{}/{}",
        dataset,
        escape_html_attr(name)
    );

    Ok(format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{name_escaped} — human mapping</title>
<link rel="stylesheet" href="../assets/style.css">
</head>
<body data-fixture="{name_attr}" data-repo="{repo}">
<header class="page-header">
<a class="back-link" href="../index.html">&larr; all fixtures</a>
<h1>{name_escaped}</h1>
<span class="language-badge">{language}</span>
<a class="source-link" href="{source_url}" target="_blank" rel="noopener">View before/after files on GitHub</a>
<ul class="legend">
<li><span class="status-unmarked">&#9679;</span> unmarked</li>
<li><span class="status-matched">&#9679;</span> matched</li>
<li><span class="status-matched op-update changed">&#9679;</span> updated</li>
<li><span class="status-matched changed">&#9679;</span> matched, not identical</li>
<li><span class="status-matched op-moved">&#9679;</span> moved, unchanged</li>
<li><span class="status-deleted">&#9679;</span> deleted</li>
<li><span class="status-inserted">&#9679;</span> inserted</li>
</ul>
</header>
<div class="panels">
<section class="panel" data-side="before">
<h2>Before</h2>
<div class="tree">{before_html}</div>
</section>
<section class="panel" data-side="after">
<h2>After</h2>
<div class="tree">{after_html}</div>
</section>
</div>
<footer class="page-footer">
<a id="file-issue" href="#" aria-disabled="true" target="_blank" rel="noopener">File an issue about the selected node</a>
<button id="toggle-identical" type="button" aria-pressed="false">Hide identical matches</button>
<span id="status-line" role="status"></span>
</footer>
{help_overlay}
{search_prompt}
<script src="../assets/viewer.js"></script>
</body>
</html>
"##,
        name_escaped = escape_html_text(name),
        name_attr = escape_html_attr(name),
        repo = REPO,
        help_overlay = HELP_OVERLAY_HTML,
        search_prompt = SEARCH_PROMPT_HTML,
    ))
}

const HELP_OVERLAY_HTML: &str = r#"<div id="help-overlay" class="hidden" role="dialog" aria-label="Keybindings">
<h2>Keybindings</h2>
<dl>
<dt>j / k</dt><dd>next / previous visible node</dd>
<dt>h / l</dt><dd>collapse / expand the focused node</dd>
<dt>g / G</dt><dd>jump to first / last visible node</dd>
<dt>Tab</dt><dd>switch focus between Before/After panels</dd>
<dt>/</dt><dd>search: jump to next node whose text contains a given string</dd>
<dt>a</dt><dd>jump the other panel to this node's mapped counterpart</dd>
<dt>i</dt><dd>hide identical matches, showing only inserted/deleted/updated nodes and their ancestors</dd>
<dt>?</dt><dd>toggle this help</dd>
</dl>
</div>"#;

const SEARCH_PROMPT_HTML: &str = r#"<div id="search-prompt" class="hidden" role="dialog" aria-label="Search">
<label for="search-input">Search (plain substring, no regex):</label>
<input id="search-input" type="text" autocomplete="off">
</div>"#;

/// A fully-quiet subtree (see `fully_quiet_subtree_sizes`) bigger than this many nodes is omitted
/// from the HTML entirely (replaced with a one-line placeholder) rather than just collapsed -
/// collapsing via `<details>` without `open` still serializes the full subtree into the page (a
/// closed `<details>` is `display: none`, not "absent"), so on the corpus's biggest fixtures -
/// either almost entirely untouched code around a small annotated diff, or a huge block explicitly
/// matched node-for-node - collapsing alone still produced multi-megabyte pages (measured up to
/// 16MB). Small fully-quiet subtrees (at or under this size) still render in full, just closed by
/// default, so a reader can still drill into an ordinary short unchanged/matched statement without
/// hitting the placeholder wall constantly.
const OMIT_THRESHOLD: usize = 20;

/// Recursively renders `node` and its subtree. `side` is `'b'` (before) or `'a'` (after) - used
/// both as the id-namespace prefix (so before/after tree-sitter node ids, which can collide in
/// value between the two independently-parsed trees, never collide in the DOM) and to pick which
/// half of `caches` every per-side lookup below reads from (`status_before`/`status_after`,
/// `is_identical_before`/`_after`, `match_operation_before`/`_after`, `is_moved_before`/`_after`) -
/// each an `if side == 'b' { ... } else { ... }` inline, not a caller-selected function pointer:
/// `fully_quiet_subtree_sizes`/`mark_fully_quiet` still take `status_fn`/`identical_fn` as function
/// pointers (they have no `side` of their own to dispatch on - they're a separate, whole-tree pass
/// that runs *before* `render_node`, over one side at a time), but within `render_node` itself
/// `side` is always in scope, so there is no reason for two different dispatch conventions in one
/// function. `quiet_sizes` (see `fully_quiet_subtree_sizes`) maps a fully-quiet node's id to its
/// subtree's node count; `force_open` overrides both the closed-by-default and the
/// omit-with-placeholder treatment for `node` itself (but not its descendants) - used to keep the
/// tree root fully rendered and open even on the rare fixture where it happens to be entirely
/// quiet (otherwise the page would load empty or collapsed).
fn render_node(
    node: Node,
    src: &[u8],
    side: char,
    caches: &Caches,
    quiet_sizes: &HashMap<usize, usize>,
    force_open: bool,
) -> String {
    let status = match side {
        'b' => status_before(node, caches),
        _ => status_after(node, caches),
    };
    // Folds `NodeStatus`'s with-children/inherited distinction down to one of four colors: that
    // distinction matters for `human_solver`'s editing workflow (has this exact node been marked,
    // or only an ancestor), but a read-only viewer just needs "is this deleted", not why.
    let (status_class, matched_other_id) = match status {
        NodeStatus::Unmarked => ("unmarked", None),
        NodeStatus::Matched => (
            "matched",
            match side {
                'b' => caches.before_match.get(&node.id()),
                _ => caches.after_match.get(&node.id()),
            },
        ),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            ..
        } => ("deleted", None),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            ..
        } => ("inserted", None),
    };
    // A second, independent class on top of `status_class`: a `Matched` pair can still be a real
    // edit (`Update`/`MatchButNotIdentical`), which the "hide identical matches" toggle needs to
    // keep visible right alongside deleted/inserted nodes, unlike a genuinely-`Identical` match.
    let is_identical = match side {
        'b' => is_identical_before(node, caches),
        _ => is_identical_after(node, caches),
    };
    let changed_class = if status == NodeStatus::Matched && !is_identical {
        " changed"
    } else {
        ""
    };
    // A third, independent class that picks out *which* kind of matched pair this is, for
    // color - `changed_class` above only says "not identical", not why. `Update` (a leaf whose
    // text changed) gets its own color regardless of `changed_class`; a genuinely-`Identical` pair
    // whose `before_path`/`after_path` differ (moved to a different position without any content
    // change - see `Caches::before_moved`) gets a different one still, even though it's *not*
    // `changed_class` (its content is identical, so it stays hidden by the "hide identical
    // matches" toggle same as any other identical match - only its color differs).
    let (operation, moved) = match side {
        'b' => (
            match_operation_before(node, caches),
            is_moved_before(node, caches),
        ),
        _ => (
            match_operation_after(node, caches),
            is_moved_after(node, caches),
        ),
    };
    let operation_class = match operation {
        Some(HumanOperation::Update) => " op-update",
        Some(HumanOperation::Identical) if moved => " op-moved",
        _ => "",
    };

    let other_side = if side == 'b' { 'a' } else { 'b' };
    let id_attr = format!("{side}-{}", node.id());
    let match_attr = matched_other_id
        .map(|&other_id| format!(" data-match=\"{other_side}-{other_id}\""))
        .unwrap_or_default();
    let kind_attr = escape_html_attr(node.kind());

    let quiet_size = quiet_sizes.get(&node.id()).copied();

    if !force_open && quiet_size.is_some_and(|size| size > OMIT_THRESHOLD) {
        let size = quiet_size.unwrap();
        let kind_label = escape_html_text(node.kind());
        // Keeps the real status class/data-match (computed above) rather than hardcoding
        // "unmarked": a placeholder can just as well be the root of a huge *matched* block (an
        // exhaustively-annotated fixture), in which case it still has a real counterpart worth
        // linking to, even though its individual descendants aren't in the DOM to link to
        // themselves. Never carries `changed_class` in practice - `fully_quiet_subtree_sizes` only
        // treats a `Matched` node as quiet (and thus placeholder-eligible) when it's identical -
        // but `operation_class` can still be `op-moved` here (a whole subtree relocated intact).
        return format!(
            r#"<div class="node leaf status-{status_class}{changed_class}{operation_class} placeholder" id="{id_attr}"{match_attr} data-kind="{kind_attr}" tabindex="0">{kind_label} (+{size} nodes collapsed)</div>"#
        );
    }

    if node.child_count() == 0 {
        let label = escape_html_text(&leaf_label(node, src));
        format!(
            r#"<div class="node leaf status-{status_class}{changed_class}{operation_class}" id="{id_attr}"{match_attr} data-kind="{kind_attr}" tabindex="0">{label}</div>"#
        )
    } else {
        let mut children = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            children.push_str(&render_node(child, src, side, caches, quiet_sizes, false));
        }
        let kind_label = escape_html_text(node.kind());
        let open_attr = if force_open || quiet_size.is_none() {
            " open"
        } else {
            ""
        };
        format!(
            r#"<details class="node status-{status_class}{changed_class}{operation_class}" id="{id_attr}"{match_attr} data-kind="{kind_attr}"{open_attr}><summary tabindex="0">{kind_label}</summary>{children}</details>"#
        )
    }
}

/// Same convention as `human_solver`'s `node_label`: kind plus a truncated, `Debug`-quoted
/// (Rust-escaped) snippet of the node's own text.
fn leaf_label(node: Node, src: &[u8]) -> String {
    let text = node.utf8_text(src).unwrap_or("");
    let truncated: String = text.chars().take(60).collect();
    let ellipsis = if text.chars().count() > 60 { "..." } else { "" };
    format!("{} {:?}{}", node.kind(), truncated, ellipsis)
}

/// Whether `node` (given its already-computed `status`) is "quiet": neither a deletion nor an
/// insertion, and, if matched, actually `Identical` rather than a real edit that merely stayed
/// paired (`Update`/`MatchButNotIdentical`, per `identical_fn`). Covers `Unmarked` (never
/// annotated - most of a typical fixture) unconditionally, since there's nothing to check there -
/// a node in either state has nothing actively being edited at it, which is the only thing that
/// actually needs to stay visible by default. Excludes `Marked { kind: Deleted | Inserted, .. }`
/// (exactly the content a reviewer needs to see) and a non-identical `Matched` node (an edit that
/// just happens to still be pairable, e.g. a changed string literal) for the same reason - it must
/// never be swallowed into an "unremarkable" placeholder alongside genuinely untouched code.
fn is_quiet(
    node: Node,
    caches: &Caches,
    status: NodeStatus,
    identical_fn: fn(Node, &Caches) -> bool,
) -> bool {
    match status {
        NodeStatus::Marked {
            kind: MarkKind::Deleted | MarkKind::Inserted,
            ..
        } => false,
        NodeStatus::Matched => identical_fn(node, caches),
        NodeStatus::Unmarked => true,
    }
}

/// Maps a node's id to its subtree's node count (itself plus every descendant), for every node
/// whose entire subtree is quiet (see `is_quiet`) - no deletion, insertion, or non-identical match
/// anywhere inside it, whether because nothing was ever annotated there or because everything in
/// it was explicitly confirmed matched *and* identical. The inverse-polarity counterpart of
/// `human_solver`'s own `fully_solved_nodes` (which finds subtrees that are entirely *marked*, to
/// hide during active editing) - kept as a separate, generator-local function rather than unified
/// with it, since the two serve different purposes for different audiences and only coincidentally
/// share a shape. The size is `render_node`'s to decide whether a fully-quiet subtree is small
/// enough to still render in full (just closed by default) or big enough to omit outright behind a
/// placeholder (see `OMIT_THRESHOLD`).
fn fully_quiet_subtree_sizes(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    identical_fn: fn(Node, &Caches) -> bool,
) -> HashMap<usize, usize> {
    let mut sizes = HashMap::new();
    mark_fully_quiet(root, caches, status_fn, identical_fn, &mut sizes);
    sizes
}

/// Post-order: returns `Some(subtree size)` if `node`'s own subtree is fully quiet (recording it
/// in `sizes` too), `None` otherwise.
fn mark_fully_quiet(
    node: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    identical_fn: fn(Node, &Caches) -> bool,
    sizes: &mut HashMap<usize, usize>,
) -> Option<usize> {
    let mut cursor = node.walk();
    let mut all_children_quiet = true;
    let mut subtree_size = 1usize;
    for child in node.children(&mut cursor) {
        match mark_fully_quiet(child, caches, status_fn, identical_fn, sizes) {
            Some(child_size) => subtree_size += child_size,
            None => all_children_quiet = false,
        }
    }

    let quiet = all_children_quiet && is_quiet(node, caches, status_fn(node, caches), identical_fn);
    if quiet {
        sizes.insert(node.id(), subtree_size);
        Some(subtree_size)
    } else {
        None
    }
}

/// One row of the index page's sortable table - `main`'s corpus loop builds one of these per
/// fixture that has a `human_mapping.json`, computing `codediff_mismatches`/`unix_diff_mismatches`
/// via `human_mapping::line_mismatches_for` alongside the page it already renders for that
/// fixture, so the index page doesn't need a second pass over the corpus.
struct IndexEntry {
    name: String,
    language: Language,
    /// Line-level mismatches against the human mapping - see `human_mapping::LineMismatches`.
    codediff_mismatches: usize,
    unix_diff_mismatches: usize,
    total_lines: usize,
}

fn render_index_page(entries: &[IndexEntry]) -> String {
    let mut rows = String::new();
    for entry in entries {
        let name_attr = escape_html_attr(&entry.name);
        let name_escaped = escape_html_text(&entry.name);
        rows.push_str(&format!(
            r#"<tr data-name="{name_attr}" data-language="{language}" data-codediff="{codediff}" data-unix_diff="{unix_diff}" data-total_lines="{total_lines}">
<td><a href="fixtures/{name_attr}.html">{name_escaped}</a></td>
<td><span class="language-badge">{language}</span></td>
<td>{codediff}</td>
<td>{unix_diff}</td>
<td>{total_lines}</td>
</tr>
"#,
            language = entry.language,
            codediff = entry.codediff_mismatches,
            unix_diff = entry.unix_diff_mismatches,
            total_lines = entry.total_lines,
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>codediff human mappings</title>
<link rel="stylesheet" href="assets/style.css">
</head>
<body>
<header class="page-header">
<h1>Human-authored ground-truth mappings</h1>
<p>Each page below shows one fixture's before/after AST, annotated with what a human decided
should match, get deleted, or get inserted. Disagree with one? Open the fixture, select the node,
and use the "file an issue" button.</p>
<p>"codediff mismatches"/"unix diff mismatches" are line-level disagreements against the human
mapping (see the introductory paper for why line granularity, not AST-node granularity, is the only
fair way to compare codediff against a line-only tool like Unix <code>diff</code>) - click a column
header to sort by it.</p>
</header>
<table class="fixture-table" id="fixture-table">
<thead>
<tr>
<th data-sort="name" data-type="string" tabindex="0" aria-sort="ascending">Fixture</th>
<th data-sort="language" data-type="string" tabindex="0" aria-sort="none">Language</th>
<th data-sort="codediff" data-type="number" tabindex="0" aria-sort="none">codediff mismatches</th>
<th data-sort="unix_diff" data-type="number" tabindex="0" aria-sort="none">Unix diff mismatches</th>
<th data-sort="total_lines" data-type="number" tabindex="0" aria-sort="none">Total lines</th>
</tr>
</thead>
<tbody>
{rows}</tbody>
</table>
<script src="assets/index.js"></script>
</body>
</html>
"#
    )
}

fn escape_html_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(s: &str) -> String {
    escape_html_text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper::human_mapping::{HumanMapping, HumanMappingEntry, HumanOperation};

    #[test]
    fn escape_html_text_escapes_the_three_html_metacharacters_but_not_quotes() {
        assert_eq!(
            escape_html_text("a < b && c > d \"quoted\""),
            "a &lt; b &amp;&amp; c &gt; d \"quoted\""
        );
    }

    #[test]
    fn escape_html_attr_also_escapes_double_quotes() {
        assert_eq!(
            escape_html_attr("say \"hi\" <b>"),
            "say &quot;hi&quot; &lt;b&gt;"
        );
    }

    #[test]
    fn render_fixture_page_links_to_the_fixtures_directory_in_this_repo() {
        let source = "fn f() {}\n";
        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);
        let mapping = HumanMapping {
            entries: vec![],
            ..Default::default()
        };

        let html =
            render_fixture_page("rust-add-if", &before, &after, &mapping).expect("should render");

        assert!(
            html.contains(
                r#"href="https://github.com/ivankovic/codediff/tree/main/src/test/data/diffs/handmade/rust-add-if""#
            ),
            "expected a link straight to this fixture's own before/after files: {html}"
        );
    }

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let language =
            codediff::code::language::to_treesitter(&codediff::code::Language::Rust).unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn render_node_emits_a_leaf_div_and_a_nonleaf_details_with_nested_children() {
        let source = "fn f() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let caches = Caches::default();

        let html = render_node(root, source.as_bytes(), 'b', &caches, &HashMap::new(), true);

        assert!(
            html.starts_with(r#"<details class="node status-unmarked" id="b-"#),
            "root (non-leaf) should render as a <details>: {html}"
        );
        assert!(
            html.contains(r#"<div class="node leaf status-unmarked""#),
            "a leaf token (e.g. the 'fn' keyword) should render as a <div>: {html}"
        );
        assert!(html.contains(">fn \"fn\"<"), "leaf label missing: {html}");
        // details/summary/div must each be balanced - a common bug class in hand-rolled recursive
        // HTML generation is an off-by-one closing tag on one recursion path but not another.
        for tag in ["details", "summary", "div"] {
            let opens = html.matches(&format!("<{tag}")).count();
            let closes = html.matches(&format!("</{tag}>")).count();
            assert_eq!(
                opens, closes,
                "{tag}: {opens} opens vs {closes} closes in {html}"
            );
        }
    }

    #[test]
    fn render_node_marks_matched_nodes_with_a_data_match_pointing_at_the_other_side() {
        let source = "fn f() {}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(vec![]),
                after_path: Some(vec![]),
            }],
            ..Default::default()
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);

        let before_html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            &HashMap::new(),
            true,
        );

        let expected_id = format!("id=\"b-{}\"", before_root.id());
        let expected_match = format!("data-match=\"a-{}\"", after_root.id());
        assert!(
            before_html.contains(&expected_id),
            "expected {expected_id} in {before_html}"
        );
        assert!(
            before_html.contains("status-matched"),
            "matched root should get the matched status class: {before_html}"
        );
        assert!(
            before_html.contains(&expected_match),
            "expected {expected_match} in {before_html}"
        );
        assert!(
            !before_html.contains("changed"),
            "an Identical match must not get the changed class: {before_html}"
        );
    }

    #[test]
    fn render_node_marks_a_non_identical_matched_node_as_changed_with_its_own_operation_class() {
        let source = "fn f() {}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        // (operation, an operation-specific class it must carry (empty if none), one it must not)
        for (operation, expected_class, unexpected_class) in [
            (HumanOperation::Update, "op-update", "op-moved"),
            (HumanOperation::MatchButNotIdentical, "", "op-update"),
        ] {
            let mapping = HumanMapping {
                entries: vec![HumanMappingEntry {
                    operation,
                    before_path: Some(vec![]),
                    after_path: Some(vec![]),
                }],
                ..Default::default()
            };
            let caches = rebuild_caches(&mapping.entries, before_root, after_root);

            let before_html = render_node(
                before_root,
                source.as_bytes(),
                'b',
                &caches,
                &HashMap::new(),
                true,
            );

            assert!(
                before_html.contains("status-matched changed"),
                "{operation:?} should render matched *and* changed: {before_html}"
            );
            if !expected_class.is_empty() {
                assert!(
                    before_html.contains(expected_class),
                    "{operation:?} should get the {expected_class} class: {before_html}"
                );
            }
            assert!(
                !before_html.contains(unexpected_class),
                "{operation:?} should not get the {unexpected_class} class: {before_html}"
            );
        }
    }

    #[test]
    fn render_node_marks_an_identical_but_relocated_match_as_moved() {
        let source_a = "fn f() {\n    a();\n    b();\n}\n";
        let source_b = "fn f() {\n    b();\n    a();\n}\n";
        let before_tree = parse_rust(source_a);
        let after_tree = parse_rust(source_b);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        // `a();` is expression_statement:1 before, expression_statement:2 after (swapped with
        // `b();`) - same content, different position.
        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(vec![
                    "function_item:1".to_string(),
                    "block:1".to_string(),
                    "expression_statement:1".to_string(),
                ]),
                after_path: Some(vec![
                    "function_item:1".to_string(),
                    "block:1".to_string(),
                    "expression_statement:2".to_string(),
                ]),
            }],
            ..Default::default()
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);

        let mut cursor = before_root.walk();
        let function_item = before_root.children(&mut cursor).next().unwrap();
        let mut c2 = function_item.walk();
        let block = function_item
            .children(&mut c2)
            .find(|n| n.kind() == "block")
            .unwrap();
        let mut c3 = block.walk();
        let call_statement = block
            .children(&mut c3)
            .find(|n| n.kind() == "expression_statement")
            .unwrap();

        let before_html = render_node(
            call_statement,
            source_a.as_bytes(),
            'b',
            &caches,
            &HashMap::new(),
            true,
        );

        assert!(
            before_html.contains("op-moved"),
            "an Identical match at a different path should get the op-moved class: {before_html}"
        );
        assert!(
            !before_html.contains("changed"),
            "a moved-but-identical match is still content-identical, not changed: {before_html}"
        );
    }

    #[test]
    fn render_node_marks_a_deleted_node_without_a_data_match() {
        let source = "fn f() {}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::DeleteWithChildren,
                before_path: Some(vec![]),
                after_path: None,
            }],
            ..Default::default()
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);

        let before_html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            &HashMap::new(),
            true,
        );

        assert!(
            before_html.contains("status-deleted"),
            "expected the deleted status class: {before_html}"
        );
        assert!(
            !before_html.contains("data-match"),
            "a deleted node has no counterpart to point at: {before_html}"
        );
    }

    #[test]
    fn fully_quiet_subtree_sizes_treats_matched_as_quiet_but_deleted_as_not() {
        let source = "fn main() {\n    a();\n    b();\n    c();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_item = root.children(&mut cursor).next().unwrap();
        let block = {
            let mut c = function_item.walk();
            function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut stmt_cursor = block.walk();
        let statements: Vec<Node> = block
            .children(&mut stmt_cursor)
            .filter(|n| n.kind() == "expression_statement")
            .collect();
        let stmt_a = statements[0];
        let stmt_c = statements[2];

        let mut caches = Caches::default();
        // `a();`'s call_expression is explicitly Matched - still quiet, nothing being edited.
        let mut c = stmt_a.walk();
        let call_expr_a = stmt_a.children(&mut c).next().unwrap();
        caches.before_match.insert(call_expr_a.id(), usize::MAX);
        // `c();`'s call_expression is explicitly Deleted - genuinely not quiet.
        let mut c2 = stmt_c.walk();
        let call_expr_c = stmt_c.children(&mut c2).next().unwrap();
        caches.before_removed.insert(call_expr_c.id(), false);

        let quiet = fully_quiet_subtree_sizes(root, &caches, status_before, is_identical_before);

        assert!(
            quiet.contains_key(&stmt_a.id()),
            "a(); has only a Matched descendant, which is quiet - not a live edit"
        );
        assert!(
            !quiet.contains_key(&stmt_c.id()),
            "c(); has a Deleted descendant, so it must not count as quiet"
        );
        assert!(
            !quiet.contains_key(&root.id()),
            "the root has a Deleted descendant somewhere, so it isn't quiet either"
        );
        assert!(
            quiet.contains_key(&statements[1].id()),
            "b(); has no marks anywhere in it (fully Unmarked), so it should still be quiet"
        );
    }

    #[test]
    fn fully_quiet_subtree_sizes_excludes_a_non_identical_match_even_though_its_matched() {
        // Same fixture as the test above, but `a();`'s call_expression is now a *non-identical*
        // match (an `Update`/`MatchButNotIdentical`, simulated directly on `Caches` the same way
        // the sibling test above simulates a plain match) - a real edit that happens to still be
        // paired must not be swallowed into a placeholder alongside genuinely untouched code.
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_item = root.children(&mut cursor).next().unwrap();
        let block = {
            let mut c = function_item.walk();
            function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut stmt_cursor = block.walk();
        let statements: Vec<Node> = block
            .children(&mut stmt_cursor)
            .filter(|n| n.kind() == "expression_statement")
            .collect();
        let stmt_a = statements[0];

        let mut caches = Caches::default();
        let mut c = stmt_a.walk();
        let call_expr_a = stmt_a.children(&mut c).next().unwrap();
        caches.before_match.insert(call_expr_a.id(), usize::MAX);
        caches
            .before_operation
            .insert(call_expr_a.id(), HumanOperation::Update);

        let quiet = fully_quiet_subtree_sizes(root, &caches, status_before, is_identical_before);

        assert!(
            !quiet.contains_key(&call_expr_a.id()),
            "a non-identical match is a real edit, not quiet"
        );
        assert!(
            !quiet.contains_key(&stmt_a.id()),
            "a(); contains a non-identical match, so it isn't quiet either"
        );
        assert!(
            !quiet.contains_key(&root.id()),
            "the root contains a non-identical match somewhere, so it isn't quiet either"
        );
        assert!(
            quiet.contains_key(&statements[1].id()),
            "b(); has no marks anywhere in it (fully Unmarked), so it should still be quiet"
        );
    }

    #[test]
    fn render_node_keeps_the_root_open_even_when_the_whole_tree_is_fully_quiet() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let caches = Caches::default(); // nothing marked anywhere
        let quiet_sizes =
            fully_quiet_subtree_sizes(root, &caches, status_before, is_identical_before);

        let html = render_node(
            root,
            source.as_bytes(),
            'b',
            &caches,
            &quiet_sizes,
            true, // force_open: the root itself must stay open/rendered despite being fully unmarked
        );

        assert!(
            html.starts_with(r#"<details class="node status-unmarked" id="b-"#) && {
                let root_tag_end = html.find('>').unwrap();
                html[..root_tag_end].contains(" open")
            },
            "the root must stay open despite being fully unmarked (force_open): {html}"
        );
    }

    #[test]
    fn render_node_omits_a_large_fully_unmarked_subtree_behind_a_placeholder() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_item = root.children(&mut cursor).next().unwrap();
        let caches = Caches::default(); // nothing marked anywhere
        let quiet_sizes =
            fully_quiet_subtree_sizes(root, &caches, status_before, is_identical_before);
        // The whole function body is one fully-unmarked subtree well past OMIT_THRESHOLD (20).
        let function_item_size = *quiet_sizes.get(&function_item.id()).unwrap();
        assert!(
            function_item_size > OMIT_THRESHOLD,
            "fixture assumption broken: function_item is only {function_item_size} nodes"
        );

        let html = render_node(root, source.as_bytes(), 'b', &caches, &quiet_sizes, true);

        assert!(
            html.contains(&format!("+{function_item_size} nodes collapsed")),
            "expected an omission placeholder naming the subtree size: {html}"
        );
        assert!(
            !html.contains("fn \"fn\""),
            "an omitted subtree's leaf content must not be in the DOM at all: {html}"
        );
        assert_eq!(
            html.matches("<details").count(),
            1,
            "only the root's own <details> should remain -- function_item was omitted, not \
             nested: {html}"
        );
    }

    #[test]
    fn render_node_omits_a_large_fully_matched_subtree_but_keeps_its_own_data_match() {
        // An exhaustively-annotated fixture (e.g. auto-generated code matched node-for-node) has
        // no Unmarked nodes at all, but should compress exactly the same way: a subtree with
        // nothing but Matched status throughout is just as "quiet" as an unannotated one.
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let mut cursor = before_root.walk();
        let function_item = before_root.children(&mut cursor).next().unwrap();

        // Match every single node, before to after, one-for-one (mirrors what human_solver's `f`
        // -- match to end of file -- produces on an unchanged file, and what a fixture like
        // c-cpython-autogenerated-code's real human_mapping.json actually looks like).
        fn match_everything(b: Node, a: Node, caches: &mut Caches) {
            caches.before_match.insert(b.id(), a.id());
            caches.after_match.insert(a.id(), b.id());
            let mut bc = b.walk();
            let mut ac = a.walk();
            for (bchild, achild) in b.children(&mut bc).zip(a.children(&mut ac)) {
                match_everything(bchild, achild, caches);
            }
        }
        let mut caches = Caches::default();
        match_everything(before_root, after_root, &mut caches);

        let quiet_sizes =
            fully_quiet_subtree_sizes(before_root, &caches, status_before, is_identical_before);
        let function_item_size = *quiet_sizes.get(&function_item.id()).unwrap();
        assert!(
            function_item_size > OMIT_THRESHOLD,
            "fixture assumption broken: function_item is only {function_item_size} nodes"
        );

        let html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            &quiet_sizes,
            true,
        );

        let expected_placeholder_prefix = format!(
            r#"<div class="node leaf status-matched placeholder" id="b-{}""#,
            function_item.id()
        );
        assert!(
            html.contains(&expected_placeholder_prefix),
            "expected a matched-status placeholder for function_item: {html}"
        );

        let after_function_item = {
            let mut c = after_root.walk();
            after_root.children(&mut c).next().unwrap()
        };
        assert!(
            html.contains(&format!("data-match=\"a-{}\"", after_function_item.id())),
            "a compressed matched subtree must still link to its counterpart: {html}"
        );
        assert!(
            html.contains(&format!("+{function_item_size} nodes collapsed")),
            "expected an omission placeholder naming the subtree size: {html}"
        );
        assert!(
            !html.contains("fn \"fn\""),
            "an omitted matched subtree's leaf content must not be in the DOM either: {html}"
        );
    }

    #[test]
    fn render_node_closes_but_still_fully_renders_a_small_fully_quiet_subtree() {
        // `parameters` (`(` `)`) is fully unmarked and tiny (well under OMIT_THRESHOLD), while
        // `function_item` as a whole is not fully unmarked (its body is marked) - so `parameters`
        // should render in full, just closed by default, not omitted.
        let source = "fn main() {\n    a();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let mut c = before_root.walk();
        let function_item = before_root.children(&mut c).next().unwrap();
        let mut c2 = function_item.walk();
        let block = function_item
            .children(&mut c2)
            .find(|n| n.kind() == "block")
            .unwrap();

        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::MatchButNotIdentical,
                before_path: Some(codediff::test::helper::path_for_node(block)),
                after_path: Some(codediff::test::helper::path_for_node(block)),
            }],
            ..Default::default()
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);
        let quiet_sizes =
            fully_quiet_subtree_sizes(before_root, &caches, status_before, is_identical_before);

        let html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            &quiet_sizes,
            true,
        );

        assert!(
            html.contains(r#"data-kind="parameters""#),
            "parameters should still be present, not omitted: {html}"
        );
        let tag_start = html.find(r#"data-kind="parameters""#).unwrap();
        let details_start = html[..tag_start].rfind("<details").unwrap();
        let details_tag_end = html[details_start..].find('>').unwrap() + details_start;
        assert!(
            !html[details_start..details_tag_end].contains(" open"),
            "a small fully-unmarked subtree should start collapsed: {}",
            &html[details_start..details_tag_end]
        );
        assert!(
            html.contains(r#"data-kind="(""#) && html.contains(r#"data-kind=")""#),
            "parameters' own children must still be fully rendered, just closed: {html}"
        );
    }

    #[test]
    fn render_index_page_links_to_each_fixtures_page_and_shows_its_language() {
        let entries = vec![
            IndexEntry {
                name: "rust-add-if".to_string(),
                language: Language::Rust,
                codediff_mismatches: 0,
                unix_diff_mismatches: 3,
                total_lines: 40,
            },
            IndexEntry {
                name: "c-linux-small-bugfix".to_string(),
                language: Language::C,
                codediff_mismatches: 1,
                unix_diff_mismatches: 5,
                total_lines: 12,
            },
        ];

        let html = render_index_page(&entries);

        assert!(html.contains(r#"href="fixtures/rust-add-if.html""#));
        assert!(html.contains(">rust-add-if<"));
        assert!(html.contains(">Rust<"));
        assert!(html.contains(r#"href="fixtures/c-linux-small-bugfix.html""#));
        assert!(html.contains(">C<"));
    }

    #[test]
    fn render_index_page_puts_each_mismatch_count_in_its_own_sortable_column() {
        let entries = vec![IndexEntry {
            name: "rust-add-if".to_string(),
            language: Language::Rust,
            codediff_mismatches: 2,
            unix_diff_mismatches: 9,
            total_lines: 40,
        }];

        let html = render_index_page(&entries);

        assert!(
            html.contains(r#"data-codediff="2""#),
            "expected the row to carry codediff's mismatch count as a data attribute for the \
             sort script to read: {html}"
        );
        assert!(
            html.contains(r#"data-unix_diff="9""#),
            "expected the row to carry Unix diff's mismatch count as a data attribute: {html}"
        );
        assert!(
            html.contains(r#"data-total_lines="40""#),
            "expected the row to carry the total line count as a data attribute: {html}"
        );
        assert!(
            html.contains(">2</td>"),
            "codediff's count should render as a cell: {html}"
        );
        assert!(
            html.contains(">9</td>"),
            "Unix diff's count should render as a cell: {html}"
        );
        for sort_key in ["name", "language", "codediff", "unix_diff", "total_lines"] {
            assert!(
                html.contains(&format!(r#"data-sort="{sort_key}""#)),
                "expected a sortable column header for {sort_key}: {html}"
            );
        }
        assert!(html.contains(r#"src="assets/index.js""#));
    }

    #[test]
    fn render_index_page_escapes_fixture_names() {
        // Fixture names are always safe identifiers in practice, but the escaping path itself
        // should still be exercised directly rather than assumed correct by inspection.
        let entries = vec![IndexEntry {
            name: "a&b".to_string(),
            language: Language::Unknown,
            codediff_mismatches: 0,
            unix_diff_mismatches: 0,
            total_lines: 0,
        }];
        let html = render_index_page(&entries);
        assert!(html.contains("a&amp;b"));
        assert!(!html.contains("a&b<"));
    }
}
