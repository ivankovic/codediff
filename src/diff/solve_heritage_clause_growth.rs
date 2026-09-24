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

/// Re-tags a shifted but byte-identical `class_body`/`interface_body` as `HeritageClauseGrowth` when
/// its class gained a heritage clause (`implements Bar`), so `ranges()` does not paint it `Move`.
/// Never creates a mapping.
///
/// Keyed on these kinds rather than a general rule in `ranges()`: "shift explained by a preceding
/// insertion" is not decidable from parent match or sibling adjacency, since a genuine relocation
/// has the same geometry. These kinds gain a heritage clause and nothing else.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

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
        // Parent ids from the metadata, not `Node::parent()` (an O(depth) walk from the root).
        let Some(&before_class) = ctx
            .before_metadata()
            .node_to_parent
            .get(&before_body.id())
            .and_then(|id| node_cache.before.get(id))
        else {
            continue;
        };
        let Some(&after_class) = ctx
            .after_metadata()
            .node_to_parent
            .get(&after_body.id())
            .and_then(|id| node_cache.after.get(id))
        else {
            continue;
        };
        if !is_declaration_kind(before_class.kind()) || before_class.kind() != after_class.kind() {
            continue;
        }

        // An unshifted body keeps phase 1's reason; the adjacency check is trivially true for it.
        if before_body.start_position() == after_body.start_position() {
            continue;
        }

        // Byte comparison on top of the hash, so a collision cannot smuggle a change through.
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

/// Nearest preceding sibling of `node` with an entry in `node_map`, skipping unmapped ones.
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

/// Whether each side's nearest mapped preceding sibling is the other's counterpart (or both are
/// absent): nothing with an identity was reordered around the body, only new content appeared.
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

    #[test]
    fn a_body_that_did_not_shift_keeps_its_reason() {
        let before = Code::from_string(
            "class A {\n    x: number;\n}\nlet y = 1;\n",
            &Language::TypeScript,
        );
        let after = Code::from_string(
            "class A {\n    x: number;\n}\nlet y = 2;\n",
            &Language::TypeScript,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        assert!(
            diff.mapping
                .values()
                .all(|m| m.reason != ASTMappingReason::HeritageClauseGrowth)
        );
    }
}
