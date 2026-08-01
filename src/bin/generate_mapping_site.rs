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
    self, Caches, MarkKind, NodeStatus, rebuild_caches, status_after, status_before,
};

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

    let pairs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<&String> = pairs.keys().collect();
    names.sort();

    let mut index_entries: Vec<(String, Language)> = Vec::new();
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
        index_entries.push((name.clone(), language));
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

    let caches = rebuild_caches(&mapping.entries, before_root, after_root);
    // Most fixtures' human_mapping.json only annotates a few hundred nodes out of many thousands
    // (see human_mapping_cost's own doc comment) - the rest is untouched code the human considered
    // unchanged. Rendering every node in full made the biggest fixtures' pages multi-megabyte
    // (measured up to 16MB) for no reason: `render_node` uses these sizes to omit large
    // fully-unmarked subtrees behind a placeholder (small ones still render in full, just closed by
    // default), keeping the actually-interesting (annotated) parts front and center. Every node's
    // path (used only by the "file an issue" button) is deliberately *not* precomputed and baked in
    // here - `viewer.js` derives it lazily, client-side, only for whichever single node gets
    // clicked, since baking a `data-path` string into every node measurably added to that same
    // page-size problem (over a third of a node's own markup on a representative fixture).
    let before_unmarked_sizes = fully_unmarked_subtree_sizes(before_root, &caches, status_before);
    let after_unmarked_sizes = fully_unmarked_subtree_sizes(after_root, &caches, status_after);

    let before_html = render_node(
        before_root,
        before.contents.as_bytes(),
        'b',
        &caches,
        status_before,
        &before_unmarked_sizes,
        true,
    );
    let after_html = render_node(
        after_root,
        after.contents.as_bytes(),
        'a',
        &caches,
        status_after,
        &after_unmarked_sizes,
        true,
    );

    let language = before.metadata.language.unwrap_or_default();

    Ok(format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{name_escaped} — human mapping</title>
<link rel="stylesheet" href="../assets/style.css">
</head>
<body data-fixture="{name_attr}" data-repo="ivankovic/codediff">
<header class="page-header">
<a class="back-link" href="../index.html">&larr; all fixtures</a>
<h1>{name_escaped}</h1>
<span class="language-badge">{language}</span>
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
<dt>?</dt><dd>toggle this help</dd>
</dl>
</div>"#;

const SEARCH_PROMPT_HTML: &str = r#"<div id="search-prompt" class="hidden" role="dialog" aria-label="Search">
<label for="search-input">Search (plain substring, no regex):</label>
<input id="search-input" type="text" autocomplete="off">
</div>"#;

/// A fully-unmarked subtree (see `fully_unmarked_subtree_sizes`) bigger than this many nodes is
/// omitted from the HTML entirely (replaced with a one-line placeholder) rather than just
/// collapsed - collapsing via `<details>` without `open` still serializes the full subtree into
/// the page (a closed `<details>` is `display: none`, not "absent"), so on the corpus's biggest
/// fixtures - which are almost entirely untouched code around a small annotated diff - collapsing
/// alone still produced multi-megabyte pages (measured up to 16MB). Small fully-unmarked subtrees
/// (at or under this size) still render in full, just closed by default, so a reader can still
/// drill into ordinary short unchanged statements without hitting the placeholder wall constantly.
const OMIT_THRESHOLD: usize = 20;

/// Recursively renders `node` and its subtree. `side` is `'b'` (before) or `'a'` (after) - used
/// both as the id-namespace prefix (so before/after tree-sitter node ids, which can collide in
/// value between the two independently-parsed trees, never collide in the DOM) and to pick which
/// half of `caches` to read a match from. `unmarked_sizes` (see `fully_unmarked_subtree_sizes`)
/// maps a fully-unmarked node's id to its subtree's node count; `force_open` overrides both the
/// closed-by-default and the omit-with-placeholder treatment for `node` itself (but not its
/// descendants) - used to keep the tree root fully rendered and open even on the rare fixture
/// where it happens to be entirely unmarked (otherwise the page would load empty or collapsed).
fn render_node(
    node: Node,
    src: &[u8],
    side: char,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    unmarked_sizes: &HashMap<usize, usize>,
    force_open: bool,
) -> String {
    let status = status_fn(node, caches);
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

    let other_side = if side == 'b' { 'a' } else { 'b' };
    let id_attr = format!("{side}-{}", node.id());
    let match_attr = matched_other_id
        .map(|&other_id| format!(" data-match=\"{other_side}-{other_id}\""))
        .unwrap_or_default();
    let kind_attr = escape_html_attr(node.kind());

    let unmarked_size = unmarked_sizes.get(&node.id()).copied();

    if !force_open && unmarked_size.is_some_and(|size| size > OMIT_THRESHOLD) {
        let size = unmarked_size.unwrap();
        let kind_label = escape_html_text(node.kind());
        return format!(
            r#"<div class="node leaf status-unmarked placeholder" id="{id_attr}" data-kind="{kind_attr}" tabindex="0">{kind_label} (+{size} unchanged nodes collapsed)</div>"#
        );
    }

    if node.child_count() == 0 {
        let label = escape_html_text(&leaf_label(node, src));
        format!(
            r#"<div class="node leaf status-{status_class}" id="{id_attr}"{match_attr} data-kind="{kind_attr}" tabindex="0">{label}</div>"#
        )
    } else {
        let mut children = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            children.push_str(&render_node(
                child,
                src,
                side,
                caches,
                status_fn,
                unmarked_sizes,
                false,
            ));
        }
        let kind_label = escape_html_text(node.kind());
        let open_attr = if force_open || unmarked_size.is_none() {
            " open"
        } else {
            ""
        };
        format!(
            r#"<details class="node status-{status_class}" id="{id_attr}"{match_attr} data-kind="{kind_attr}"{open_attr}><summary tabindex="0">{kind_label}</summary>{children}</details>"#
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

/// Maps a node's id to its subtree's node count (itself plus every descendant), for every node
/// whose entire subtree is `NodeStatus::Unmarked` - nothing in it was annotated by a human. The
/// inverse-polarity counterpart of `human_solver`'s own `fully_solved_nodes` (which finds subtrees
/// that are entirely *marked*, to hide during active editing) - kept as a separate, generator-local
/// function rather than unified with it, since the two serve different purposes for different
/// audiences and only coincidentally share a shape. The size is `render_node`'s to decide whether a
/// fully-unmarked subtree is small enough to still render in full (just closed by default) or big
/// enough to omit outright behind a placeholder (see `OMIT_THRESHOLD`).
fn fully_unmarked_subtree_sizes(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> HashMap<usize, usize> {
    let mut sizes = HashMap::new();
    mark_fully_unmarked(root, caches, status_fn, &mut sizes);
    sizes
}

/// Post-order: returns `Some(subtree size)` if `node`'s own subtree is fully unmarked (recording
/// it in `sizes` too), `None` otherwise.
fn mark_fully_unmarked(
    node: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    sizes: &mut HashMap<usize, usize>,
) -> Option<usize> {
    let mut cursor = node.walk();
    let mut all_children_unmarked = true;
    let mut subtree_size = 1usize;
    for child in node.children(&mut cursor) {
        match mark_fully_unmarked(child, caches, status_fn, sizes) {
            Some(child_size) => subtree_size += child_size,
            None => all_children_unmarked = false,
        }
    }

    let is_unmarked = all_children_unmarked && status_fn(node, caches) == NodeStatus::Unmarked;
    if is_unmarked {
        sizes.insert(node.id(), subtree_size);
        Some(subtree_size)
    } else {
        None
    }
}

fn render_index_page(entries: &[(String, Language)]) -> String {
    let mut rows = String::new();
    for (name, language) in entries {
        rows.push_str(&format!(
            "<li><a href=\"fixtures/{name_attr}.html\">{name_escaped}</a> <span class=\"language-badge\">{language}</span></li>\n",
            name_attr = escape_html_attr(name),
            name_escaped = escape_html_text(name),
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
</header>
<ul class="fixture-list">
{rows}</ul>
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

        let html = render_node(
            root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
            &HashMap::new(),
            true,
        );

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
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);

        let before_html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
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
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);

        let before_html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
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
    fn fully_unmarked_nodes_only_includes_subtrees_with_no_marks_anywhere_in_them() {
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
        // Mark just `a();`'s call_expression as matched - everything else (including `b();`)
        // stays Unmarked.
        let mut c = stmt_a.walk();
        let call_expr = stmt_a.children(&mut c).next().unwrap();
        caches.before_match.insert(call_expr.id(), usize::MAX);

        let unmarked = fully_unmarked_subtree_sizes(root, &caches, status_before);

        assert!(
            !unmarked.contains_key(&stmt_a.id()),
            "a(); contains a marked descendant, so it isn't fully unmarked"
        );
        assert!(
            !unmarked.contains_key(&root.id()),
            "the root has a marked descendant somewhere, so it isn't fully unmarked either"
        );
        assert!(
            unmarked.contains_key(&statements[1].id()),
            "b(); has no marks anywhere in it, so it should be fully unmarked"
        );
    }

    #[test]
    fn render_node_keeps_the_root_open_even_when_the_whole_tree_is_fully_unmarked() {
        let source = "fn main() {\n    a();\n    b();\n}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let caches = Caches::default(); // nothing marked anywhere
        let unmarked_sizes = fully_unmarked_subtree_sizes(root, &caches, status_before);

        let html = render_node(
            root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
            &unmarked_sizes,
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
        let unmarked_sizes = fully_unmarked_subtree_sizes(root, &caches, status_before);
        // The whole function body is one fully-unmarked subtree well past OMIT_THRESHOLD (20).
        let function_item_size = *unmarked_sizes.get(&function_item.id()).unwrap();
        assert!(
            function_item_size > OMIT_THRESHOLD,
            "fixture assumption broken: function_item is only {function_item_size} nodes"
        );

        let html = render_node(
            root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
            &unmarked_sizes,
            true,
        );

        assert!(
            html.contains(&format!("+{function_item_size} unchanged nodes collapsed")),
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
    fn render_node_closes_but_still_fully_renders_a_small_fully_unmarked_subtree() {
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
        };
        let caches = rebuild_caches(&mapping.entries, before_root, after_root);
        let unmarked_sizes = fully_unmarked_subtree_sizes(before_root, &caches, status_before);

        let html = render_node(
            before_root,
            source.as_bytes(),
            'b',
            &caches,
            status_before,
            &unmarked_sizes,
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
            ("rust-add-if".to_string(), Language::Rust),
            ("c-linux-small-bugfix".to_string(), Language::C),
        ];

        let html = render_index_page(&entries);

        assert!(html.contains(r#"href="fixtures/rust-add-if.html""#));
        assert!(html.contains(">rust-add-if<"));
        assert!(html.contains(">Rust<"));
        assert!(html.contains(r#"href="fixtures/c-linux-small-bugfix.html""#));
        assert!(html.contains(">C<"));
    }

    #[test]
    fn render_index_page_escapes_fixture_names() {
        // Fixture names are always safe identifiers in practice, but the escaping path itself
        // should still be exercised directly rather than assumed correct by inspection.
        let entries = vec![("a&b".to_string(), Language::Unknown)];
        let html = render_index_page(&entries);
        assert!(html.contains("a&amp;b"));
        assert!(!html.contains("a&b<"));
    }
}
