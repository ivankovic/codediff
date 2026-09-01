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

//! [`crate::code::similarity`] measured against two real corpus fixtures, rather than against
//! synthetic sets of integers as its own unit tests are.
//!
//! Both fixtures are here because they are the two halves of the question the sketch was built to
//! answer, and a synthetic test cannot stand in for either: `yaml-draios-sysdig-string-url-change`
//! is six near-identical URLs permuted inside one ordered `flow_sequence` - the exact shape that
//! defeated the 2026-08-17 crossed-sibling repair, whose `sequence_edit_cost` estimator could only
//! see "identical" or "not identical" - and `css-wordpress-reformat` is the near-miss the equality
//! hashes have to call "different": two declarations separated by one added `;`.

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::code::Code;
    use crate::code::similarity::SimilaritySketch;

    /// The widest flat container in a tree: the node with the most direct children, its children
    /// in document order. Both fixtures put the siblings of interest under exactly one such
    /// container (a YAML `flow_sequence`, a CSS rule `block`).
    fn widest_container(code: &Code) -> Vec<usize> {
        let metadata = code.metadata.ast_metadata.as_ref().expect("parsed");
        let mut best: Vec<usize> = Vec::new();
        for info in metadata.node_info.values() {
            if info.children.len() > best.len() {
                best = info.children.clone();
            }
        }
        // `node_info` is a hash map and ties above are broken arbitrarily; document order is
        // restored here so the returned indices mean something stable.
        best.sort_by_key(|c| metadata.node_info[c].start_byte);
        best
    }

    fn sketches(code: &Code, ids: &[usize]) -> Vec<SimilaritySketch> {
        let metadata = code.metadata.ast_metadata.as_ref().expect("parsed");
        ids.iter()
            .map(|id| metadata.node_to_similarity_sketch[id].clone())
            .collect()
    }

    fn kinds(code: &Code, ids: &[usize]) -> Vec<String> {
        let metadata = code.metadata.ast_metadata.as_ref().expect("parsed");
        ids.iter()
            .map(|id| metadata.node_info[id].kind.clone())
            .collect()
    }

    fn load(name: &str) -> Result<(Vec<SimilaritySketch>, Vec<SimilaritySketch>, Vec<String>)> {
        let pairs = crate::test::helper::handmade_test_code_pairs_for(&[name])?;
        let (before, after) = &**pairs.get(name).expect("fixture present");
        let (bc, ac) = (widest_container(before), widest_container(after));
        let names = kinds(before, &bc);
        Ok((sketches(before, &bc), sketches(after, &ac), names))
    }

    #[test]
    fn sketch_recovers_a_permutation_of_near_identical_yaml_urls() -> Result<()> {
        let (before, after, kinds) = load("yaml-draios-sysdig-string-url-change")?;

        // The fixture permutes six URLs; the file is otherwise unchanged, so every `flow_node`
        // has exactly one true counterpart and the sketch should point straight at it.
        let expected: Vec<(usize, usize)> = vec![(1, 3), (3, 9), (5, 1), (7, 5), (9, 11), (11, 7)];
        for (b, a) in expected {
            assert_eq!(kinds[b], "flow_node", "fixture shape changed at index {b}");

            let scores: Vec<f32> = after.iter().map(|s| before[b].jaccard(s)).collect();
            let best = scores
                .iter()
                .enumerate()
                .max_by(|x, y| x.1.total_cmp(y.1))
                .map(|(i, _)| i)
                .expect("non-empty");
            assert_eq!(
                best, a,
                "before[{b}] should be most similar to after[{a}], got {best}: {scores:?}"
            );
            assert_eq!(scores[a], 1.0, "true counterparts are byte-identical here");

            // The margin matters as much as the argmax: a decision rule needs the runner-up to be
            // clearly worse, not merely worse. Measured 2026-08-18 at 1.00 vs 0.33 - the URLs
            // share their quote tokens and nothing else, since tree-sitter-yaml keeps the string
            // body in gap text rather than in a child node.
            let runner_up = scores
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != a)
                .map(|(_, s)| *s)
                .fold(0.0f32, f32::max);
            assert!(
                runner_up < 0.5,
                "before[{b}]'s runner-up scored {runner_up}, too close to call"
            );
        }
        Ok(())
    }

    #[test]
    fn sketch_grades_the_near_miss_the_equality_hashes_call_different() -> Result<()> {
        let (before, after, kinds) = load("css-wordpress-reformat")?;
        assert_eq!(kinds[1], "declaration", "fixture shape changed");

        // `margin-top: ...` vs `margin-bottom: ...` - same property shape, different property,
        // one differing token out of a handful. Every Merkle hash in `code::hash` reports only
        // "different"; the sketch has to report "mostly the same" for the crossed-sibling repair
        // this was built for to have anything to decide on.
        let near_miss = before[3].jaccard(&after[2]);
        assert!(
            (0.5..1.0).contains(&near_miss),
            "near-miss declarations scored {near_miss}, wanted clearly-similar-but-not-equal"
        );

        // ...while an unrelated declaration in the same block must stay far below it, or the
        // measure is just reporting "both are declarations".
        let unrelated = before[1].jaccard(&after[2]);
        assert!(
            unrelated < near_miss * 0.5,
            "unrelated declaration scored {unrelated} against a near-miss of {near_miss}"
        );
        Ok(())
    }
}
