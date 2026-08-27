// Plain-Node regression test for viewer.js's logic - no framework, no npm dependency, no DOM,
// consistent with this directory's own "no framework, no build step" convention (see index.js's
// header comment). Run directly: `node assets/mapping_site/viewer.test.js` (also wired into
// `make test-mapping-site-js` and CI - see Makefile/`.github/workflows/ci.yml`).
//
// viewer.js is mostly DOM wiring, which this deliberately does not try to fake. What it covers is
// the logic underneath: the path format shared with the Rust side, the search wrap-around, and the
// remembered-rendering fallback. Those are the parts where a bug is silent rather than obvious -
// nothing on screen looks broken, the answer is just wrong.
"use strict";

const assert = require("assert");
const { otherSide, nodePath, nextMatchIndex, pickRendering } = require("./viewer.js");

// ── A stand-in for the elements generate_mapping_site.rs's `render_node` emits ────────────────
//
// Only what nodePath actually reads: parentElement, children, classList.contains and dataset.kind.
// Real pages also put a <summary> inside every non-leaf node, so one is modelled here to keep the
// shape honest. Note that nodePath's "node" class filter is defensive rather than load-bearing: a
// <summary> carries no data-kind, so it can never match the kind being counted either way. What is
// load-bearing - and what the assertion below actually pins - is that only *same-kind* siblings
// are counted, which is what makes `let_declaration:2` mean the second let rather than the fourth
// child.
function el(className, kind, children) {
  const node = {
    classList: { contains: (c) => className.split(" ").includes(c) },
    dataset: { kind },
    children: children || [],
    parentElement: null,
  };
  for (const child of node.children) child.parentElement = node;
  return node;
}
const node = (kind, children) => el("node", kind, children);
const leaf = (kind) => el("node leaf", kind);
const summary = () => el("summary", undefined);

// ── nodePath ─────────────────────────────────────────────────────────────────────────────────
//
// The other end of a cross-language pin. This is the same tree, and the same expected string, as
// `path_for_node_agrees_with_viewer_js_on_a_shared_example` in src/bin/generate_mapping_site.rs:
// two implementations of one format, so neither is pinned only to itself. The path goes into the
// "file an issue" body, so getting it wrong hands the reader a plausible path naming no node.
//
//     fn f() {
//         let a = 1;
//         let b = 2;
//     }
{
  const literalB = leaf("integer_literal");
  const letB = node("let_declaration", [
    summary(),
    leaf("let"),
    leaf("identifier"),
    leaf("="),
    literalB,
    leaf(";"),
  ]);
  const block = node("block", [
    summary(),
    leaf("{"),
    node("let_declaration", [
      summary(),
      leaf("let"),
      leaf("identifier"),
      leaf("="),
      leaf("integer_literal"),
      leaf(";"),
    ]),
    letB,
    leaf("}"),
  ]);
  const root = node("source_file", [
    summary(),
    node("function_item", [summary(), leaf("fn"), leaf("identifier"), node("parameters", []), block]),
  ]);
  // Wire the parents the constructor could not: `el` links only its own direct children, and the
  // tree above is built inside-out.
  (function link(parent) {
    for (const child of parent.children) {
      child.parentElement = parent;
      link(child);
    }
  })(root);

  assert.strictEqual(
    nodePath(literalB),
    "function_item:1/block:1/let_declaration:2/integer_literal:1",
    "if this string changes, change it in generate_mapping_site.rs too - they pin each other"
  );

  // The root itself contributes no segment: its parent is not a node, which is where the walk
  // stops. Matches the Rust side, whose loop ends when `parent()` is None.
  assert.strictEqual(nodePath(root), "");

  // A path whose every segment is an all-firsts walk, as the simplest shape - and the one that
  // would still pass if occurrence counting were broken, which is why the assertion above uses the
  // *second* let_declaration instead.
  assert.strictEqual(nodePath(block), "function_item:1/block:1");
}

// ── nextMatchIndex ───────────────────────────────────────────────────────────────────────────
{
  const texts = ["alpha", "beta", "gamma", "beta again"];

  // Starts at startIndex + 1, so searching again from a match moves on rather than finding the
  // same node forever.
  assert.strictEqual(nextMatchIndex(texts, -1, "beta"), 1);
  assert.strictEqual(nextMatchIndex(texts, 1, "beta"), 3);

  // ...and wraps, so the last match leads back to the first.
  assert.strictEqual(nextMatchIndex(texts, 3, "beta"), 1);

  // The node it started on is checked last rather than skipped: with one match in the list,
  // searching from that match finds it again instead of reporting nothing.
  assert.strictEqual(nextMatchIndex(texts, 2, "gamma"), 2);

  assert.strictEqual(nextMatchIndex(texts, 0, "ALPHA"), 0, "search is case-insensitive");
  assert.strictEqual(nextMatchIndex(texts, 0, "nothing here"), -1);
  assert.strictEqual(nextMatchIndex([], 0, "beta"), -1);
  assert.strictEqual(nextMatchIndex(texts, 0, ""), -1, "an empty query matches nothing, not everything");
}

// ── pickRendering ────────────────────────────────────────────────────────────────────────────
//
// The contract that makes remembering a painting by name safe across fixtures. Most of the corpus
// is unpainted, so the stored name usually names a rendering this page does not have.
{
  const painted = ["From the node mapping", "Minimal", "Full"];

  assert.strictEqual(pickRendering(painted, "Full"), 2);
  assert.strictEqual(
    pickRendering(painted, "Only one solution"),
    0,
    "a name this page doesn't have falls back to the node mapping, which every page has"
  );
  assert.strictEqual(pickRendering(["From the node mapping"], "Minimal"), 0);
  assert.strictEqual(pickRendering([], "Minimal"), -1, "a page with no renderings selects nothing");
}

// ── otherSide ────────────────────────────────────────────────────────────────────────────────
{
  assert.strictEqual(otherSide("before"), "after");
  assert.strictEqual(otherSide("after"), "before");
}

console.log("viewer.test.js: all assertions passed");
