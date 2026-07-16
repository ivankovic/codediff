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
// Split out of common.rs (formerly its trailing #[cfg(test)] mod tests block, ~1760 of
// common.rs's then-4119 lines) purely to shrink that file's visible size - no behavior change.

    use super::super::engine::*;
    use super::*;
    use crate::diff::{ASTMappingOperation, ASTMappingReason};
    use crate::test::helper;

    /// `ContainmentCtx::adjust` walks `node_to_parent` (via `is_ancestor_or_self`) to decide
    /// whether a candidate rename target actually contains a pruned descendant's landing spot -
    /// left empty, every non-root node looks parentless and `adjust` degenerates to forbidding
    /// almost everything. Derived from `node_info`'s children lists, same as
    /// `compute_ast_metadata`'s real one.
    fn node_to_parent_from(node_info: &HashMap<usize, ASTNodeMetadata>) -> rustc_hash::FxHashMap<usize, usize> {
        let mut parents = rustc_hash::FxHashMap::default();
        for (&id, info) in node_info {
            for &child in &info.children {
                parents.insert(child, id);
            }
        }
        parents
    }

    fn synthetic_meta(nodes: &[(usize, &str, &str, &[usize])]) -> ASTMetadata {
        let mut node_info = HashMap::new();
        for &(id, kind, text, children) in nodes {
            node_info.insert(
                id,
                ASTNodeMetadata {
                    kind: kind.to_string(),
                    text: text.to_string(),
                    children: children.to_vec(),
                    start_byte: id,
                    preorder_index: id,
                },
            );
        }
        let node_to_parent = node_to_parent_from(&node_info);
        ASTMetadata {
            node_info,
            node_to_parent,
            ..Default::default()
        }
    }

    fn mapping_total_cost(
        decisions: &[RawDecision],
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        cost_model: &UnitCostModel,
    ) -> u64 {
        decisions
            .iter()
            .map(|d| match *d {
                RawDecision::Match(b, a) => {
                    cost_model.ren(&before_meta.node_info[&b], &after_meta.node_info[&a])
                }
                RawDecision::Delete(b) => cost_model.del(&before_meta.node_info[&b]),
                RawDecision::Insert(a) => cost_model.ins(&after_meta.node_info[&a]),
            })
            .sum()
    }

    /// Differential check: the new APTED-engine-backed `compute_delta` must produce a mapping
    /// with the exact same total cost as the classic Zhang-Shasha oracle, for the given forests.
    fn assert_distance_matches_oracle(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
    ) {
        assert_distance_matches_oracle_pruned(
            before_meta,
            after_meta,
            before_root_ids,
            after_root_ids,
            &HashMap::new(),
            &HashMap::new(),
        );
    }

    fn assert_distance_matches_oracle_pruned(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
        before_node_map: &HashMap<usize, usize>,
        after_node_map: &HashMap<usize, usize>,
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };

        let before_idx = PostorderIndexer::build(before_meta, before_root_ids, before_node_map);
        let after_idx = PostorderIndexer::build(after_meta, after_root_ids, after_node_map);

        // Built from the *same* pruning maps `before_idx`/`after_idx` were pruned against, so
        // both engines below see the identical containment constraints - a differential check
        // that only exercises `adjust()` if it's built from a real `ASTDiff`, not `None`.
        let diff = ASTDiff {
            before_node_map: before_node_map.clone(),
            after_node_map: after_node_map.clone(),
            ..Default::default()
        };
        let containment = ContainmentCtx::build(
            before_root_ids,
            after_root_ids,
            before_meta,
            after_meta,
            &diff,
        );

        let mut oracle_delta = compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            Some(&containment),
        );
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            Some(&containment),
            &mut oracle_delta,
        );
        let oracle_cost =
            mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            before_root_ids,
            after_root_ids,
            before_node_map,
            after_node_map,
            Some(&containment),
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            Some(&containment),
            &mut new_delta,
        );
        let new_cost = mapping_total_cost(&new_decisions, before_meta, after_meta, &cost_model);

        assert_eq!(
            new_cost, oracle_cost,
            "new engine cost {new_cost} != oracle cost {oracle_cost}\nbefore_roots={before_root_ids:?} after_roots={after_root_ids:?}"
        );
    }

    /// Same as `assert_distance_matches_oracle_pruned`, but pins the forced-RIGHT driver
    /// instead of the live (forced-left) engine - validates `spf_r`/`compute_right_keyroots`/
    /// `apted_tree_edit_dist_r` in isolation, the same way the forced-left tests above pin `spf_l`.
    fn assert_distance_matches_oracle_forced_right(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        let before_idx = PostorderIndexer::build(before_meta, before_root_ids, &empty_map);
        let after_idx = PostorderIndexer::build(after_meta, after_root_ids, &empty_map);

        let mut oracle_delta = compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
        None,
        );
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        let oracle_cost =
            mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

        let mut new_delta = compute_delta_forced_right(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            before_root_ids,
            after_root_ids,
            &empty_map,
            &empty_map,
            None,
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut new_delta,
        );
        let new_cost = mapping_total_cost(&new_decisions, before_meta, after_meta, &cost_model);

        assert_eq!(
            new_cost, oracle_cost,
            "forced-right cost {new_cost} != oracle cost {oracle_cost}\nbefore_roots={before_root_ids:?} after_roots={after_root_ids:?}"
        );
    }

    #[test]
    fn test_apted_engine_forced_right_matches_oracle_fuzz() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(7));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle_forced_right(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                panic!(
                    "forced-right fuzz failure at seed {seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
                );
            }
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_single_leaf() {
        let before = synthetic_meta(&[(0, "leaf", "a", &[])]);
        let after = synthetic_meta(&[(0, "leaf", "b", &[])]);
        assert_distance_matches_oracle(&before, &after, &[0], &[0]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_small_trees() {
        // before: root(a, b)   after: root(a, b, c)
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 2]),
            (1, "leaf", "a", &[]),
            (2, "leaf", "b", &[]),
        ]);
        let after = synthetic_meta(&[
            (10, "root", "", &[11, 12, 13]),
            (11, "leaf", "a", &[]),
            (12, "leaf", "b", &[]),
            (13, "leaf", "c", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[10]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_multi_root_forest() {
        // before forest: leaf(a), leaf(b), inner(c -> leaf(d))
        let before = synthetic_meta(&[
            (0, "leaf", "a", &[]),
            (1, "leaf", "b", &[]),
            (2, "inner", "", &[3]),
            (3, "leaf", "d", &[]),
        ]);
        // after forest: leaf(a), inner(c -> leaf(d), leaf(e)), leaf(z)
        let after = synthetic_meta(&[
            (10, "leaf", "a", &[]),
            (11, "inner", "", &[12, 13]),
            (12, "leaf", "d", &[]),
            (13, "leaf", "e", &[]),
            (14, "leaf", "z", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0, 1, 2], &[10, 11, 14]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_deep_unbalanced() {
        // before: a deep left chain with a branchy right side.
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 6]),
            (1, "chain", "", &[2]),
            (2, "chain", "", &[3]),
            (3, "chain", "", &[4]),
            (4, "leaf", "x", &[]),
            (6, "branch", "", &[7, 8, 9]),
            (7, "leaf", "p", &[]),
            (8, "leaf", "q", &[]),
            (9, "leaf", "r", &[]),
        ]);
        let after = synthetic_meta(&[
            (100, "root", "", &[101, 106]),
            (101, "chain", "", &[102]),
            (102, "chain", "", &[104]),
            (104, "leaf", "x", &[]),
            (106, "branch", "", &[107, 109, 108]),
            (107, "leaf", "p", &[]),
            (108, "leaf", "q", &[]),
            (109, "leaf", "s", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[100]);
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn range(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn gen_random_tree(
        rng: &mut Rng,
        next_id: &mut usize,
        depth: usize,
        max_depth: usize,
        kinds: &[&str],
        texts: &[&str],
        nodes: &mut Vec<(usize, String, String, Vec<usize>)>,
    ) -> usize {
        let id = *next_id;
        *next_id += 1;
        let kind = kinds[rng.range(kinds.len())];
        let is_leaf = depth >= max_depth || rng.range(3) == 0;
        if is_leaf {
            let text = texts[rng.range(texts.len())];
            nodes.push((id, kind.to_string(), text.to_string(), Vec::new()));
        } else {
            let nchildren = 1 + rng.range(3);
            let mut child_ids = Vec::new();
            for _ in 0..nchildren {
                child_ids.push(gen_random_tree(
                    rng,
                    next_id,
                    depth + 1,
                    max_depth,
                    kinds,
                    texts,
                    nodes,
                ));
            }
            nodes.push((id, kind.to_string(), String::new(), child_ids));
        }
        id
    }

    fn meta_from_owned(nodes: &[(usize, String, String, Vec<usize>)]) -> ASTMetadata {
        let mut node_info = HashMap::new();
        for (id, kind, text, children) in nodes {
            node_info.insert(
                *id,
                ASTNodeMetadata {
                    kind: kind.clone(),
                    text: text.clone(),
                    children: children.clone(),
                    start_byte: *id,
                    preorder_index: *id,
                },
            );
        }
        let node_to_parent = node_to_parent_from(&node_info);
        ASTMetadata {
            node_info,
            node_to_parent,
            ..Default::default()
        }
    }

    /// Perfectly balanced binary tree: `2^depth - 1` nodes, the adversarial shape for an
    /// L/R-only (no spfA/INNER) decomposition strategy per the APTED papers.
    fn gen_balanced_binary_tree(
        next_id: &mut usize,
        depth: usize,
        kinds: &[&str],
        texts: &[&str],
        nodes: &mut Vec<(usize, String, String, Vec<usize>)>,
    ) -> usize {
        let id = *next_id;
        *next_id += 1;
        let kind = kinds[id % kinds.len()];
        if depth == 0 {
            let text = texts[id % texts.len()];
            nodes.push((id, kind.to_string(), text.to_string(), Vec::new()));
        } else {
            let left = gen_balanced_binary_tree(next_id, depth - 1, kinds, texts, nodes);
            let right = gen_balanced_binary_tree(next_id, depth - 1, kinds, texts, nodes);
            nodes.push((id, kind.to_string(), String::new(), vec![left, right]));
        }
        id
    }

    #[test]
    #[ignore = "ad hoc timing measurement, not a correctness check"]
    fn bench_compute_delta_large_balanced_trees() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        for depth in [9usize, 10, 11, 12, 14] {
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root =
                gen_balanced_binary_tree(&mut next_id, depth, &kinds, &texts, &mut before_nodes);
            let mut after_nodes = Vec::new();
            // A different balanced tree of the same shape/size, so nothing trivially matches by
            // id and the engine has to do real work, like an unmatched-residual diff would.
            let after_root =
                gen_balanced_binary_tree(&mut next_id, depth, &kinds, &texts, &mut after_nodes);

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let before_idx = PostorderIndexer::build(&before_meta, &[before_root], &empty_map);
            let after_idx = PostorderIndexer::build(&after_meta, &[after_root], &empty_map);
            let n = before_nodes.len();

            let t0 = std::time::Instant::now();
            let mut new_delta = compute_delta(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                &[before_root],
                &[after_root],
                &empty_map,
                &empty_map,
                None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut new_delta,
            );
            let new_elapsed = t0.elapsed();

            let t0 = std::time::Instant::now();
            let mut oracle_delta = compute_delta_zhang_shasha(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
            None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut oracle_delta,
            );
            let oracle_elapsed = t0.elapsed();

            eprintln!(
                "depth={depth} n={n}: new_engine={new_elapsed:?} zhang_shasha_oracle={oracle_elapsed:?}"
            );
        }
    }

    #[test]
    #[ignore = "ad hoc timing measurement, not a correctness check"]
    fn bench_compute_delta_typical_random_trees() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        for depth in [8usize, 9, 10] {
            let mut rng = Rng(depth as u64 * 7919 + 1);
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                depth,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                depth,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let before_idx = PostorderIndexer::build(&before_meta, &[before_root], &empty_map);
            let after_idx = PostorderIndexer::build(&after_meta, &[after_root], &empty_map);
            let n = before_nodes.len() + after_nodes.len();

            let t0 = std::time::Instant::now();
            let mut new_delta = compute_delta(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                &[before_root],
                &[after_root],
                &empty_map,
                &empty_map,
                None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut new_delta,
            );
            let new_elapsed = t0.elapsed();

            let t0 = std::time::Instant::now();
            let mut oracle_delta = compute_delta_zhang_shasha(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
            None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut oracle_delta,
            );
            let oracle_elapsed = t0.elapsed();

            eprintln!(
                "depth={depth} n={n}: new_engine={new_elapsed:?} zhang_shasha_oracle={oracle_elapsed:?}"
            );
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_fuzz_minimal_repro() {
        let before = meta_from_owned(&[
            (2, "a".into(), "x".into(), vec![]),
            (5, "a".into(), "z".into(), vec![]),
            (4, "c".into(), "".into(), vec![5]),
            (6, "a".into(), "x".into(), vec![]),
            (3, "c".into(), "".into(), vec![4, 6]),
            (1, "c".into(), "".into(), vec![2, 3]),
            (0, "a".into(), "".into(), vec![1]),
        ]);
        let after = meta_from_owned(&[
            (9, "b".into(), "z".into(), vec![]),
            (8, "b".into(), "".into(), vec![9]),
            (11, "b".into(), "z".into(), vec![]),
            (12, "a".into(), "x".into(), vec![]),
            (10, "a".into(), "".into(), vec![11, 12]),
            (14, "b".into(), "y".into(), vec![]),
            (13, "b".into(), "".into(), vec![14]),
            (7, "b".into(), "".into(), vec![8, 10, 13]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[7]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_tiny_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (4, "c".into(), "x".into(), vec![]),
            (3, "b".into(), "".into(), vec![4]),
            (5, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3, 5]),
            (0, "c".into(), "".into(), vec![1, 2]),
        ]);
        let after = meta_from_owned(&[(6, "c".into(), "y".into(), vec![])]);
        assert_distance_matches_oracle(&before, &after, &[0], &[6]);
    }

    fn debug_dump_case(
        before: &ASTMetadata,
        after: &ASTMetadata,
        before_root: usize,
        after_root: usize,
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();
        let before_idx = PostorderIndexer::build(before, &[before_root], &empty_map);
        let after_idx = PostorderIndexer::build(after, &[after_root], &empty_map);

        let mut oracle_delta =
            compute_delta_zhang_shasha(&before_idx, &after_idx, before, after, &cost_model, None);
        // Snapshot *before* compute_edit_mapping, which mutates delta in place as it recomputes
        // forest_dist - comparing post-mutation tables would compare the wrong thing.
        let oracle_snapshot: Vec<Vec<u64>> = (0..before_idx.size)
            .map(|b| {
                (0..after_idx.size)
                    .map(|a| oracle_delta.get(b, a))
                    .collect()
            })
            .collect();
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        eprintln!("ORACLE decisions: {oracle_decisions:?}");

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            &[before_root],
            &[after_root],
            &empty_map,
            &empty_map,
            None,
        );
        let new_snapshot: Vec<Vec<u64>> = (0..before_idx.size)
            .map(|b| (0..after_idx.size).map(|a| new_delta.get(b, a)).collect())
            .collect();
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut new_delta,
        );
        eprintln!("NEW decisions: {new_decisions:?}");

        for b in 0..before_idx.size {
            for a in 0..after_idx.size {
                let bn = before_idx.node_id_at(before_idx.pre_to_post[b] + 1);
                let an = after_idx.node_id_at(after_idx.pre_to_post[a] + 1);
                let ov = oracle_snapshot[b][a];
                let nv = new_snapshot[b][a];
                eprintln!("delta[{b}(id={bn})][{a}(id={an})]: oracle={ov} new={nv}");
            }
        }

        // Recompute the top-level forest_dist table fresh from each snapshot, to find exactly
        // where the two diverge (since compute_edit_mapping mutates delta as it runs).
        let rebuild = |snap: &[Vec<u64>]| {
            let mut d = DeltaTable::new(before_idx.size.max(1), after_idx.size.max(1));
            for (b, row) in snap.iter().enumerate() {
                for (a, &v) in row.iter().enumerate() {
                    if v != 0 {
                        d.set(b, a, v);
                    }
                }
            }
            d
        };
        let mut oracle_d2 = rebuild(&oracle_snapshot);
        let mut new_d2 = rebuild(&new_snapshot);
        let mut oracle_fd = ForestDist::new(before_idx.size + 1, after_idx.size + 1, 0);
        let mut new_fd = ForestDist::new(before_idx.size + 1, after_idx.size + 1, 0);
        forest_dist(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut oracle_d2,
            before_idx.size,
            after_idx.size,
            &mut oracle_fd,
            false,
        );
        forest_dist(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut new_d2,
            before_idx.size,
            after_idx.size,
            &mut new_fd,
            false,
        );
        for di in 0..=before_idx.size {
            for dj in 0..=after_idx.size {
                let ov = oracle_fd[(di, dj)];
                let nv = new_fd[(di, dj)];
                eprintln!(
                    "forestdist[{di}][{dj}]: oracle={ov} new={nv}{}",
                    if ov != nv { " <<<" } else { "" }
                );
            }
        }
    }

    #[test]
    fn debug_dump_tiny_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (4, "c".into(), "x".into(), vec![]),
            (3, "b".into(), "".into(), vec![4]),
            (5, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3, 5]),
            (0, "c".into(), "".into(), vec![1, 2]),
        ]);
        let after = meta_from_owned(&[(6, "c".into(), "y".into(), vec![])]);
        debug_dump_case(&before, &after, 0, 6);
    }

    #[test]
    fn debug_dump_n7_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (3, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3]),
            (4, "a".into(), "y".into(), vec![]),
            (0, "a".into(), "".into(), vec![1, 2, 4]),
        ]);
        let after = meta_from_owned(&[
            (6, "c".into(), "x".into(), vec![]),
            (5, "a".into(), "".into(), vec![6]),
        ]);
        debug_dump_case(&before, &after, 0, 5);
    }

    #[test]
    fn debug_dump_minimal_repro() {
        let before = meta_from_owned(&[
            (2, "a".into(), "x".into(), vec![]),
            (5, "a".into(), "z".into(), vec![]),
            (4, "c".into(), "".into(), vec![5]),
            (6, "a".into(), "x".into(), vec![]),
            (3, "c".into(), "".into(), vec![4, 6]),
            (1, "c".into(), "".into(), vec![2, 3]),
            (0, "a".into(), "".into(), vec![1]),
        ]);
        let after = meta_from_owned(&[
            (9, "b".into(), "z".into(), vec![]),
            (8, "b".into(), "".into(), vec![9]),
            (11, "b".into(), "z".into(), vec![]),
            (12, "a".into(), "x".into(), vec![]),
            (10, "a".into(), "".into(), vec![11, 12]),
            (14, "b".into(), "y".into(), vec![]),
            (13, "b".into(), "".into(), vec![14]),
            (7, "b".into(), "".into(), vec![8, 10, 13]),
        ]);
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();
        let before_idx = PostorderIndexer::build(&before, &[0], &empty_map);
        let after_idx = PostorderIndexer::build(&after, &[7], &empty_map);

        let mut oracle_delta =
            compute_delta_zhang_shasha(&before_idx, &after_idx, &before, &after, &cost_model, None);
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        eprintln!("ORACLE decisions: {oracle_decisions:?}");

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            &[0],
            &[7],
            &empty_map,
            &empty_map,
            None,
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            None,
            &mut new_delta,
        );
        eprintln!("NEW decisions: {new_decisions:?}");

        for b in 0..before_idx.size {
            for a in 0..after_idx.size {
                let ov = oracle_delta.get(b, a);
                let nv = new_delta.get(b, a);
                if ov != nv {
                    let bn = before_idx.node_id_at(before_idx.pre_to_post[b] + 1);
                    let an = after_idx.node_id_at(after_idx.pre_to_post[a] + 1);
                    eprintln!(
                        "delta mismatch: before_pre={b}(id={bn}) after_pre={a}(id={an}) oracle={ov} new={nv}"
                    );
                }
            }
        }

        // Dump the strategy table's choices (virtual-space, vroot included) to see which
        // (v, w) pairs picked INNER (the new, previously-unexercised path).
        let mut bidx = AptedIndexer::build(&before, &[0], &empty_map);
        let mut aidx = AptedIndexer::build(&after, &[7], &empty_map);
        bidx.fill_subtree_costs(&before, &cost_model);
        aidx.fill_subtree_costs(&after, &cost_model);
        let strategy = compute_opt_strategy_post_l(&bidx, &aidx, false);
        let path_id_offset = bidx.size as i64;
        for v in 0..bidx.size {
            for w in 0..aidx.size {
                if bidx.sizes[v] <= 1 || aidx.sizes[w] <= 1 {
                    continue;
                }
                let sp = strategy.get(v, w);
                let node = sp.abs() - 1;
                let is_t1 = node < path_id_offset;
                let (idx, root, sz) = if is_t1 {
                    (&bidx, v, bidx.sizes[v])
                } else {
                    (&aidx, w, aidx.sizes[w])
                };
                let local_node = if is_t1 { node } else { node - path_id_offset };
                let ty = get_strategy_path_type(sp, path_id_offset, root, sz);
                if ty == 2 {
                    eprintln!(
                        "INNER: v={v} w={w} sp={sp} is_t1={is_t1} local_node={local_node} (idx size={})",
                        idx.size
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "shrinker for the spfA bug hunt, not a correctness check"]
    fn shrink_apted_engine_fuzz_failure() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let mut smallest: Option<(usize, u64, Vec<_>, Vec<_>)> = None;
        for seed in 0..20000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(1));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                3,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                3,
                &kinds,
                &texts,
                &mut after_nodes,
            );
            let n = before_nodes.len() + after_nodes.len();
            if let Some((best_n, ..)) = &smallest
                && n >= *best_n
            {
                continue;
            }

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                smallest = Some((n, seed, before_nodes, after_nodes));
            }
        }
        match smallest {
            Some((n, seed, before_nodes, after_nodes)) => panic!(
                "smallest failure: n={n} seed={seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
            ),
            None => eprintln!("no failures found"),
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_fuzz() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(1));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                panic!(
                    "fuzz failure at seed {seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
                );
            }
        }
    }

    /// Fisher-Yates shuffle, used by `gen_random_pruning` to pick a random subset of leaves on
    /// each side without repeats.
    fn shuffle(rng: &mut Rng, v: &mut Vec<usize>) {
        for i in (1..v.len()).rev() {
            let j = rng.range(i + 1);
            v.swap(i, j);
        }
    }

    /// Builds a genuine `(before_node_map, after_node_map)` pruning constraint out of a random
    /// pair of trees: picks 1-3 random *leaf* nodes on each side (excluding the roots, so the
    /// forest itself never goes empty) and cross-matches them 1:1, exactly the shape
    /// `resolve_forest` leaves behind for `ContainmentCtx` when an earlier pass has already
    /// matched some descendants elsewhere. Leaves only (not arbitrary subtrees) keeps this
    /// simple: the same restriction `test_apted_engine_matches_oracle_with_pruned_descendants`
    /// uses by hand. Combined with the `["a","b","c"]`/`["x","y","z"]` generator (same-kind
    /// candidates are always cheaply renameable under the unit cost model, so containment is the
    /// *only* thing that can rule one out), this is what gives the fuzz test below teeth: without
    /// `adjust()` applied at every `vren` site, the Apted engine is free to rename a pruned
    /// ancestor onto a same-kind sibling subtree that doesn't actually contain where its
    /// descendant landed, which the Zhang-Shasha oracle (already containment-aware via
    /// `forest_dist`) will never do - producing a real cost divergence, not just a coincidental
    /// match.
    fn gen_random_pruning(
        rng: &mut Rng,
        before_nodes: &[(usize, String, String, Vec<usize>)],
        after_nodes: &[(usize, String, String, Vec<usize>)],
        before_root: usize,
        after_root: usize,
    ) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
        let mut before_leaves: Vec<usize> = before_nodes
            .iter()
            .filter(|(id, _, _, children)| *id != before_root && children.is_empty())
            .map(|(id, ..)| *id)
            .collect();
        let mut after_leaves: Vec<usize> = after_nodes
            .iter()
            .filter(|(id, _, _, children)| *id != after_root && children.is_empty())
            .map(|(id, ..)| *id)
            .collect();

        shuffle(rng, &mut before_leaves);
        shuffle(rng, &mut after_leaves);

        let max_k = before_leaves.len().min(after_leaves.len());
        if max_k == 0 {
            return (HashMap::new(), HashMap::new());
        }
        let k = 1 + rng.range(max_k.min(3));
        let mut before_map = HashMap::new();
        let mut after_map = HashMap::new();
        for i in 0..k {
            before_map.insert(before_leaves[i], after_leaves[i]);
            after_map.insert(after_leaves[i], before_leaves[i]);
        }
        (before_map, after_map)
    }

    /// Extends `test_apted_engine_matches_oracle_fuzz` to the case that fuzz never exercises: a
    /// forest with real pruned-descendant constraints (see `gen_random_pruning`). Both engines go
    /// through `assert_distance_matches_oracle_pruned`, which now builds one shared
    /// `ContainmentCtx` and threads it into *both* `compute_delta_zhang_shasha` (oracle) and
    /// `compute_delta` (Apted) - a divergence here means `compute_delta`'s `vren` call sites are
    /// missing an `adjust()` application somewhere, not just that pruning crashes something.
    #[test]
    fn test_apted_engine_matches_oracle_fuzz_with_containment() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(23));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let (before_node_map, after_node_map) = gen_random_pruning(
                &mut rng,
                &before_nodes,
                &after_nodes,
                before_root,
                after_root,
            );

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle_pruned(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                    &before_node_map,
                    &after_node_map,
                );
            });
            if result.is_err() {
                panic!(
                    "containment fuzz failure at seed {seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}\nbefore_node_map={before_node_map:?}\nafter_node_map={after_node_map:?}"
                );
            }
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_with_pruned_descendants() {
        // root(a, b, c, d) vs root(a, x, c, y) - but `b`/`d` (before) and `x`/`y` (after) are
        // already matched elsewhere, so only `root`+`a`+`c` survive pruning into a forest of
        // multiple unmatched roots per side (since the pruned nodes break contiguity).
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 2, 3, 4]),
            (1, "leaf", "a", &[]),
            (2, "leaf", "b", &[]),
            (3, "leaf", "c", &[]),
            (4, "leaf", "d", &[]),
        ]);
        let after = synthetic_meta(&[
            (10, "root", "", &[11, 12, 13, 14]),
            (11, "leaf", "a", &[]),
            (12, "leaf", "x", &[]),
            (13, "leaf", "c", &[]),
            (14, "leaf", "y", &[]),
        ]);
        let before_map: HashMap<usize, usize> = [(2, 12), (4, 14)].into_iter().collect();
        let after_map: HashMap<usize, usize> = [(12, 2), (14, 4)].into_iter().collect();
        assert_distance_matches_oracle_pruned(
            &before,
            &after,
            &[0],
            &[10],
            &before_map,
            &after_map,
        );
    }

    #[test]
    fn test_already_matched_nodes_are_skipped() -> Result<()> {
        // This test verifies that APTED properly skips nodes
        // that are already matched in the diff.
        //
        // Strategy: Use a code pair where nodes change, pre-populate the diff with
        // a mapping that matches a node to a DIFFERENT node than what APTED would
        // naturally choose, then verify that APTED doesn't create a second mapping
        // for the same node.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-leetcode-1-bugfix").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Get some child nodes to create an artificial mapping
        let mut before_cursor = before_root.walk();
        let before_children: Vec<_> = before_root.children(&mut before_cursor).collect();

        let mut after_cursor = after_root.walk();
        let after_children: Vec<_> = after_root.children(&mut after_cursor).collect();

        // If we have at least 2 children in both trees, create a cross-mapping
        // that APTED would not naturally choose
        if before_children.len() >= 2 && after_children.len() >= 2 {
            let before_node_1 = before_children[0];
            let before_node_2 = before_children[1];
            let after_node_1 = after_children[0];
            let after_node_2 = after_children[1];

            // Create a mapping that swaps the natural order
            // Map before_node_1 to after_node_2 (wrong partner)
            // and before_node_2 to after_node_1 (wrong partner)
            // This forces APTED to potentially create additional correct mappings
            let wrong_mapping_1 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_1.id(), after_node_2.id(), wrong_mapping_1);

            let wrong_mapping_2 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_2.id(), after_node_1.id(), wrong_mapping_2);
        }

        // Now call APTED with the diff that already has these artificial mappings
        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        // Check if any before node appears in multiple mappings
        let mut before_node_counts = std::collections::HashMap::new();
        for (before_id, _) in diff.mapping.keys() {
            *before_node_counts.entry(*before_id).or_insert(0) += 1;
        }

        // Check if any after node appears in multiple mappings
        let mut after_node_counts = std::collections::HashMap::new();
        for (_, after_id) in diff.mapping.keys() {
            *after_node_counts.entry(*after_id).or_insert(0) += 1;
        }

        // Find nodes that are mapped multiple times
        let before_nodes_with_multiple_mappings: Vec<_> = before_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        let after_nodes_with_multiple_mappings: Vec<_> = after_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        // Assert that no nodes are mapped multiple times
        assert!(
            before_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found before nodes with multiple mappings: {:?}",
            before_nodes_with_multiple_mappings
        );
        assert!(
            after_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found after nodes with multiple mappings: {:?}",
            after_nodes_with_multiple_mappings
        );

        Ok(())
    }

    #[test]
    fn test_honors_pre_existing_match_and_still_finds_nested_reuse() -> Result<()> {
        // Combines two things apted must get right at once: honoring a match that some earlier
        // pass already made (here, faked by hand, same technique as
        // test_already_matched_nodes_are_skipped), and still discovering the nested-reuse
        // match (the print(...) call moved one level deeper) for everything else.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("python-added-if-block-small")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Pre-map the `numer = 12` assignment statement by hand, as if an earlier pass had
        // already matched it.
        let assignment_path = vec!["if_statement", "block", "expression_statement:1"];
        let before_assignment = helper::node_for_path(before_root, &assignment_path)?;
        let after_assignment = helper::node_for_path(after_root, &assignment_path)?;
        diff.add_mapping(
            before_assignment.id(),
            after_assignment.id(),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            },
        );

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        // The pre-existing match must survive untouched.
        assert_eq!(
            diff.mapping
                .get(&(before_assignment.id(), after_assignment.id()))
                .map(|m| &m.reason),
            Some(&ASTMappingReason::OptimalIDU)
        );

        // The print(...) call should still be found and reused one level deeper inside the new
        // if-block, despite the unrelated pre-existing match elsewhere in the same forest.
        let print_call_before = helper::node_for_path(
            before_root,
            &["if_statement", "block", "expression_statement:2"],
        )?;
        assert!(
            diff.before_node_map
                .get(&print_call_before.id())
                .is_none_or(|&id| id != 0),
            "the reused print(...) call should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Same total as test_python_added_if_block_small - the pre-existing match was for a
        // node that would have cost 0 anyway, so honoring it changes nothing about the total.
        assert_eq!(mapping.cost, 8);

        Ok(())
    }

    #[test]
    fn test_no_change() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-no-change").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let mapping = diff
            .mapping
            .get(&(before_ast.root_node().id(), after_ast.root_node().id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);
        assert_eq!(mapping.cost, 0);

        Ok(())
    }

    #[test]
    fn test_hello_world_added_message() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("rust-hello-world-added-message")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let path = vec!["function_item", "block", "expression_statement:2"];

        assert!(
            helper::was_tree_added(&path, after_root, &diff)?,
            "The inserted line is not correctly marked as Insert"
        );

        let added_node = helper::node_for_path(after_root, &path)?;
        let mapping = diff.mapping.get(&(0, added_node.id())).unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Insert);
        // 12 nodes in total are added. expression_statement + 11 more.
        assert_eq!(mapping.cost, 12);

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The cost should correctly transfer upwards to the root node.
        assert_eq!(mapping.cost, 12);

        Ok(())
    }

    #[test]
    fn test_hello_world_removed_message() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("rust-hello-world-removed-message")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let path = vec!["function_item", "block", "expression_statement:2"];

        assert!(
            helper::was_tree_deleted(&path, before_root, &diff)?,
            "The removed line is not correctly marked as Delete"
        );

        let deleted_node = helper::node_for_path(before_root, &path)?;
        let mapping = diff.mapping.get(&(deleted_node.id(), 0)).unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Delete);
        // 12 nodes in total are removed. expression_statement + 11 more.
        assert_eq!(mapping.cost, 12);

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The cost should correctly transfer upwards to the root node.
        assert_eq!(mapping.cost, 12);

        Ok(())
    }

    #[test]
    fn test_python_added_if_block_small() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("python-added-if-block-small")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The best solution is to simply insert the 8 if_expression nodes in the tree.
        assert_eq!(mapping.cost, 8);

        Ok(())
    }

    #[test]
    fn test_python_added_if_block() -> Result<()> {
        // Larger, more realistic version of test_python_added_if_block_small: a function
        // definition precedes the if-block, and the wrapped statement is an f-string print
        // call. Pins that the nested-reuse fix generalizes beyond the minimal repro.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("python-added-if-block").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // The print(...) call should be reused (not deleted+reinserted) even though it's now
        // nested one level deeper inside the new `if result != [0, 1]:` wrapper.
        let print_call_before = helper::node_for_path(
            before_root,
            &["if_statement", "block", "expression_statement:4"],
        )?;
        assert!(
            diff.before_node_map
                .get(&print_call_before.id())
                .is_none_or(|&id| id != 0),
            "the reused print(...) call should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Only the new `if result != [0, 1]:` wrapper is genuinely new (if_statement, if,
        // comparison_operator, identifier, !=, list, [, integer, ",", integer, ], :, block =
        // 13 nodes); the print(...) call itself is fully reused at zero cost, not
        // deleted-and-reinserted.
        assert_eq!(mapping.cost, 13);

        Ok(())
    }

    #[test]
    fn test_rust_add_if() -> Result<()> {
        // Same wrap-in-a-new-if pattern as test_python_added_if_block*, but for Rust's grammar
        // and with the existing if/else demoted to an `else if` branch (nested one level
        // deeper as the new if's else_clause) instead of nested inside a block - guards
        // against tree-sitter-shape-specific assumptions in the fix.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-add-if").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // The entire original `if number % 2 == 0 { ... } else { ... }` should be reused intact
        // as the new `else if`'s content, not deleted and rebuilt.
        let original_if = helper::node_for_path(
            before_root,
            &[
                "function_item",
                "block",
                "expression_statement",
                "if_expression",
            ],
        )?;
        assert!(
            diff.before_node_map
                .get(&original_if.id())
                .is_none_or(|&id| id != 0),
            "the reused if/else should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Only the new outer `if number == 0 { println!("Zero"); } else if ...` wrapper plus
        // its own new println!("Zero") body is genuinely new; the entire original if/else is
        // reused intact (at zero cost) as the new else-if's content.
        assert_eq!(mapping.cost, 23);

        Ok(())
    }

    #[test]
    fn flat_tree_myers_diff_matches_changed_tokens() -> Result<()> {
        // Two token_tree-like flat structures: 100 identical tokens plus one changed value.
        // Myers should match 100 identical tokens and mark 2 as delete/insert.
        let before_tokens: Vec<&str> = (0..50)
            .map(|_| "tok")
            .chain(std::iter::once("old_value"))
            .chain((0..50).map(|_| "tok"))
            .collect();
        let after_tokens: Vec<&str> = (0..50)
            .map(|_| "tok")
            .chain(std::iter::once("new_value"))
            .chain((0..50).map(|_| "tok"))
            .collect();

        // Build synthetic metadata where the root has 101 leaf children.
        let mut before_meta = ASTMetadata::default();
        let mut after_meta = ASTMetadata::default();

        fn build_flat(tokens: &[&str], meta: &mut ASTMetadata) -> (usize, Vec<usize>) {
            let root_id = 9000;
            let child_ids: Vec<usize> = (0..tokens.len()).map(|i| i + 1).collect();
            for (i, &tok) in tokens.iter().enumerate() {
                let id = i + 1;
                meta.node_info.insert(
                    id,
                    ASTNodeMetadata {
                        kind: "token".to_string(),
                        text: tok.to_string(),
                        children: vec![],
                        start_byte: id,
                        preorder_index: id,
                    },
                );
                // Use the token text as hash so identical tokens match.
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tok.hash(&mut h);
                meta.node_to_full_hash.insert(id, h.finish());
            }
            meta.node_info.insert(
                root_id,
                ASTNodeMetadata {
                    kind: "token_tree".to_string(),
                    text: String::new(),
                    children: child_ids.clone(),
                    start_byte: root_id,
                    preorder_index: root_id,
                },
            );
            meta.node_to_full_hash.insert(root_id, 0); // different hashes → will not short-circuit
            (root_id, child_ids)
        }

        let (before_root, _) = build_flat(&before_tokens, &mut before_meta);
        let (after_root, _) = build_flat(&after_tokens, &mut after_meta);
        // Make root hashes differ so the identical fast-path is skipped.
        before_meta.node_to_full_hash.insert(before_root, 1);
        after_meta.node_to_full_hash.insert(after_root, 2);

        let mut diff = ASTDiff::default();
        for_nodes(
            &before_meta,
            &after_meta,
            vec![before_root],
            vec![after_root],
            Algorithm::ZhangShasha,
            "test",
            &mut diff,
        );

        // Root pair should be mapped via flat-tree path.
        let root_mapping = diff
            .mapping
            .get(&(before_root, after_root))
            .expect("root mapped");
        assert_eq!(root_mapping.reason, ASTMappingReason::FlatSequenceDiff);
        assert_eq!(
            root_mapping.operation,
            ASTMappingOperation::MatchButNotIdentical
        );

        // The 100 identical "tok" tokens should all be matched (Identical).
        let identical_count = diff
            .mapping
            .values()
            .filter(|m| m.operation == ASTMappingOperation::Identical)
            .count();
        assert_eq!(
            identical_count, 100,
            "all 100 identical tokens should be matched"
        );

        // "old_value" (before child 51) should be deleted; "new_value" (after child 51) inserted.
        assert!(
            diff.mapping.contains_key(&(51, 0)),
            "old_value token should be deleted"
        );
        assert!(
            diff.mapping.contains_key(&(0, 51)),
            "new_value token should be inserted"
        );

        Ok(())
    }

    #[test]
    fn myers_lcs_basic() {
        // [1,2,3] vs [1,4,3]: matches at positions (0,0) and (2,2).
        let a = [1u64, 2, 3];
        let b = [1u64, 4, 3];
        let matches = myers_lcs(&a, &b, 100).expect("should find solution");
        assert_eq!(matches, vec![(0, 0), (2, 2)]);
    }

    #[test]
    fn myers_lcs_identical() {
        let a = [1u64, 2, 3, 4, 5];
        let matches = myers_lcs(&a, &a, 0).expect("d=0 for identical");
        assert_eq!(matches, vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]);
    }

    #[test]
    fn myers_lcs_empty() {
        assert_eq!(myers_lcs(&[], &[1u64], 10), Some(vec![]));
        assert_eq!(myers_lcs(&[1u64], &[], 10), Some(vec![]));
        assert_eq!(myers_lcs(&[], &[], 10), Some(vec![]));
    }

    #[test]
    fn myers_lcs_exceeds_limit() {
        // a and b share no elements → d = n+m; with limit=3 it should return None.
        let a = [1u64, 2, 3];
        let b = [4u64, 5, 6];
        assert!(myers_lcs(&a, &b, 3).is_none());
    }
