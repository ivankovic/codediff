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
use codediff::diff::NodeCache;
use codediff::diff::text::{RangeMatch, TextDiff, TextOperation};
use codediff::test::helper;
use codediff::test::helper::human_mapping::{
    self, Caches, HumanOperation, HumanTextVerdict, MarkKind, NodeStatus, is_identical_after,
    is_identical_before, is_moved_after, is_moved_before, match_operation_after,
    match_operation_before, rebuild_caches_for_mapping, status_after, status_before,
    unmarked_node_count,
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

    // Where each fixture came from, joined once for the whole corpus rather than per page. A
    // fixture directory holds only the two files, so the repository/commit it was sampled from
    // lives in `sample.csv`, and turning that row's `owner-repo` slug into a URL needs the clone
    // list - see `helper::repository_urls` for why the slug can't just be split on a dash.
    let provenance = helper::sample_provenance()?;
    let repository_urls = helper::repository_urls()?;

    let mut index_entries: Vec<IndexEntry> = Vec::new();
    let mut skipped = 0usize;
    let mut warnings: Vec<String> = Vec::new();

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

        // The fixture's own `description.md`, the same note `diffs.csv`'s `comment` column carries
        // - a person's statement of what this case is and why it is worth keeping, which until now
        // the site was the only place not to show.
        let note = helper::read_note(name);
        let upstream = provenance.get(name).and_then(|sample| {
            helper::upstream_commit_url(sample, &repository_urls).map(|commit_url| Upstream {
                commit_url,
                repository: sample.repository.clone(),
                commit: sample.commit.clone(),
                path: sample.path.clone(),
            })
        });

        let page = render_fixture_page(
            name,
            before,
            after,
            &mapping,
            note.as_deref(),
            upstream.as_ref(),
            &mut warnings,
        )?;
        fs::write(fixtures_dir.join(format!("{name}.html")), page.html)
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
            // Names, not a count, and the empty list is load-bearing: "nobody painted this" and
            // "somebody painted it and there was nothing to paint" are different states, and only
            // the first has no names (see `HumanMapping::text_mappings`).
            paintings: mapping
                .text_mappings
                .iter()
                .map(|named| named.name.clone())
                .collect(),
            note,
            unmarked_nodes: page.unmarked_nodes,
        });
    }

    fs::write(
        args.out.join("index.html"),
        render_index_page(&index_entries),
    )?;

    let painted = index_entries
        .iter()
        .filter(|entry| !entry.paintings.is_empty())
        .count();
    let incomplete = index_entries
        .iter()
        .filter(|entry| entry.unmarked_nodes > 0)
        .count();
    println!(
        "Generated {} fixture page(s) into {:?} ({painted} with a painting, {incomplete} with \
         unmarked nodes, {skipped} skipped, no human_mapping.json)",
        index_entries.len(),
        args.out,
    );
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

/// Where a fixture's two files were sampled from, resolved to something linkable.
///
/// The `url` is the commit the *after* side comes from - the before side is the same file in its
/// single parent - so it opens on exactly the change this fixture captures. See
/// `helper::SampleProvenance`.
struct Upstream {
    /// The full commit URL, already built by `helper::upstream_commit_url` - not the repository
    /// URL, so nothing here has to know how a given forge spells a commit path.
    commit_url: String,
    /// `owner-repo`, the clone-directory slug `sample.csv` records. Shown as the link's text, since
    /// a bare commit hash says nothing about which project it belongs to.
    repository: String,
    commit: String,
    path: String,
}

/// One rendered fixture page, plus the one number the index wants that only rendering computes.
struct FixturePage {
    html: String,
    /// Nodes the human mapping says nothing about. Counted here rather than in `main` because
    /// `render_fixture_page` already builds the `Caches` it needs; recomputing them per fixture
    /// for the index would double the corpus's cache-rebuild cost for one integer.
    unmarked_nodes: usize,
}

fn render_fixture_page(
    name: &str,
    before: &Code,
    after: &Code,
    mapping: &human_mapping::HumanMapping,
    note: Option<&str>,
    upstream: Option<&Upstream>,
    // Data problems worth a maintainer's attention that are not worth failing over - currently
    // only an unreadable painting, which costs its own panel and nothing else. `main` prints them
    // once the whole corpus has been rendered.
    warnings: &mut Vec<String>,
) -> Result<FixturePage> {
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
    // Counted from the caches already built above, before they are consumed by rendering.
    let unmarked_nodes = unmarked_node_count(before_root, &caches, status_before)
        + unmarked_node_count(after_root, &caches, status_after);

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

    // The second view of the same mapping: the tree above says which *nodes* the human paired,
    // this says what that looks like as code. They answer different questions - a tree node like
    // an `expression_statement` wrapper has no visible text of its own, so a reader scanning the
    // tree cannot tell which annotations correspond to something they would actually see on
    // screen (the same visible-vs-scaffolding split `visible_node_ids` draws for mismatch
    // counting). The code panel is the visible half, rendered directly.
    //
    // Routed through exactly the machinery codediff's own output uses - `as_ast_diff_for_mapping`
    // turns the human mapping into a real `ASTDiff`, which `TextDiff::from` projects to per-side
    // ranges - so the highlighting here is the human's answer rendered the way the TUI renders
    // codediff's, not a second, separately-written interpretation of `human_mapping.json` that
    // could drift from the first.
    let human_diff = human_mapping::as_ast_diff_for_mapping(mapping, before, after)
        .with_context(|| format!("building a synthetic ASTDiff for '{name}'"))?;
    let node_cache = NodeCache::build(before, after);
    let text_diff = TextDiff::from(before, after, &human_diff, &node_cache);
    let before_ranges = text_diff.all(0);
    let after_ranges = text_diff.all(1);
    let row_counts = [
        before.contents.split('\n').count(),
        after.contents.split('\n').count(),
    ];

    // The tree mapping's rendering first, then one per human painting. They are alternative
    // accounts of the same edit, not a decomposition of it, so they are stacked as separate panels
    // the reader switches between rather than merged into one - see `HumanMapping::text_mappings`
    // for why a fixture carries several answers at all.
    let mut renderings: Vec<(String, String, [PanelRanges; 2])> = vec![(
        "tree".to_string(),
        "From the node mapping".to_string(),
        [
            PanelRanges::from_tree(
                "b",
                "a",
                before_ranges.clone(),
                &after_ranges,
                row_counts[0],
            ),
            PanelRanges::from_tree(
                "a",
                "b",
                after_ranges.clone(),
                &before_ranges,
                row_counts[1],
            ),
        ],
    )];
    for (index, named) in mapping.text_mappings.iter().enumerate() {
        match painting_panels(named, &before.contents, &after.contents, index, row_counts) {
            Ok(panels) => renderings.push((format!("p{index}"), named.name.clone(), panels)),
            // One unreadable painting costs its own panel, not the page and not the site build.
            // Reported by `main`, which is where a maintainer will see it.
            Err(error) => warnings.push(format!("'{name}' painting: {error:#}")),
        }
    }

    // Folding is decided once per side, across every rendering at once, and both halves of that
    // matter - see `code_visible_rows`.
    let mut anchors: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for (_, _, panels) in &renderings {
        for side in 0..2 {
            anchors[side].extend(panels[side].anchor_rows());
        }
    }
    let visible = [
        code_visible_rows(&anchors[0], row_counts[0]),
        code_visible_rows(&anchors[1], row_counts[1]),
    ];

    let mut code_sections = String::new();
    let mut rendering_buttons = String::new();
    for (key, label, panels) in &renderings {
        let selected = key == "tree";
        // Both the DOM handle and the human name: `viewer.js` switches by handle but *remembers*
        // by name, since `p0` means a different painting on the next fixture while "Minimal" means
        // the same thing everywhere.
        let label_attr = escape_html_attr(label);
        rendering_buttons.push_str(&format!(
            r#"<button type="button" data-painting="{key}" data-painting-name="{label_attr}" aria-pressed="{selected}">{}</button>"#,
            escape_html_text(label)
        ));
        code_sections.push_str(&format!(
            r#"<div class="panels code-panels{hidden}" data-painting="{key}" data-painting-name="{label_attr}">
<section class="panel code-panel" data-side="before">
<h2>Before</h2>
<div class="code">{}</div>
</section>
<section class="panel code-panel" data-side="after">
<h2>After</h2>
<div class="code">{}</div>
</section>
</div>
"#,
            render_code_panel(&before.contents, &panels[0], &visible[0]),
            render_code_panel(&after.contents, &panels[1], &visible[1]),
            hidden = if selected { "" } else { " hidden" },
        ));
    }

    // No switch and no explanation on a fixture nobody has painted: one rendering needs no chooser,
    // and a note about paintings on a page with none is just noise.
    let painting_switch = if renderings.len() > 1 {
        format!(
            r#"<div class="painting-switch" role="group" aria-label="Code rendering">
<span class="painting-switch-label">Code view:</span>
{rendering_buttons}</div>
<p class="notice painting-notice">A painting is a person's account of this edit <em>as text</em>, recorded independently of the node mapping - a rendering often has several equally defensible answers where the mapping has one. Deletions and insertions carry no position on the opposite side in a painting, so that panel draws no caret for them.</p>"#
        )
    } else {
        String::new()
    };

    // The human's own words about this fixture, when there are any. First thing on the page after
    // the header: it says what the case is *for*, which no amount of reading the two trees does.
    let description = match note {
        Some(note) => format!(
            r#"<p class="description">{}</p>"#,
            escape_html_text(note.trim())
        ),
        None => String::new(),
    };

    // An incomplete mapping and a complete one look identical on this page - every unmarked node
    // renders as unmarked, which is also how a node the human deliberately left alone renders. Say
    // which it is, since reviewing ground truth is the whole point of the site.
    let unmarked_notice = if unmarked_nodes == 0 {
        String::new()
    } else {
        let total = human_mapping::total_node_count_for(before, after);
        format!(
            r#"<p class="notice">This mapping is unfinished: {unmarked_nodes} of {total} nodes are still unmarked.</p>"#
        )
    };

    // `representative_entries` (via `as_ast_diff_for_mapping`) has to collapse each multi-map
    // group down to one concrete pairing to produce an `ASTDiff` at all - but a group exists
    // precisely because several pairings are equally correct. Say so, rather than letting a page
    // that shows one of them imply it is the answer.
    let groups_notice = if mapping.groups.is_empty() {
        String::new()
    } else {
        let count = mapping.groups.len();
        let plural = if count == 1 { "" } else { "s" };
        format!(
            r#"<p class="notice">This mapping has {count} multi-map group{plural}: several pairings are equally correct there. The code view shows one arbitrary valid pairing, not the only one.</p>"#
        )
    };

    let language = before.metadata.language.unwrap_or_default();
    // `diffs_case_dir` resolves which `DIFF_DATASETS` folder this fixture actually lives under
    // (`helper::DIFF_DATASETS`) - the URL needs that segment, even though every other parameter
    // here is already in memory and doesn't otherwise touch disk.
    let dataset = helper::diffs_case_dir(name)
        .and_then(|dir| {
            dir.parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "small".to_string());
    // The other link in the header points at *our* copy of the two files; this one points at the
    // change they were cut from. A fixture with no sample row (every handmade one) or an
    // unresolvable repository simply has no such link - see `helper::repository_urls`.
    let upstream_link = match upstream {
        Some(upstream) => {
            // Seven characters is what every forge abbreviates a hash to, and the full 40 crowds
            // out the repository name beside it.
            let short: String = upstream.commit.chars().take(7).collect();
            format!(
                r#"<a class="source-link" href="{url}" target="_blank" rel="noopener">Upstream commit {repository}@{short}</a>
<span class="source-path" title="path in the upstream repository">{path}</span>"#,
                url = escape_html_attr(&upstream.commit_url),
                repository = escape_html_text(&upstream.repository),
                short = escape_html_text(&short),
                path = escape_html_text(&upstream.path),
            )
        }
        None => String::new(),
    };

    let source_url = format!(
        "https://github.com/{REPO}/tree/main/src/test/data/diffs/{}/{}",
        dataset,
        escape_html_attr(name)
    );

    let html = format!(
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
{upstream_link}
<ul class="legend">
<li><span class="status-unmarked">&#9679;</span> unmarked</li>
<li><span class="status-matched">&#9679;</span> matched</li>
<li><span class="status-matched op-update changed">&#9679;</span> updated</li>
<li><span class="status-matched changed">&#9679;</span> matched, not identical</li>
<li><span class="status-matched op-moved">&#9679;</span> moved, unchanged</li>
<li><span class="status-deleted">&#9679;</span> deleted</li>
<li><span class="status-inserted">&#9679;</span> inserted</li>
</ul>
<ul class="legend code-legend">
<li><span class="cd cd-inserted-swatch cd-insert"></span> inserted</li>
<li><span class="cd cd-inserted-swatch cd-delete"></span> deleted</li>
<li><span class="cd cd-inserted-swatch cd-update"></span> updated</li>
<li><span class="cd cd-inserted-swatch cd-move"></span> moved</li>
<li>click a highlight to reveal its counterpart</li>
</ul>
<div class="view-switch" role="group" aria-label="View">
<button type="button" data-view="split" aria-pressed="true">Split</button>
<button type="button" data-view="code" aria-pressed="false">Code</button>
<button type="button" data-view="tree" aria-pressed="false">Tree</button>
</div>
</header>
{description}
{unmarked_notice}
{groups_notice}
{painting_switch}
{code_sections}<div class="panels tree-panels">
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
        painting_switch = painting_switch,
        code_sections = code_sections,
        description = description,
        unmarked_notice = unmarked_notice,
        upstream_link = upstream_link,
        groups_notice = groups_notice,
        name_escaped = escape_html_text(name),
        name_attr = escape_html_attr(name),
        repo = REPO,
        help_overlay = HELP_OVERLAY_HTML,
        search_prompt = SEARCH_PROMPT_HTML,
    );
    Ok(FixturePage {
        html,
        unmarked_nodes,
    })
}

const HELP_OVERLAY_HTML: &str = r#"<div id="help-overlay" class="hidden" role="dialog" aria-label="Keybindings">
<h2>Keybindings</h2>
<dl>
<dt>j / k</dt><dd>next / previous visible node (tree panels)</dd>
<dt>h / l</dt><dd>collapse / expand the focused node</dd>
<dt>g / G</dt><dd>jump to first / last visible node</dd>
<dt>Tab</dt><dd>switch focus between Before/After panels</dd>
<dt>/</dt><dd>search: jump to next node whose text contains a given string</dd>
<dt>a</dt><dd>jump the other panel to this node's mapped counterpart</dd>
<dt>i</dt><dd>hide identical matches, showing only inserted/deleted/updated nodes and their ancestors</dd>
<dt>v</dt><dd>cycle the view: split / code only / tree only</dd>
<dt>p</dt><dd>cycle what the code view renders: the node mapping, then each human painting</dd>
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

/// Rows of unchanged context kept around every changed row in a code panel. A code panel renders
/// the file's real source text, so unlike the tree panels (whose markup is several times the size
/// of the code it describes) it is cheap per row - but the corpus's `full` dataset holds real
/// multi-thousand-line source files whose diffs touch a handful of lines, and rendering all of
/// those rows twice per page for nothing is exactly the page-size problem `OMIT_THRESHOLD` already
/// solves for the trees. Same treatment, same reason, just a much more generous budget.
const CODE_CONTEXT_ROWS: usize = 6;

/// A run of consecutive unchanged, out-of-context rows shorter than this is rendered in full
/// rather than folded: below it the fold placeholder costs about as much markup as the rows it
/// replaces, and reading around a two-line gap is worse than reading through it.
const CODE_FOLD_THRESHOLD: usize = 4;

/// The CSS class painting `operation`, or `None` for the two sentinels that mean "not a change" -
/// `Identical` text is the panel's plain, unhighlighted background, so it gets no span at all.
fn code_operation_class(operation: &TextOperation) -> Option<&'static str> {
    match operation {
        TextOperation::Insert => Some("cd-insert"),
        TextOperation::Delete => Some("cd-delete"),
        TextOperation::Update => Some("cd-update"),
        TextOperation::Move => Some("cd-move"),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
}

/// Snaps `column` down to the nearest UTF-8 character boundary of `line`, clamped to its length.
///
/// `TextRange` columns are tree-sitter's own *byte* columns, and every range this module renders
/// comes from a node boundary, so a column should always already land on a character boundary.
/// This exists so that "should" can't turn a malformed range into a panicking site generator:
/// slicing a `&str` mid-character panics, and a whole site build failing over one odd range in one
/// fixture is a far worse outcome than that fixture's highlight being a byte or two off.
fn snap_to_char_boundary(line: &str, column: usize) -> usize {
    let mut column = column.min(line.len());
    while column > 0 && !line.is_char_boundary(column) {
        column -= 1;
    }
    column
}

/// A caret drawn on one side to mark where the *other* side's inserted or deleted text belongs -
/// the only thing a pure insertion's before panel, or a pure deletion's after panel, has to show
/// at all.
///
/// Derived from the other side's ranges, not this side's. `TextRange`'s doc comment describes a
/// symmetric scheme where each side gets its own zero-width placeholder for what the other side
/// added or removed, but that is not what reaches a consumer: on a pure deletion the before side
/// carries a `Delete` range whose *destination* is the zero-width after-side position, and the
/// after side's range list has no non-`Identical` entry at all (verified on
/// `c-htop-remove-function-declaration`, whose after panel this makes the difference between a
/// page of unmarked context and a readable one). So the mark has to be read off the counterpart's
/// `destination`.
struct CodeMarker {
    row: usize,
    column: usize,
    operation: TextOperation,
    /// This caret's own `data-range` id.
    id: String,
    /// The `data-range` id of the real text, on the other side, that it stands in for. Together
    /// with `id` this is what makes the two ends of the pair name each other.
    points_at: String,
}

/// The `data-range` id each range in a list carries, when the ids are simply positional.
///
/// This is what every range gets in the tree-derived panels, where each `RangeMatch` is its own
/// independent decision. A painting is the exception - see [`painting_panels`], where several
/// spans deliberately share one id.
fn positional_ids(side: &str, count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{side}{index}")).collect()
}

/// Every caret one side should draw, read off `other`'s ranges (see [`CodeMarker`]).
///
/// `row_count` is this side's own line count, and the clamp against it is load-bearing rather than
/// defensive: a delete at end-of-file puts its destination at the position *after* the last line,
/// which is not a row that gets rendered. Unclamped, `code_anchor_rows` would still anchor there
/// (it clamps for its own indexing) while the per-row lookup in `render_code_panel` would find
/// nothing - dropping the one mark that panel had to show, silently. One corpus fixture does
/// exactly this: `python-api-change` puts a caret at row 18 of an 18-row side.
fn code_markers(
    other: &[RangeMatch],
    row_count: usize,
    side: &str,
    other_ids: &[String],
) -> Vec<CodeMarker> {
    other
        .iter()
        .enumerate()
        .filter(|(_, range_match)| {
            code_operation_class(&range_match.operation).is_some()
                && !range_match.source.is_empty()
                && range_match.destination.is_empty()
        })
        .map(|(index, range_match)| CodeMarker {
            row: range_match
                .destination
                .start_row
                .min(row_count.saturating_sub(1)),
            column: range_match.destination.start_column,
            operation: range_match.operation.clone(),
            id: format!("{side}m{index}"),
            points_at: other_ids[index].clone(),
        })
        .collect()
}

/// Maps each index in `from`'s range list to the `data-range` id of the thing on the *other* side
/// that it points at, so a clicked span can reveal its counterpart.
///
/// `RangeMatch::destination` already carries the counterpart's extent directly, so the ordinary
/// case is just a lookup of that extent in the other side's own range list - the two lists are
/// built from one `TextDiff` over the same pair, so a destination that names real text is
/// normally present there verbatim as some range's `source`. "Normally" measured, not assumed:
/// across the corpus's 493 mapped fixtures, 2214 of 2232 linkable ranges (99.2%) find their
/// counterpart by exact key, so the 18 that don't are not worth a fuzzy nearest-overlap fallback -
/// they simply render without a link, which is what an unlinkable range should do anyway.
///
/// An insert or a delete has no real text on the other side to point at, only a position; that
/// position is drawn as a caret (see [`code_markers`]), and this links to the caret instead, so
/// the pairing reads the same in both directions.
fn code_counterparts(
    from: &[RangeMatch],
    to: &[RangeMatch],
    to_side: &str,
    to_ids: &[String],
) -> HashMap<usize, String> {
    let key = |r: &codediff::diff::text_range::TextRange| {
        (r.start_row, r.start_column, r.end_row, r.end_column)
    };
    let mut by_source: HashMap<(usize, usize, usize, usize), usize> = HashMap::new();
    for (index, range_match) in to.iter().enumerate() {
        if range_match.source.is_empty() {
            continue;
        }
        by_source.entry(key(&range_match.source)).or_insert(index);
    }

    from.iter()
        .enumerate()
        .filter(|(_, range_match)| code_operation_class(&range_match.operation).is_some())
        .filter_map(|(index, range_match)| {
            if range_match.destination.is_empty() {
                // The other side has only a caret here, and `code_markers` names it after the
                // range it stands in for - which is this one.
                return Some((index, format!("{to_side}m{index}")));
            }
            by_source
                .get(&key(&range_match.destination))
                .map(|&other| (index, to_ids[other].clone()))
        })
        .collect()
}

/// One side of one code rendering: the ranges to paint onto that side's source text, the DOM ids
/// they carry, what each points at on the other side, and the carets standing in for text this
/// side doesn't have.
///
/// One page now holds several of these per side - the tree mapping projected to text, plus one per
/// human painting (see [`painting_panels`]) - which is why `side` is a *string* prefix rather than
/// the `'b'`/`'a'` char it used to be. Every `data-range` and row `id` is built from it, and
/// `viewer.js`'s `spansForRange` looks ids up document-wide, so two renderings sharing a prefix
/// would have clicks in the visible panel selecting spans in a hidden one.
struct PanelRanges {
    /// DOM id prefix for this panel: `b`/`a` for the tree mapping, `b{k}`/`a{k}` for painting *k*.
    side: String,
    ranges: Vec<RangeMatch>,
    /// `data-range` id per range, parallel to `ranges`. Several ranges may deliberately share one
    /// id - that is how an N:M painted match says "these spans are one decision". `viewer.js`
    /// already selects by id with `querySelectorAll`, since a multi-row range is rendered as one
    /// span per row, so a shared id needs no client-side change.
    ids: Vec<String>,
    /// `data-counterpart` per range index, where there is something on the other side to point at.
    counterparts: HashMap<usize, String>,
    markers: Vec<CodeMarker>,
    /// This side's per-row operation, from `line_operations` - kept rather than recomputed because
    /// both the fold anchors and the row tints read it.
    ops: Vec<TextOperation>,
}

impl PanelRanges {
    /// One side of the tree mapping's own rendering, the panel this page has always drawn.
    fn from_tree(
        side: &str,
        other_side: &str,
        ranges: Vec<RangeMatch>,
        other: &[RangeMatch],
        row_count: usize,
    ) -> Self {
        let ids = positional_ids(side, ranges.len());
        let other_ids = positional_ids(other_side, other.len());
        PanelRanges {
            counterparts: code_counterparts(&ranges, other, other_side, &other_ids),
            markers: code_markers(other, row_count, side, &other_ids),
            ops: codediff::diff::text::line_operations(&ranges, row_count),
            side: side.to_string(),
            ids,
            ranges,
        }
    }

    /// Rows worth centering a fold on for *this* rendering: changed rows, plus every caret's row.
    ///
    /// The second kind is not optional. A caret has no columns on this side to color, so
    /// `line_operations` cannot see it, and a panel anchored on changed rows alone folds a pure
    /// deletion's after side away entirely and renders a page with nothing on it.
    fn anchor_rows(&self) -> Vec<usize> {
        let mut anchors: Vec<usize> = self
            .ops
            .iter()
            .enumerate()
            .filter(|(_, op)| **op != TextOperation::Identical && **op != TextOperation::NotYetSet)
            .map(|(row, _)| row)
            .collect();
        anchors.extend(self.markers.iter().map(|marker| marker.row));
        anchors
    }
}

/// Both sides of one named painting, as the panels [`render_code_panel`] already draws.
///
/// The conversion is deliberately thin: a painted span *is* a range to color, so it becomes a
/// `RangeMatch` and goes through exactly the renderer the tree-derived panels use. Two things a
/// painting does not carry, and this does not invent:
///
/// * **No destination per span.** `RangeMatch::destination` exists so `TextDiff`'s consumers can
///   find a range's counterpart by extent; a painting states its correspondences by grouping spans
///   into one entry instead, so the counterpart links below are built from that grouping directly
///   and `destination` is left zero. Nothing in the rendering path reads it.
/// * **No caret positions.** A `Delete` records where text *was*, never where its absence sits on
///   the after side, so the opposite panel draws no caret for it - unlike the tree panel, where
///   `TextDiff` computes that position. This is why the fold is unioned across renderings in
///   `render_fixture_page`: the painting panels then keep the tree panel's caret rows visible
///   without drawing tree data, and a one-sided fixture's opposite panel doesn't unfold whole.
///
/// An N:M match gives every span on a side the same id, so clicking any one of them reveals the
/// whole opposite side of that entry. That is what the entry asserts - which specific span pairs
/// with which is explicitly not recorded (see `HumanTextEntry`) - so naming a pairing here would
/// be inventing one.
///
/// `Err` if any entry is malformed or falls outside its file (`HumanTextEntry::verdict`'s own
/// contract). The caller skips that painting rather than failing the build, for the same reason
/// `snap_to_char_boundary` clamps: one bad painting should cost its own panel, not the whole site.
fn painting_panels(
    named: &human_mapping::NamedTextMapping,
    before: &str,
    after: &str,
    painting: usize,
    row_counts: [usize; 2],
) -> Result<[PanelRanges; 2]> {
    let sides = [format!("b{painting}"), format!("a{painting}")];
    let mut ranges: [Vec<RangeMatch>; 2] = [Vec::new(), Vec::new()];
    let mut ids: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let mut counterparts: [HashMap<usize, String>; 2] = [HashMap::new(), HashMap::new()];

    for (entry_index, entry) in named.mapping.entries.iter().enumerate() {
        let verdict = entry
            .verdict(before, after)
            .with_context(|| format!("entry {entry_index} of the '{}' painting", named.name))?;
        let operation = match verdict {
            HumanTextVerdict::Move => TextOperation::Move,
            HumanTextVerdict::Update => TextOperation::Update,
            HumanTextVerdict::Delete => TextOperation::Delete,
            HumanTextVerdict::Insert => TextOperation::Insert,
        };
        // A `Match` is the only entry with text on both sides, so it is the only one whose spans
        // have anything to point at.
        let linked = matches!(verdict, HumanTextVerdict::Move | HumanTextVerdict::Update);
        let entry_ids = [
            format!("{}e{entry_index}", sides[0]),
            format!("{}e{entry_index}", sides[1]),
        ];
        for (side, spans) in [(0usize, &entry.before), (1usize, &entry.after)] {
            for span in spans {
                let index = ranges[side].len();
                ranges[side].push(RangeMatch {
                    source: span.to_text_range(),
                    destination: codediff::diff::text_range::TextRange::zero(),
                    operation: operation.clone(),
                });
                ids[side].push(entry_ids[side].clone());
                if linked {
                    counterparts[side].insert(index, entry_ids[1 - side].clone());
                }
            }
        }
    }

    let [before_ranges, after_ranges] = ranges;
    let [before_ids, after_ids] = ids;
    let [before_counterparts, after_counterparts] = counterparts;
    Ok([
        PanelRanges {
            ops: codediff::diff::text::line_operations(&before_ranges, row_counts[0]),
            side: sides[0].clone(),
            ranges: before_ranges,
            ids: before_ids,
            counterparts: before_counterparts,
            markers: Vec::new(),
        },
        PanelRanges {
            ops: codediff::diff::text::line_operations(&after_ranges, row_counts[1]),
            side: sides[1].clone(),
            ranges: after_ranges,
            ids: after_ids,
            counterparts: after_counterparts,
            markers: Vec::new(),
        },
    ])
}

/// Which rows of a panel are actually rendered: every anchor, plus `CODE_CONTEXT_ROWS` on each
/// side of it. Everything else is folded away by [`render_code_panel`].
///
/// `anchors` is the union over *every* rendering of this side - the tree mapping's and each
/// painting's - not one panel's own. Two reasons, and the first is the point of the page:
///
/// * A reader flips between the tree mapping and a painting to see where they differ. Folding each
///   panel by its own anchors shifts every row between the two, so the comparison is against a
///   moving target.
/// * A painting draws no carets, so a pure insertion's *before* painting panel has no anchors at
///   all and would fall through to the "show everything" case below - unfolding a 120KB file.
///   Sharing the tree panel's anchors gives it the fold the caret was there to produce.
///
/// If nothing anchors anywhere (the two sides are wholly unchanged), everything stays visible:
/// there is no change to center a fold on, and a blank panel is strictly worse than a long one.
fn code_visible_rows(anchors: &[usize], row_count: usize) -> Vec<bool> {
    if anchors.is_empty() {
        return vec![true; row_count];
    }

    let mut visible = vec![false; row_count];
    for &row in anchors {
        let start = row.saturating_sub(CODE_CONTEXT_ROWS);
        let end = (row + CODE_CONTEXT_ROWS + 1).min(row_count);
        for slot in visible.iter_mut().take(end).skip(start) {
            *slot = true;
        }
    }
    visible
}

/// Renders one side's source text with a rendering's changes painted onto it, character-precise -
/// the code-shaped counterpart to `render_node`'s tree.
///
/// Built *source-text-first*: the file's own bytes are walked row by row and a `<span>` is opened
/// only where a non-`Identical` range covers them. It deliberately does not concatenate the
/// ranges' own text, which would look equivalent and silently corrupt the output - `diff::text`
/// ranges are whitespace-insensitive and leave gaps between themselves (leading indentation
/// especially; see `line_operations`' doc comment on why *it* is row-granular for the same
/// reason), so range-concatenation would drop exactly those gap bytes. The
/// `render_code_panel_reproduces_the_source_text_exactly` test below is what holds this property
/// down, for the painted panels as much as the tree-derived one: strip the tags back off and what
/// remains must be the file, byte for byte.
fn render_code_panel(contents: &str, panel: &PanelRanges, visible: &[bool]) -> String {
    let lines: Vec<&str> = contents.split('\n').collect();

    let mut markers_by_row: HashMap<usize, Vec<&CodeMarker>> = HashMap::new();
    for marker in &panel.markers {
        markers_by_row.entry(marker.row).or_default().push(marker);
    }
    let no_markers: Vec<&CodeMarker> = Vec::new();
    let row_html = |row: usize| {
        render_code_row(
            lines[row],
            row,
            &panel.ops[row],
            panel,
            markers_by_row.get(&row).unwrap_or(&no_markers),
        )
    };

    let mut html = String::new();
    let mut row = 0usize;
    while row < lines.len() {
        if visible[row] {
            html.push_str(&row_html(row));
            row += 1;
            continue;
        }
        let start = row;
        while row < lines.len() && !visible[row] {
            row += 1;
        }
        let folded = row - start;
        if folded >= CODE_FOLD_THRESHOLD {
            // The 1-indexed line range, not just a count: the count alone is impossible to check
            // against the gutter (and `split('\n')` contributes a trailing empty row that makes it
            // read one high), while the range says exactly which lines to go look at on GitHub.
            let (first, last) = (start + 1, row);
            html.push_str(&format!(
                r#"<div class="cl fold"><span class="ln">&hellip;</span><span class="lt">lines {first}&ndash;{last} unchanged ({folded} lines)</span></div>"#
            ));
        } else {
            // Too short to be worth a placeholder - render it after all.
            for short_row in start..row {
                html.push_str(&row_html(short_row));
            }
        }
    }
    html
}

/// One row of a code panel: a line-number gutter cell plus the row's text, split into plain
/// stretches, highlighted spans, and any carets that belong on it.
fn render_code_row(
    line: &str,
    row: usize,
    row_op: &TextOperation,
    panel: &PanelRanges,
    markers: &[&CodeMarker],
) -> String {
    let row_len = line.len();
    let side = &panel.side;

    // Every span this row draws, as byte-column bounds in left-to-right order: this rendering's
    // own ranges as real, text-covering spans, plus any carets as zero-width ones. `Identical`
    // ranges are the unpainted default, so they produce nothing.
    let mut segments: Vec<(usize, usize, &TextOperation, &String, Option<&String>)> = panel
        .ranges
        .iter()
        .enumerate()
        .filter(|(_, range_match)| {
            code_operation_class(&range_match.operation).is_some() && !range_match.source.is_empty()
        })
        .filter_map(|(index, range_match)| {
            range_match.source.columns_on_row(row, row_len).map(
                |(start, end)| -> (usize, usize, &TextOperation, &String, Option<&String>) {
                    (
                        start,
                        end,
                        &range_match.operation,
                        &panel.ids[index],
                        panel.counterparts.get(&index),
                    )
                },
            )
        })
        .collect();
    segments.extend(markers.iter().map(|marker| {
        let column = snap_to_char_boundary(line, marker.column);
        (
            column,
            column,
            &marker.operation,
            &marker.id,
            Some(&marker.points_at),
        )
    }));
    // End position as the secondary key, so a zero-width caret sharing a start column with a real
    // range sorts before it - the same ordering `widgets::code_viewer::build_range_order` uses.
    segments.sort_by_key(|(start, end, _, _, _)| (*start, *end));

    let mut text = String::new();
    let mut cursor = 0usize;
    let mut has_marker = false;
    for (start, end, operation, id, counterpart) in segments {
        let start = snap_to_char_boundary(line, start).max(cursor);
        let end = snap_to_char_boundary(line, end).max(start);
        if start > cursor {
            text.push_str(&escape_html_text(&line[cursor..start]));
        }
        // `code_operation_class` returned `Some` for every segment that survived the filter above,
        // so this can't be `None` - but default rather than unwrap, since a panic here would take
        // down the whole site build over one row.
        let class = code_operation_class(operation).unwrap_or("cd-update");
        let counterpart_attr = counterpart
            .map(|other| format!(" data-counterpart=\"{other}\""))
            .unwrap_or_default();
        if end > start {
            text.push_str(&format!(
                r#"<span class="cd {class}" data-range="{id}"{counterpart_attr} tabindex="0">{}</span>"#,
                escape_html_text(&line[start..end])
            ));
        } else {
            has_marker = true;
            let title = match operation {
                TextOperation::Delete => "deleted here",
                _ => "inserted here",
            };
            text.push_str(&format!(
                r#"<span class="cd cd-gap {class}" data-range="{id}"{counterpart_attr} title="{title}" tabindex="0"></span>"#
            ));
        }
        cursor = end;
    }
    if cursor < row_len {
        text.push_str(&escape_html_text(&line[cursor..]));
    }

    // The row-level class is the coarse signal (a tint across the whole row, so changed rows are
    // findable while scrolling); the spans above are the precise one. Both are wanted: the spans
    // alone are easy to scroll straight past on a long line. A row that only carries a caret gets
    // no tint - its own text really is unchanged - just a marker class so it stays findable.
    let row_class = match code_operation_class(row_op) {
        Some(class) => format!(" row-{class}"),
        None if has_marker => " row-gap".to_string(),
        None => String::new(),
    };
    format!(
        r#"<div class="cl{row_class}" id="{side}L{row}"><span class="ln">{}</span><span class="lt">{text}</span></div>"#,
        row + 1
    )
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
    /// The fixture's `description.md`, if it has one - shown under its name so the list says what
    /// each case is, not just how big it is.
    note: Option<String>,
    /// Nodes the human mapping still says nothing about. `0` is a finished mapping; anything else
    /// is work in progress, which a reader picking a fixture to review wants to know before they
    /// open it.
    unmarked_nodes: usize,
    /// Every painting's name, in file order. Empty means unpainted - see the comment at the one
    /// place this is built for why the names, and not just how many, are what gets carried.
    paintings: Vec<String>,
}

fn render_index_page(entries: &[IndexEntry]) -> String {
    let mut rows = String::new();
    for entry in entries {
        let name_attr = escape_html_attr(&entry.name);
        let name_escaped = escape_html_text(&entry.name);
        rows.push_str(&format!(
            r#"<tr data-name="{name_attr}" data-language="{language}" data-codediff="{codediff}" data-unix_diff="{unix_diff}" data-total_lines="{total_lines}" data-paintings="{painting_count}" data-unmarked="{unmarked}">
<td><a href="fixtures/{name_attr}.html">{name_escaped}</a>{note}</td>
<td><span class="language-badge">{language}</span></td>
<td>{codediff}</td>
<td>{unix_diff}</td>
<td>{total_lines}</td>
<td class="paintings">{painting_names}</td>
<td>{unmarked_cell}</td>
</tr>
"#,
            language = entry.language,
            codediff = entry.codediff_mismatches,
            unix_diff = entry.unix_diff_mismatches,
            total_lines = entry.total_lines,
            painting_count = entry.paintings.len(),
            unmarked = entry.unmarked_nodes,
            // A finished mapping is the normal state and reads as a clean cell; a count is the
            // exception worth seeing.
            unmarked_cell = if entry.unmarked_nodes == 0 {
                "&mdash;".to_string()
            } else {
                entry.unmarked_nodes.to_string()
            },
            // Under the name rather than in a column of its own: it is free prose of no fixed
            // width, and a column wide enough for the longest one would squeeze every number off
            // the screen.
            note = match &entry.note {
                Some(note) => format!(
                    r#"<div class="fixture-note">{}</div>"#,
                    escape_html_text(note.trim())
                ),
                None => String::new(),
            },
            painting_names = if entry.paintings.is_empty() {
                // Absence, not a zero: the column sorts on `data-paintings` (0 here), while the
                // cell has to read as "nobody has painted this" rather than as a count.
                "&mdash;".to_string()
            } else {
                escape_html_text(&entry.paintings.join(", "))
            },
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
<p>"Paintings" lists the hand-recorded accounts of each diff <em>as text</em>, kept independently of
the node mapping because a rendering often has several equally correct answers where the mapping has
one. Open a fixture and use the Code view's switch to flip between them and the mapping's own
rendering. A dash means nobody has painted that fixture yet.</p>
<p>"Unmarked nodes" counts what the human mapping still says nothing about - a dash means the
mapping is finished. Sort by it to find the ones that still need work. Where a fixture carries a
description, it appears under its name.</p>
</header>
<table class="fixture-table" id="fixture-table">
<thead>
<tr>
<th data-sort="name" data-type="string" tabindex="0" aria-sort="ascending">Fixture</th>
<th data-sort="language" data-type="string" tabindex="0" aria-sort="none">Language</th>
<th data-sort="codediff" data-type="number" tabindex="0" aria-sort="none">codediff mismatches</th>
<th data-sort="unix_diff" data-type="number" tabindex="0" aria-sort="none">Unix diff mismatches</th>
<th data-sort="total_lines" data-type="number" tabindex="0" aria-sort="none">Total lines</th>
<th data-sort="paintings" data-type="number" tabindex="0" aria-sort="none">Paintings</th>
<th data-sort="unmarked" data-type="number" tabindex="0" aria-sort="none">Unmarked nodes</th>
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
    use codediff::test::helper::human_mapping::{
        HumanMapping, HumanMappingEntry, HumanOperation, HumanTextEntry, HumanTextOperation,
        HumanTextSpan,
    };

    /// Undoes `escape_html_text` and strips every tag, recovering the plain text of one rendered
    /// row. Only usable on this module's own output, which emits a fixed, tiny set of tags and
    /// entities - not a general HTML parser.
    fn strip_tags(html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        for ch in html.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        // `&amp;` last: unescaping it first would turn a literal `&amp;lt;` in the source into
        // `<`, which was never there.
        out.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    /// Every rendered row of a code panel, as `(row index, plain text)`, recovered from the
    /// `id="{side}L{row}"` attribute `render_code_row` stamps on each one. Fold placeholders carry
    /// no id and are skipped.
    fn rendered_rows(html: &str, side: &str) -> Vec<(usize, String)> {
        let id_prefix = format!("\" id=\"{side}L");
        html.split("<div class=\"cl")
            .filter_map(|chunk| {
                let id_start = chunk.find(&id_prefix)? + id_prefix.len();
                let id_end = id_start + chunk[id_start..].find('"')?;
                let row: usize = chunk[id_start..id_end].parse().ok()?;
                let text_start = chunk.find("<span class=\"lt\">")? + "<span class=\"lt\">".len();
                let text_end = chunk.rfind("</span></div>")?;
                Some((row, strip_tags(&chunk[text_start..text_end])))
            })
            .collect()
    }

    /// The property that makes the code panel trustworthy at all: it renders the *file*, with the
    /// mapping's changes painted on top, not a reassembly of the mapping's own range texts.
    ///
    /// Those two look identical on a page and are not: `diff::text` ranges are whitespace-
    /// insensitive and leave gaps between themselves (leading indentation especially - the same
    /// property that forces `line_operations` to be row-granular), so a panel built by
    /// concatenating range texts silently loses the gap bytes and renders code that was never in
    /// the file. Eyeballing a generated page does not catch that; comparing the stripped rows back
    /// against the source does.
    #[test]
    fn render_code_panel_reproduces_the_source_text_exactly() {
        let mut checked = 0usize;
        let mut painted = 0usize;
        for name in codediff::test::helper::UNIT_TEST_FIXTURES {
            let Ok((before, after)) = helper::handmade_test_code_pair(name) else {
                continue;
            };
            let Ok(mapping) = human_mapping::load(name) else {
                continue;
            };
            let Ok(diff) = human_mapping::as_ast_diff_for_mapping(&mapping, &before, &after) else {
                continue;
            };
            let node_cache = NodeCache::build(&before, &after);
            let text_diff = TextDiff::from(&before, &after, &diff, &node_cache);
            let before_ranges = text_diff.all(0);
            let after_ranges = text_diff.all(1);
            let row_counts = [
                before.contents.split('\n').count(),
                after.contents.split('\n').count(),
            ];

            // Every rendering this fixture's page carries, not just the tree mapping's. A painting
            // is a second, independent producer of ranges into the same renderer - a span read off
            // hand-recorded row/column pairs rather than computed from a node - so it needs this
            // guarantee more than the derived one does, not less.
            let mut renderings = vec![[
                PanelRanges::from_tree(
                    "b",
                    "a",
                    before_ranges.clone(),
                    &after_ranges,
                    row_counts[0],
                ),
                PanelRanges::from_tree(
                    "a",
                    "b",
                    after_ranges.clone(),
                    &before_ranges,
                    row_counts[1],
                ),
            ]];
            for (index, named) in mapping.text_mappings.iter().enumerate() {
                let panels =
                    painting_panels(named, &before.contents, &after.contents, index, row_counts)
                        .unwrap_or_else(|error| {
                            panic!("'{name}' painting '{}': {error:#}", named.name)
                        });
                renderings.push(panels);
                painted += 1;
            }

            let source_lines = [
                before.contents.split('\n').collect::<Vec<&str>>(),
                after.contents.split('\n').collect::<Vec<&str>>(),
            ];
            for panels in &renderings {
                for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
                    let panel = &panels[side];
                    // Nothing folded, so every row of the file has to come back - a stricter check
                    // than any real page performs.
                    let html = render_code_panel(contents, panel, &vec![true; row_counts[side]]);
                    let rows = rendered_rows(&html, &panel.side);
                    assert!(
                        !rows.is_empty(),
                        "'{name}' panel '{}' rendered no rows at all",
                        panel.side
                    );
                    for (row, text) in rows {
                        assert_eq!(
                            text, source_lines[side][row],
                            "'{name}' panel '{}' row {row} does not reproduce its source line",
                            panel.side
                        );
                    }
                }
            }
            checked += 1;
        }
        assert!(
            checked > 0,
            "no fixture in UNIT_TEST_FIXTURES had a loadable human mapping - \
             this test would silently pass while checking nothing"
        );
        // A floor, not `> 0`: this is the only check that compares a *painted* panel's text back
        // against the file, and `UNIT_TEST_FIXTURES` is a subset of the corpus, so "some fixture
        // was painted" would be satisfied by one and leave the property essentially unmeasured.
        assert!(
            painted >= 10,
            "only {painted} painting(s) in UNIT_TEST_FIXTURES reached this check - the painted \
             half of the test is barely measuring anything"
        );
    }

    #[test]
    fn render_code_row_paints_only_the_changed_columns() {
        let ranges = vec![
            RangeMatch {
                source: codediff::diff::text_range::TextRange::new(0, 4, 0, 7),
                destination: codediff::diff::text_range::TextRange::new(0, 4, 0, 7),
                operation: TextOperation::Update,
            },
            RangeMatch {
                source: codediff::diff::text_range::TextRange::new(0, 0, 0, 4),
                destination: codediff::diff::text_range::TextRange::new(0, 0, 0, 4),
                operation: TextOperation::Identical,
            },
        ];
        let panel = PanelRanges::from_tree("b", "a", ranges, &[], 1);
        let html = render_code_row("let foo = 1;", 0, &TextOperation::Update, &panel, &[]);

        assert!(
            html.contains(r#"<span class="cd cd-update" data-range="b0" tabindex="0">foo</span>"#),
            "expected exactly the update columns to be wrapped, got: {html}"
        );
        // The Identical range contributes no span, and the untouched tail is plain text - but all
        // of it is still present, which is what `strip_tags` proves.
        assert_eq!(strip_tags(&html), "1let foo = 1;");
    }

    /// On a pure deletion the after side has no changed text at all - and, as `code_markers`'
    /// own doc comment records, no range of its own either: the mark has to be read off the
    /// before side's `destination`. Without the caret that panel is a page of unmarked context
    /// and the reader cannot tell what happened or where.
    #[test]
    fn render_code_row_draws_a_caret_for_the_other_sides_deletion() {
        use codediff::diff::text_range::TextRange;

        // A before-side delete: it owns real text on its own side, and points at a zero-width
        // position on the after side.
        let before = vec![RangeMatch {
            source: TextRange::new(7, 0, 9, 0),
            destination: TextRange::new(0, 4, 0, 4),
            operation: TextOperation::Delete,
        }];
        let panel = PanelRanges::from_tree("a", "b", Vec::new(), &before, 20);
        assert_eq!(
            panel.markers.len(),
            1,
            "the delete should produce exactly one caret"
        );

        let html = render_code_row(
            "let foo = 1;",
            0,
            &TextOperation::Identical,
            &panel,
            &panel.markers.iter().collect::<Vec<_>>(),
        );

        assert!(
            html.contains(
                r#"<span class="cd cd-gap cd-delete" data-range="am0" data-counterpart="b0" title="deleted here" tabindex="0"></span>"#
            ),
            "expected an empty caret span pointing back at the deleted text, got: {html}"
        );
        // And the other end of the pair names the caret, so clicking either reveals the other.
        assert_eq!(
            code_counterparts(&before, &[], "a", &[])
                .get(&0)
                .map(String::as_str),
            Some("am0")
        );
        // A caret marks a position, it does not change the row - so the row keeps its own text
        // intact and gets the marker class rather than a full operation tint.
        assert!(html.contains(r#"class="cl row-gap""#), "got: {html}");
        assert_eq!(strip_tags(&html), "1let foo = 1;");
    }

    #[test]
    fn code_visible_rows_keeps_context_around_a_change_and_drops_the_rest() {
        let visible = code_visible_rows(&[15], 30);

        assert!(visible[15], "the changed row itself must be visible");
        assert!(visible[15 - CODE_CONTEXT_ROWS], "context above");
        assert!(visible[15 + CODE_CONTEXT_ROWS], "context below");
        assert!(
            !visible[15 - CODE_CONTEXT_ROWS - 1],
            "beyond the context above"
        );
        assert!(
            !visible[15 + CODE_CONTEXT_ROWS + 1],
            "beyond the context below"
        );
        assert!(!visible[0] && !visible[29]);
    }

    /// The regression `anchor_rows` was written for: on a pure deletion the after side has no
    /// changed row at all, only the caret marking where the deleted text used to be. Anchoring on
    /// `line_operations` alone folds that whole panel away.
    #[test]
    fn anchor_rows_anchors_on_a_caret_with_no_changed_row() {
        use codediff::diff::text_range::TextRange;

        let panel = PanelRanges::from_tree(
            "a",
            "b",
            Vec::new(),
            &[RangeMatch {
                source: TextRange::new(3, 0, 5, 0),
                destination: TextRange::new(15, 0, 15, 0),
                operation: TextOperation::Delete,
            }],
            30,
        );
        assert_eq!(
            panel.anchor_rows(),
            vec![15],
            "the caret's row is the panel's only anchor - it has no changed row of its own"
        );

        let visible = code_visible_rows(&panel.anchor_rows(), 30);

        assert!(visible[15], "the caret's own row must be visible");
        assert!(visible[15 - CODE_CONTEXT_ROWS] && visible[15 + CODE_CONTEXT_ROWS]);
        assert!(!visible[0] && !visible[29]);
    }

    #[test]
    fn code_visible_rows_shows_everything_when_nothing_anchors() {
        assert!(code_visible_rows(&[], 30).iter().all(|v| *v));
    }

    /// The reason folding is decided across every rendering at once rather than per panel. A
    /// painting draws no carets (it records no position for a deletion on the opposite side), so
    /// on a pure deletion its after panel anchors nowhere - and on its own would fall through to
    /// "show everything", unfolding the whole file next to a tightly folded tree panel.
    #[test]
    fn a_paintings_anchors_are_unioned_with_the_tree_panels_own() {
        use codediff::diff::text_range::TextRange;

        let tree = PanelRanges::from_tree(
            "a",
            "b",
            Vec::new(),
            &[RangeMatch {
                source: TextRange::new(3, 0, 5, 0),
                destination: TextRange::new(15, 0, 15, 0),
                operation: TextOperation::Delete,
            }],
            30,
        );
        let painting = PanelRanges {
            side: "a0".to_string(),
            ranges: Vec::new(),
            ids: Vec::new(),
            counterparts: HashMap::new(),
            markers: Vec::new(),
            ops: vec![TextOperation::Identical; 30],
        };
        assert!(
            painting.anchor_rows().is_empty(),
            "a painting with nothing on this side anchors nowhere by itself"
        );

        let mut anchors = tree.anchor_rows();
        anchors.extend(painting.anchor_rows());
        let visible = code_visible_rows(&anchors, 30);

        assert!(visible[15], "the union keeps the tree panel's caret row");
        assert!(
            !visible[0] && !visible[29],
            "and it does not fall through to showing the whole file"
        );
    }

    #[test]
    fn code_counterparts_links_real_text_directly_and_a_deletion_to_its_caret() {
        use codediff::diff::text_range::TextRange;

        let before = vec![
            // Update: a real counterpart on the other side.
            RangeMatch {
                source: TextRange::new(0, 0, 0, 3),
                destination: TextRange::new(5, 0, 5, 3),
                operation: TextOperation::Update,
            },
            // Delete: the after side has no text of its own here, only the caret `code_markers`
            // puts at that position - which is what this links to instead.
            RangeMatch {
                source: TextRange::new(1, 0, 1, 3),
                destination: TextRange::new(9, 0, 9, 0),
                operation: TextOperation::Delete,
            },
        ];
        let after = vec![RangeMatch {
            source: TextRange::new(5, 0, 5, 3),
            destination: TextRange::new(0, 0, 0, 3),
            operation: TextOperation::Update,
        }];

        let after_ids = positional_ids("a", after.len());
        let links = code_counterparts(&before, &after, "a", &after_ids);

        assert_eq!(links.get(&0).map(String::as_str), Some("a0"));
        assert_eq!(links.get(&1).map(String::as_str), Some("am1"));
    }

    /// A painting spans `HumanTextSpan`s covering rows/byte-columns of the file, so `to_text_range`
    /// is the whole of the geometry - but the ids are the part that carries meaning a reader can
    /// act on, and they are this module's invention rather than the data's.
    fn painting(entries: Vec<HumanTextEntry>) -> human_mapping::NamedTextMapping {
        human_mapping::NamedTextMapping {
            name: "Minimal".to_string(),
            mapping: human_mapping::HumanTextMapping { entries },
        }
    }

    fn span(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
    ) -> HumanTextSpan {
        HumanTextSpan {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }

    /// An N:M match asserts a correspondence *whole* - which before span pairs with which after
    /// span is explicitly not recorded (see `HumanTextEntry`). So every span on a side gets the
    /// entry's one id, and points at the other side's one id: clicking any of them reveals all of
    /// the counterpart, and no pairing is invented to make the link.
    #[test]
    fn painting_panels_give_every_span_of_one_match_a_single_shared_id() {
        // Three `foo` on the before side, two on the after - identical text throughout, which is
        // what makes the group well formed and its pairing genuinely arbitrary.
        let before = "foo\nfoo\nfoo\n";
        let after = "foo\nfoo\n";
        let named = painting(vec![HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 3), span(1, 0, 1, 3), span(2, 0, 2, 3)],
            after: vec![span(0, 0, 0, 3), span(1, 0, 1, 3)],
        }]);

        let [before_panel, after_panel] =
            painting_panels(&named, before, after, 0, [4, 3]).unwrap();

        assert_eq!(before_panel.ids, vec!["b0e0".to_string(); 3]);
        assert_eq!(after_panel.ids, vec!["a0e0".to_string(); 2]);
        for index in 0..3 {
            assert_eq!(
                before_panel.counterparts.get(&index).map(String::as_str),
                Some("a0e0"),
                "every before span points at the whole after side of the entry"
            );
        }
        for index in 0..2 {
            assert_eq!(
                after_panel.counterparts.get(&index).map(String::as_str),
                Some("b0e0")
            );
        }
        // Identical text on both sides is a move, not an update - derived from the spans, never
        // recorded by the painter.
        assert!(
            before_panel
                .ranges
                .iter()
                .all(|range| range.operation == TextOperation::Move)
        );
    }

    /// The one thing a painting records that the tree mapping's projection does not, and the one
    /// thing it doesn't. A `Delete` says where text *was*; it says nothing about where its absence
    /// sits on the after side, so there is no caret to draw and nothing to link to.
    #[test]
    fn painting_panels_draw_no_caret_for_a_one_sided_entry() {
        let before = "keep\ngone\n";
        let after = "keep\n";
        let named = painting(vec![HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(1, 0, 1, 4)],
            after: vec![],
        }]);

        let [before_panel, after_panel] =
            painting_panels(&named, before, after, 1, [3, 2]).unwrap();

        assert_eq!(before_panel.ranges.len(), 1);
        assert_eq!(before_panel.ranges[0].operation, TextOperation::Delete);
        assert!(
            before_panel.counterparts.is_empty(),
            "a delete has nothing on the other side to point at"
        );
        assert!(after_panel.ranges.is_empty() && after_panel.markers.is_empty());
        assert!(
            before_panel.markers.is_empty(),
            "painted panels never carry carets - only the tree projection computes those positions"
        );
    }

    /// `viewer.js` looks `data-range` ids up document-wide, and a fixture page now stacks several
    /// renderings of the same two files - so a shared prefix would have a click in the visible
    /// panel selecting, and scrolling to, spans inside a hidden one.
    #[test]
    fn each_rendering_gets_its_own_dom_id_prefix() {
        let source = "foo\n";
        let tree = PanelRanges::from_tree("b", "a", Vec::new(), &[], 2);
        let named = painting(vec![HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(0, 0, 0, 3)],
            after: vec![],
        }]);
        let [first, _] = painting_panels(&named, source, source, 0, [2, 2]).unwrap();
        let [second, _] = painting_panels(&named, source, source, 1, [2, 2]).unwrap();

        let sides = [tree.side.clone(), first.side.clone(), second.side.clone()];
        assert_eq!(sides, ["b".to_string(), "b0".to_string(), "b1".to_string()]);
        assert!(
            first.ids.iter().all(|id| id.starts_with("b0")),
            "got: {:?}",
            first.ids
        );
        assert!(second.ids.iter().all(|id| id.starts_with("b1")));
    }

    /// A malformed painting - here a span past the end of its file - is a data problem in one
    /// fixture, and the site build renders 500 of them. It costs its own panel and gets reported;
    /// it does not take the page, let alone the run, down with it.
    #[test]
    fn an_unreadable_painting_is_reported_and_skipped_rather_than_failing_the_page() {
        let source = "fn f() {}\n";
        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);
        let mapping = HumanMapping {
            entries: vec![],
            text_mappings: vec![painting(vec![HumanTextEntry {
                operation: HumanTextOperation::Delete,
                before: vec![span(99, 0, 99, 4)],
                after: vec![],
            }])],
            ..Default::default()
        };

        let mut warnings = Vec::new();
        let html = render_fixture_page(
            "rust-add-if",
            &before,
            &after,
            &mapping,
            None,
            None,
            &mut warnings,
        )
        .expect("a bad painting must not fail the page")
        .html;

        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(
            warnings[0].contains("rust-add-if") && warnings[0].contains("Minimal"),
            "the warning has to name the fixture and the painting: {warnings:?}"
        );
        assert!(
            !html.contains("painting-switch"),
            "with the only painting skipped there is one rendering left, and one rendering needs \
             no chooser"
        );
    }

    /// The feature, end to end: a painted fixture's page carries the mapping's own rendering plus
    /// one panel per painting, a button for each, and exactly one of them visible.
    #[test]
    fn a_painted_fixture_page_stacks_one_code_panel_per_painting() {
        let source = "fn f() {}\n";
        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);
        let mut minimal = painting(vec![]);
        minimal.name = "Minimal".to_string();
        let mut full = painting(vec![]);
        full.name = "Full".to_string();
        let mapping = HumanMapping {
            entries: vec![],
            text_mappings: vec![minimal, full],
            ..Default::default()
        };

        let mut warnings = Vec::new();
        let html = render_fixture_page(
            "rust-add-if",
            &before,
            &after,
            &mapping,
            None,
            None,
            &mut warnings,
        )
        .expect("should render")
        .html;

        assert!(warnings.is_empty(), "got: {warnings:?}");
        for key in ["tree", "p0", "p1"] {
            assert!(
                html.contains(&format!(
                    r#"<div class="panels code-panels" data-painting="{key}""#
                )) || html.contains(&format!(
                    r#"<div class="panels code-panels hidden" data-painting="{key}""#
                )),
                "expected a code panel for '{key}': {html}"
            );
        }
        // Remembered across fixtures by name, not by the `p0`/`p1` handle - see the switch's own
        // comment in `viewer.js`.
        assert!(html.contains(r#"data-painting-name="Minimal""#));
        assert!(html.contains(r#"data-painting-name="Full""#));
        // The node mapping is what the page opens on, and it is the only pressed button in this
        // group - the view switch has its own, which is why this counts buttons carrying a
        // `data-painting` rather than every pressed button on the page.
        assert!(html.contains(r#"data-painting="tree" data-painting-name="From the node mapping" aria-pressed="true""#), "got: {html}");
        for (key, name) in [("p0", "Minimal"), ("p1", "Full")] {
            assert!(
                html.contains(&format!(
                    r#"data-painting="{key}" data-painting-name="{name}" aria-pressed="false""#
                )),
                "expected an unpressed button for '{name}': {html}"
            );
        }
        assert_eq!(
            html.matches("code-panels hidden").count(),
            2,
            "the two paintings start hidden: {html}"
        );
    }

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

        let html = render_fixture_page(
            "rust-add-if",
            &before,
            &after,
            &mapping,
            None,
            None,
            &mut Vec::new(),
        )
        .expect("should render")
        .html;

        assert!(
            html.contains(
                r#"href="https://github.com/ivankovic/codediff/tree/main/src/test/data/diffs/handmade/rust-add-if""#
            ),
            "expected a link straight to this fixture's own before/after files: {html}"
        );
    }

    /// The other link in the header, and the one this site could not draw at all until the sample
    /// provenance reached it: the upstream commit the two files were cut from.
    #[test]
    fn render_fixture_page_links_to_the_upstream_commit_when_there_is_one() {
        let source = "fn f() {}\n";
        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);
        let mapping = HumanMapping::default();
        let upstream = Upstream {
            commit_url:
                "https://github.com/awslabs/aws-c-common/commit/fbb21230e117d2afe49d03ebe8605270dacb4ab3"
                    .to_string(),
            repository: "awslabs-aws-c-common".to_string(),
            commit: "fbb21230e117d2afe49d03ebe8605270dacb4ab3".to_string(),
            path: "include/aws/common/file.h".to_string(),
        };

        let html = render_fixture_page(
            "rust-add-if",
            &before,
            &after,
            &mapping,
            Some("Only insert, nothing else."),
            Some(&upstream),
            &mut Vec::new(),
        )
        .expect("should render")
        .html;

        assert!(
            html.contains(
                r#"href="https://github.com/awslabs/aws-c-common/commit/fbb21230e117d2afe49d03ebe8605270dacb4ab3""#
            ),
            "expected a link to the upstream commit: {html}"
        );
        // Abbreviated in the link text, next to the repository - a bare hash names no project.
        assert!(
            html.contains(">Upstream commit awslabs-aws-c-common@fbb2123</a>"),
            "got: {html}"
        );
        assert!(
            html.contains(">include/aws/common/file.h</span>"),
            "got: {html}"
        );
        assert!(
            html.contains(r#"<p class="description">Only insert, nothing else.</p>"#),
            "the fixture's own note belongs on its page: {html}"
        );
    }

    /// A handmade fixture was never sampled, so it has no upstream commit to point at - and the
    /// header has to simply not carry that link rather than carry a broken one.
    #[test]
    fn render_fixture_page_has_no_upstream_link_without_provenance() {
        let source = "fn f() {}\n";
        let before = Code::from_string(source, &Language::Rust);
        let after = Code::from_string(source, &Language::Rust);

        let html = render_fixture_page(
            "rust-add-if",
            &before,
            &after,
            &HumanMapping::default(),
            None,
            None,
            &mut Vec::new(),
        )
        .expect("should render")
        .html;

        assert!(!html.contains("Upstream commit"), "got: {html}");
        assert!(!html.contains("class=\"description\""), "got: {html}");
    }

    /// One end of a cross-language pin. `viewer.js`'s `nodePath` walks the HTML this file emits and
    /// rebuilds the same `kind:occurrence` path `helper::path_for_node` produces from the tree - it
    /// has to, because that path goes into the "file an issue" body, and a path that doesn't
    /// resolve is a silent failure: the reader gets a plausible-looking path naming no node.
    ///
    /// Two implementations of one format, so neither can be pinned to itself. This asserts the
    /// Rust side of a shared example; `assets/mapping_site/viewer.test.js` asserts that its
    /// `nodePath` produces the identical string for the identical tree. Change one and the other
    /// fails.
    #[test]
    fn path_for_node_agrees_with_viewer_js_on_a_shared_example() {
        let source = "fn f() {\n    let a = 1;\n    let b = 2;\n}\n";
        let tree = parse_rust(source);

        // The `2` in `let b = 2` - deliberately the *second* `let_declaration`, so the example
        // exercises the same-kind sibling counting rather than a path of all-firsts.
        let node = helper::node_for_path(
            tree.root_node(),
            &[
                "function_item:1",
                "block:1",
                "let_declaration:2",
                "integer_literal:1",
            ],
        )
        .expect("the example path should resolve");

        assert_eq!(
            helper::path_for_node(node).join("/"),
            "function_item:1/block:1/let_declaration:2/integer_literal:1",
            "if this string changes, change it in viewer.test.js too - they pin each other"
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
                paintings: vec!["Minimal".to_string(), "Full".to_string()],
                note: Some("Requires a N:M match for perfect solution".to_string()),
                unmarked_nodes: 0,
            },
            IndexEntry {
                name: "c-linux-small-bugfix".to_string(),
                language: Language::C,
                codediff_mismatches: 1,
                unix_diff_mismatches: 5,
                total_lines: 12,
                paintings: Vec::new(),
                note: None,
                unmarked_nodes: 7,
            },
        ];

        let html = render_index_page(&entries);

        assert!(html.contains(r#"href="fixtures/rust-add-if.html""#));
        assert!(html.contains(">rust-add-if<"));
        assert!(html.contains(">Rust<"));
        assert!(html.contains(r#"href="fixtures/c-linux-small-bugfix.html""#));
        assert!(html.contains(">C<"));
        // Names, not a count: "painted twice" and "painted once" are different answers to the same
        // fixture, and an unpainted one has to read as absent rather than as a zero.
        assert!(
            html.contains(">Minimal, Full</td>"),
            "expected a painted fixture to list its paintings by name: {html}"
        );
        assert!(
            html.contains(">&mdash;</td>"),
            "expected an unpainted fixture to read as absent: {html}"
        );
        // A description belongs under its fixture's name, not in a column of its own.
        assert!(
            html.contains(
                r#"<div class="fixture-note">Requires a N:M match for perfect solution</div>"#
            ),
            "expected the fixture's description under its name: {html}"
        );
        assert!(
            html.contains(r#"data-unmarked="7""#) && html.contains(">7</td>"),
            "expected the unfinished mapping's unmarked-node count: {html}"
        );
    }

    #[test]
    fn render_index_page_puts_each_mismatch_count_in_its_own_sortable_column() {
        let entries = vec![IndexEntry {
            name: "rust-add-if".to_string(),
            language: Language::Rust,
            codediff_mismatches: 2,
            unix_diff_mismatches: 9,
            total_lines: 40,
            paintings: vec!["Only one solution".to_string()],
            note: None,
            unmarked_nodes: 4,
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
        assert!(
            html.contains(r#"data-paintings="1""#),
            "expected the row to carry how many paintings the fixture has: {html}"
        );
        assert!(
            html.contains(r#"data-unmarked="4""#),
            "expected the row to carry how much of the mapping is still unmarked: {html}"
        );
        for sort_key in [
            "name",
            "language",
            "codediff",
            "unix_diff",
            "total_lines",
            "paintings",
            "unmarked",
        ] {
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
            paintings: Vec::new(),
            note: None,
            unmarked_nodes: 0,
        }];
        let html = render_index_page(&entries);
        assert!(html.contains("a&amp;b"));
        assert!(!html.contains("a&b<"));
    }
}
