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
use tree_sitter::Node;

use crate::diff::PassCtx;
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason};

/// Fixes up one attribution gap phase 1's hash descent structurally cannot close: when a class or
/// interface gains a heritage clause (`class Foo implements Bar`, `interface Foo extends Bar`),
/// its body is byte-identical before and after but sits at a different row/column, because the
/// heritage clause is a *new* sibling inserted before the body, not because the body itself moved.
/// Phase 1 already matches the body correctly (`Identical`, by hash) - this pass only re-tags that
/// existing match's `reason` so `ranges()` can recognize it as a verified pure repositioning and
/// skip painting it `Move`, the same mechanism `solve_nested_condition_collapse` established for
/// Rust's let-chain collapse. It never creates a new mapping.
///
/// Two general rendering heuristics for this same "shift explained by an unrelated preceding
/// insertion" shape were tried and reverted in `ranges()` itself (see the Move/Identical branch's
/// own history and `RenderOptions::paint_reindent_only_moves`'s doc comment): one keyed on parent-
/// match alone, one on nearest-mapped-sibling adjacency, applied to *any* node. Both broke other
/// fixtures - `rust-add-if` (a genuine relocation into a new `if`) and a JS destructuring rewrite
/// where the human ground truth *itself* wants a coincidentally-duplicated literal painted `Move`.
/// Neither failure is reachable here: this pass only ever looks at `class_body`/`interface_body`
/// nodes whose immediate parent is `class_declaration`/`interface_declaration`, which by
/// construction excludes both counter-examples' node kinds entirely - narrow-by-shape rather than
/// narrow-by-threshold.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    // A snapshot, not a live iterator: this pass only ever mutates a mapping's `reason` in place,
    // never adds or removes entries, so a snapshot can't miss or duplicate a candidate - unlike
    // `solve_leading_siblings`'s chain-walk, nothing here depends on seeing another candidate's
    // own outcome mid-loop.
    let candidates: Vec<(usize, usize)> = diff
        .mapping
        .iter()
        .filter(|(_, mapping)| mapping.operation == ASTMappingOperation::Identical)
        .map(|(&ids, _)| ids)
        .collect();

    for (before_body_id, after_body_id) in candidates {
        let Some(&before_body) = node_cache.before.get(&before_body_id) else {
            continue;
        };
        let Some(&after_body) = node_cache.after.get(&after_body_id) else {
            continue;
        };
        if !is_body_kind(before_body.kind()) || before_body.kind() != after_body.kind() {
            continue;
        }
        let Some(before_class) = before_body.parent() else {
            continue;
        };
        let Some(after_class) = after_body.parent() else {
            continue;
        };
        if !is_declaration_kind(before_class.kind()) || before_class.kind() != after_class.kind() {
            continue;
        }

        // Only a real re-tag if the body's position actually moved - an unchanged file (e.g. a
        // class body untouched between before and after) must keep whatever reason phase 1 gave
        // it (typically `IdenticalHashOfAncestor`), not get relabeled just because the adjacency
        // check below is trivially satisfied when nothing shifted at all.
        if before_body.start_position() == after_body.start_position() {
            continue;
        }

        // Belt and suspenders on top of the `Identical` operation (itself already hash-verified):
        // compare the actual bytes directly, so a hash collision could never smuggle a real change
        // through this pass.
        if before_body.utf8_text(before_src) != after_body.utf8_text(after_src) {
            continue;
        }

        if !shift_explained_by_preceding_insertion(before_body, after_body, diff) {
            continue;
        }

        if let Some(mapping) = diff.mapping.get_mut(&(before_body_id, after_body_id)) {
            mapping.reason = ASTMappingReason::HeritageClauseGrowth;
        }
    }
}

fn is_body_kind(kind: &str) -> bool {
    matches!(kind, "class_body" | "interface_body")
}

fn is_declaration_kind(kind: &str) -> bool {
    matches!(kind, "class_declaration" | "interface_declaration")
}

/// Nearest tree-sitter sibling *before* `node` that has an entry in `node_map` (before-tree id ->
/// after-tree id, or vice versa depending which map is passed), walking back past any number of
/// unmapped siblings. `None` means `node` is the first mapped child among its siblings.
fn nearest_mapped_prev_sibling_id(
    mut node: Node,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> Option<usize> {
    while let Some(prev) = node.prev_sibling() {
        if node_map.contains_key(&prev.id()) {
            return Some(prev.id());
        }
        node = prev;
    }
    None
}

/// Whether `before_body`'s shift to `after_body` is fully explained by content inserted *before*
/// it among its own matched siblings, rather than the body itself having a different structural
/// position: each side's nearest already-matched preceding sibling must be the other's
/// counterpart (or both absent - both are the first matched child). Nothing with an existing
/// identity was reordered around the body; only a brand-new heritage clause appeared ahead of it.
fn shift_explained_by_preceding_insertion(
    before_body: Node,
    after_body: Node,
    diff: &ASTDiff,
) -> bool {
    let before_pred = nearest_mapped_prev_sibling_id(before_body, &diff.before_node_map);
    let after_pred = nearest_mapped_prev_sibling_id(after_body, &diff.after_node_map);
    match (before_pred, after_pred) {
        (Some(before_pred_id), Some(after_pred_id)) => {
            diff.before_node_map.get(&before_pred_id) == Some(&after_pred_id)
        }
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::diff_code;

    #[test]
    fn class_body_pushed_by_a_new_implements_clause_is_tagged() {
        let body = "    constructor(public name: string, public email: string) {}\n    \
                     getName(): string {\n        return this.name;\n    }\n    \
                     getEmail(): string {\n        return this.email;\n    }\n";
        let before =
            Code::from_string(&format!("class User {{\n{body}}}\n"), &Language::TypeScript);
        let after = Code::from_string(
            &format!(
                "interface Person {{\n    name: string;\n}}\n\nclass User implements Person {{\n{body}}}\n"
            ),
            &Language::TypeScript,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        let tagged = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::HeritageClauseGrowth)
            .count();
        assert_eq!(
            tagged, 1,
            "class User's body should be re-tagged as a verified pure repositioning"
        );
    }

    #[test]
    fn interface_body_pushed_by_a_new_extends_clause_is_tagged() {
        let body = "    name: string;\n    email: string;\n    age: number;\n    active: boolean;\n    \
                     getName(): string;\n    getEmail(): string;\n    getAge(): number;\n    isActive(): boolean;\n";
        let before = Code::from_string(
            &format!("interface Named {{\n{body}}}\n"),
            &Language::TypeScript,
        );
        let after = Code::from_string(
            &format!(
                "interface Aged {{\n    age: number;\n    unit: string;\n}}\n\ninterface Named extends Aged {{\n{body}}}\n"
            ),
            &Language::TypeScript,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        let tagged = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::HeritageClauseGrowth)
            .count();
        assert_eq!(
            tagged, 1,
            "interface Named's body should be re-tagged as a verified pure repositioning"
        );
    }

    /// A body that actually changed content must never be tagged, even if its class also gained a
    /// heritage clause in the same edit - the byte-identical check is the guard.
    #[test]
    fn a_body_whose_content_also_changed_is_left_alone() {
        let body = "    constructor(public name: string, public email: string) {}\n    \
                     getName(): string {\n        return this.name;\n    }\n    \
                     getEmail(): string {\n        return this.email;\n    }\n";
        let before =
            Code::from_string(&format!("class User {{\n{body}}}\n"), &Language::TypeScript);
        let after = Code::from_string(
            &format!(
                "interface Person {{\n    name: string;\n}}\n\nclass User implements Person {{\n{body}    newField: string;\n}}\n"
            ),
            &Language::TypeScript,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        assert!(
            diff.mapping
                .values()
                .all(|m| m.reason != ASTMappingReason::HeritageClauseGrowth),
            "a body that gained a new member is a real edit, not a pure repositioning"
        );
    }
}
