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

use super::super::engine::*;
use super::*;
use crate::diff::{ASTMappingOperation, ASTMappingReason};
use crate::test::helper;
use anyhow::Result;

/// `ContainmentCtx::adjust` needs real parents: with none, every node looks parentless and it
/// forbids almost everything.
fn node_to_parent_from(
    node_info: &rustc_hash::FxHashMap<usize, ASTNodeMetadata>,
) -> rustc_hash::FxHashMap<usize, usize> {
    let mut parents = rustc_hash::FxHashMap::default();
    for (&id, info) in node_info {
        for &child in &info.children {
            parents.insert(child, id);
        }
    }
    parents
}

fn synthetic_meta(nodes: &[(usize, &str, &str, &[usize])]) -> ASTMetadata {
    let mut node_info = rustc_hash::FxHashMap::default();
    for &(id, kind, text, children) in nodes {
        node_info.insert(
            id,
            ASTNodeMetadata::new(
                kind.to_string(),
                text.to_string(),
                children.to_vec(),
                id,
                id,
            ),
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

/// Differential check: APTED's `compute_delta` must yield a mapping of exactly the Zhang-Shasha
/// oracle's total cost.
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
        &rustc_hash::FxHashMap::default(),
        &rustc_hash::FxHashMap::default(),
    );
}

fn assert_distance_matches_oracle_pruned(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_node_map: &rustc_hash::FxHashMap<usize, usize>,
    after_node_map: &rustc_hash::FxHashMap<usize, usize>,
) {
    let cost_model = UnitCostModel::new(Language::Unknown);

    let before_idx = PostorderIndexer::build(before_meta, before_root_ids, before_node_map);
    let after_idx = PostorderIndexer::build(after_meta, after_root_ids, after_node_map);

    // From the same pruning maps as the indexers, so both engines see the same constraints.
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
        "test_oracle_fuzz",
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
    let oracle_cost = mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

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

/// `assert_distance_matches_oracle_pruned` for the forced-RIGHT driver, so `PostDir::Right` is
/// checked on every case.
fn assert_distance_matches_oracle_forced_right(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
) {
    let cost_model = UnitCostModel::new(Language::Unknown);
    let empty_map = rustc_hash::FxHashMap::default();

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
    let oracle_cost = mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

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
    let mut node_info = rustc_hash::FxHashMap::default();
    for (id, kind, text, children) in nodes {
        node_info.insert(
            *id,
            ASTNodeMetadata::new(kind.clone(), text.clone(), children.clone(), *id, *id),
        );
    }
    let node_to_parent = node_to_parent_from(&node_info);
    ASTMetadata {
        node_info,
        node_to_parent,
        ..Default::default()
    }
}

/// Perfectly balanced binary tree of `2^depth - 1` nodes: the adversarial shape for an
/// L/R-only strategy, per the APTED papers.
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
    let cost_model = UnitCostModel::new(Language::Unknown);
    let empty_map = rustc_hash::FxHashMap::default();

    for depth in [9usize, 10, 11, 12, 14] {
        let mut before_nodes = Vec::new();
        let mut next_id = 0usize;
        let before_root =
            gen_balanced_binary_tree(&mut next_id, depth, &kinds, &texts, &mut before_nodes);
        let mut after_nodes = Vec::new();
        // Different labels, so nothing matches trivially.
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
    let cost_model = UnitCostModel::new(Language::Unknown);
    let empty_map = rustc_hash::FxHashMap::default();

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
    let cost_model = UnitCostModel::new(Language::Unknown);
    let empty_map = rustc_hash::FxHashMap::default();
    let before_idx = PostorderIndexer::build(before, &[before_root], &empty_map);
    let after_idx = PostorderIndexer::build(after, &[after_root], &empty_map);

    let mut oracle_delta =
        compute_delta_zhang_shasha(&before_idx, &after_idx, before, after, &cost_model, None);
    // Before `compute_edit_mapping`, which mutates `delta`.
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

    // From the snapshots, to find where the two diverge.
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
#[ignore = "debug dump: prints, asserts nothing"]
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
#[ignore = "debug dump: prints, asserts nothing"]
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
#[ignore = "debug dump: prints, asserts nothing"]
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
    let cost_model = UnitCostModel::new(Language::Unknown);
    let empty_map = rustc_hash::FxHashMap::default();
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

    // Virtual-space strategy choices, to see which pairs picked INNER.
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

/// Fisher-Yates shuffle.
fn shuffle(rng: &mut Rng, v: &mut [usize]) {
    for i in (1..v.len()).rev() {
        let j = rng.range(i + 1);
        v.swap(i, j);
    }
}

/// A `(before_node_map, after_node_map)` that cross-matches 1-3 random non-root leaves per side,
/// the shape an earlier pass leaves for `ContainmentCtx`. With labels that are always cheaply
/// renameable, containment is the only thing ruling a pairing out, so an APTED `vren` site
/// missing `adjust()` shows as a real cost divergence from the oracle.
fn gen_random_pruning(
    rng: &mut Rng,
    before_nodes: &[(usize, String, String, Vec<usize>)],
    after_nodes: &[(usize, String, String, Vec<usize>)],
    before_root: usize,
    after_root: usize,
) -> (
    rustc_hash::FxHashMap<usize, usize>,
    rustc_hash::FxHashMap<usize, usize>,
) {
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
        return (
            rustc_hash::FxHashMap::default(),
            rustc_hash::FxHashMap::default(),
        );
    }
    let k = 1 + rng.range(max_k.min(3));
    let mut before_map = rustc_hash::FxHashMap::default();
    let mut after_map = rustc_hash::FxHashMap::default();
    for i in 0..k {
        before_map.insert(before_leaves[i], after_leaves[i]);
        after_map.insert(after_leaves[i], before_leaves[i]);
    }
    (before_map, after_map)
}

/// `test_apted_engine_matches_oracle_fuzz` with pruned-descendant constraints
/// (`gen_random_pruning`); a divergence means an APTED `vren` site misses `adjust()`.
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
    // root(a, b, c, d) vs root(a, x, c, y) with b, d, x, y already matched elsewhere.
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
    let before_map: rustc_hash::FxHashMap<usize, usize> = [(2, 12), (4, 14)].into_iter().collect();
    let after_map: rustc_hash::FxHashMap<usize, usize> = [(12, 2), (14, 4)].into_iter().collect();
    assert_distance_matches_oracle_pruned(&before, &after, &[0], &[10], &before_map, &after_map);
}

#[test]
fn test_already_matched_nodes_are_skipped() -> Result<()> {
    // Pre-map two nodes to partners APTED would not choose; APTED must not map them again.
    let (before, after) = &*helper::handmade_test_code_pair("rust-leetcode-1-bugfix")?;

    let mut diff = ASTDiff::default();

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mut before_cursor = before_root.walk();
    let before_children: Vec<_> = before_root.children(&mut before_cursor).collect();

    let mut after_cursor = after_root.walk();
    let after_children: Vec<_> = after_root.children(&mut after_cursor).collect();

    if before_children.len() >= 2 && after_children.len() >= 2 {
        let before_node_1 = before_children[0];
        let before_node_2 = before_children[1];
        let after_node_1 = after_children[0];
        let after_node_2 = after_children[1];

        let wrong_mapping_1 = ASTMapping::identical(ASTMappingReason::OptimalIDU);
        diff.add_mapping(before_node_1.id(), after_node_2.id(), wrong_mapping_1);

        let wrong_mapping_2 = ASTMapping::identical(ASTMappingReason::OptimalIDU);
        diff.add_mapping(before_node_2.id(), after_node_1.id(), wrong_mapping_2);
    }

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let mut before_node_counts = std::collections::HashMap::new();
    for (before_id, _) in diff.mapping.keys() {
        *before_node_counts.entry(*before_id).or_insert(0) += 1;
    }

    let mut after_node_counts = std::collections::HashMap::new();
    for (_, after_id) in diff.mapping.keys() {
        *after_node_counts.entry(*after_id).or_insert(0) += 1;
    }

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
    let (before, after) = &*helper::handmade_test_code_pair("python-added-if-block-small")?;

    let mut diff = ASTDiff::default();

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    // As if an earlier pass had matched `numer = 12`.
    let assignment_path = vec!["if_statement", "block", "expression_statement:1"];
    let before_assignment = helper::node_for_path(before_root, &assignment_path)?;
    let after_assignment = helper::node_for_path(after_root, &assignment_path)?;
    diff.add_mapping(
        before_assignment.id(),
        after_assignment.id(),
        ASTMapping::identical(ASTMappingReason::OptimalIDU),
    );

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    assert_eq!(
        diff.mapping
            .get(&(before_assignment.id(), after_assignment.id()))
            .map(|m| &m.reason),
        Some(&ASTMappingReason::OptimalIDU)
    );

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
    // The pre-matched node would have cost 0 anyway.
    assert_eq!(mapping.cost, 8);

    Ok(())
}

#[test]
fn test_no_change() -> Result<()> {
    let (before, after) = &*helper::handmade_test_code_pair("rust-no-change")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

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
    let (before, after) = &*helper::handmade_test_code_pair("rust-hello-world-added-message")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

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
    // expression_statement and its 11 descendants.
    assert_eq!(mapping.cost, 12);

    let mapping = diff
        .mapping
        .get(&(before_root.id(), after_root.id()))
        .unwrap();
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
    // The cost accumulates up to the root.
    assert_eq!(mapping.cost, 12);

    Ok(())
}

#[test]
fn test_hello_world_removed_message() -> Result<()> {
    let (before, after) = &*helper::handmade_test_code_pair("rust-hello-world-removed-message")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

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
    // expression_statement and its 11 descendants.
    assert_eq!(mapping.cost, 12);

    let mapping = diff
        .mapping
        .get(&(before_root.id(), after_root.id()))
        .unwrap();
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
    // The cost accumulates up to the root.
    assert_eq!(mapping.cost, 12);

    Ok(())
}

#[test]
fn test_python_added_if_block_small() -> Result<()> {
    let (before, after) = &*helper::handmade_test_code_pair("python-added-if-block-small")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mapping = diff
        .mapping
        .get(&(before_root.id(), after_root.id()))
        .unwrap();
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
    // Only the 8 new if_expression nodes are inserted.
    assert_eq!(mapping.cost, 8);

    Ok(())
}

#[test]
fn test_python_added_if_block() -> Result<()> {
    let (before, after) = &*helper::handmade_test_code_pair("python-added-if-block")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

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
    // Only the 13 nodes of the new `if result != [0, 1]:` wrapper are new.
    assert_eq!(mapping.cost, 13);

    Ok(())
}

#[test]
fn test_rust_add_if() -> Result<()> {
    // The old if/else becomes the new if's `else if` branch rather than sitting in a block.
    let (before, after) = &*helper::handmade_test_code_pair("rust-add-if")?;

    let mut diff = ASTDiff::default();

    for_roots(before, after, Algorithm::ZhangShasha, "test", &mut diff);

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

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
    // Only the new outer `if number == 0 { println!("Zero"); } else ...` is new.
    assert_eq!(mapping.cost, 23);

    Ok(())
}

#[test]
fn flat_tree_myers_diff_matches_changed_tokens() -> Result<()> {
    // 100 identical tokens plus one changed value.
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

    let mut before_meta = ASTMetadata::default();
    let mut after_meta = ASTMetadata::default();

    fn build_flat(tokens: &[&str], meta: &mut ASTMetadata) -> (usize, Vec<usize>) {
        let root_id = 9000;
        let child_ids: Vec<usize> = (0..tokens.len()).map(|i| i + 1).collect();
        for (i, &tok) in tokens.iter().enumerate() {
            let id = i + 1;
            meta.node_info.insert(
                id,
                ASTNodeMetadata::new("token".to_string(), tok.to_string(), vec![], id, id),
            );
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            tok.hash(&mut h);
            meta.node_to_full_hash.insert(id, h.finish());
        }
        meta.node_info.insert(
            root_id,
            ASTNodeMetadata::new(
                "token_tree".to_string(),
                String::new(),
                child_ids.clone(),
                root_id,
                root_id,
            ),
        );
        meta.node_to_full_hash.insert(root_id, 0); // different hashes → will not short-circuit
        (root_id, child_ids)
    }

    let (before_root, _) = build_flat(&before_tokens, &mut before_meta);
    let (after_root, _) = build_flat(&after_tokens, &mut after_meta);
    // Differing root hashes, so the identical shortcut is skipped.
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

    let root_mapping = diff
        .mapping
        .get(&(before_root, after_root))
        .expect("root mapped");
    assert_eq!(root_mapping.reason, ASTMappingReason::FlatSequenceDiff);
    assert_eq!(
        root_mapping.operation,
        ASTMappingOperation::MatchButNotIdentical
    );

    let identical_count = diff
        .mapping
        .values()
        .filter(|m| m.operation == ASTMappingOperation::Identical)
        .count();
    assert_eq!(
        identical_count, 100,
        "all 100 identical tokens should be matched"
    );

    // The lone Myers-unmatched pair goes through APTED, which relabels same-kind leaves.
    let changed_mapping = diff.mapping.get(&(51, 51)).expect(
        "changed token pair should be recursively resolved, not atomically deleted/inserted",
    );
    assert_eq!(changed_mapping.operation, ASTMappingOperation::Update);

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
    // Nothing shared, so d = n + m > 3.
    let a = [1u64, 2, 3];
    let b = [4u64, 5, 6];
    assert!(myers_lcs(&a, &b, 3).is_none());
}

fn leaf(id: usize, hash: u64, meta: &mut ASTMetadata) {
    meta.node_info.insert(
        id,
        ASTNodeMetadata::new("leaf".to_string(), String::new(), vec![], id, id),
    );
    meta.node_to_full_hash.insert(id, hash);
}

/// `leaf` with a real kind and text: `ren` compares those, not the hash, and sees any two `leaf`
/// nodes as identical.
fn leaf_with_kind(id: usize, hash: u64, kind: &str, text: &str, meta: &mut ASTMetadata) {
    meta.node_info.insert(
        id,
        ASTNodeMetadata::new(kind.to_string(), text.to_string(), vec![], id, id),
    );
    meta.node_to_full_hash.insert(id, hash);
}

fn interior(id: usize, children: Vec<usize>, meta: &mut ASTMetadata) {
    meta.node_info.insert(
        id,
        ASTNodeMetadata::new("interior".to_string(), String::new(), children, id, id),
    );
}

#[test]
fn maximal_unmatched_roots_stops_at_first_unmatched_node_each_branch() {
    // root(1, matched) -> A(2, matched) -> C(4, unmatched)
    //                   -> B(3, unmatched) -> D(5, unmatched)
    // Expected {4, 3}: descent continues through matched A, and stops at wholly unmatched B.
    let mut meta = ASTMetadata::default();
    interior(1, vec![2, 3], &mut meta);
    interior(2, vec![4], &mut meta);
    interior(3, vec![5], &mut meta);
    leaf(4, 40, &mut meta);
    leaf(5, 50, &mut meta);

    let mut node_map = rustc_hash::FxHashMap::default();
    node_map.insert(1, 100); // root matched
    node_map.insert(2, 200); // A matched

    let mut roots = maximal_unmatched_roots(1, &meta, &node_map);
    roots.sort_unstable();
    assert_eq!(
        roots,
        vec![3, 4],
        "B and C should be the maximal unmatched roots, not D"
    );
}

#[test]
fn resolve_residual_forest_via_myers_lcs_matches_identical_and_recurses_the_rest() {
    // One child pair shares a hash; the other is alone in the gap after it, so it recurses
    // through APTED, which relabels two same-kind leaves rather than replacing them.
    let mut before_meta = ASTMetadata::default();
    let mut after_meta = ASTMetadata::default();
    interior(1, vec![2, 3], &mut before_meta);
    leaf(2, 999, &mut before_meta); // shared with after's node 12
    leaf(3, 111, &mut before_meta); // same kind as 13, no shared hash

    interior(11, vec![12, 13], &mut after_meta);
    leaf(12, 999, &mut after_meta); // shared with before's node 2
    leaf(13, 222, &mut after_meta); // same kind as 3, no shared hash

    let mut diff = ASTDiff::default();
    diff.before_node_map.insert(1, 11); // roots pre-matched by an earlier phase
    diff.after_node_map.insert(11, 1);

    resolve_residual_forest_via_myers_lcs(
        &before_meta,
        &after_meta,
        1,
        11,
        "test_source",
        &mut diff,
    );

    let matched = diff
        .mapping
        .get(&(2, 12))
        .expect("identical-hash children should be matched");
    assert_eq!(matched.operation, ASTMappingOperation::Identical);
    assert_eq!(matched.reason, ASTMappingReason::APTED("test_source"));

    assert_eq!(
        diff.before_node_map.get(&3).copied(),
        Some(13),
        "the single leftover entry on each side of the gap should be matched to each other via \
         real APTED, not atomically deleted/inserted"
    );
}

#[test]
fn resolve_residual_forest_via_myers_lcs_does_not_relabel_across_different_kinds() {
    // As above, but the leftover leaves differ in kind, which `ren` prices above delete+insert.
    let mut before_meta = ASTMetadata::default();
    let mut after_meta = ASTMetadata::default();
    interior(1, vec![2, 3], &mut before_meta);
    leaf(2, 999, &mut before_meta); // shared with after's node 12
    leaf_with_kind(3, 111, "kind_a", "x", &mut before_meta);

    interior(11, vec![12, 13], &mut after_meta);
    leaf(12, 999, &mut after_meta); // shared with before's node 2
    leaf_with_kind(13, 222, "kind_b", "y", &mut after_meta);

    let mut diff = ASTDiff::default();
    diff.before_node_map.insert(1, 11);
    diff.after_node_map.insert(11, 1);

    resolve_residual_forest_via_myers_lcs(
        &before_meta,
        &after_meta,
        1,
        11,
        "test_source",
        &mut diff,
    );

    assert!(
        diff.mapping.contains_key(&(3, 0)),
        "different-kind leftover entries must still delete, never relabel across kinds"
    );
    assert!(
        diff.mapping.contains_key(&(0, 13)),
        "different-kind leftover entries must still insert, never relabel across kinds"
    );
}

#[test]
fn resolve_residual_forest_via_myers_lcs_replaces_everything_past_the_edit_cap() {
    // 600 unshared leaves a side: edit distance 1200 exceeds FALLBACK_MAX_EDIT.
    const N: usize = 600;
    let mut before_meta = ASTMetadata::default();
    let mut after_meta = ASTMetadata::default();

    let before_children: Vec<usize> = (0..N).map(|i| i + 1).collect();
    let after_children: Vec<usize> = (0..N).map(|i| N + i + 1).collect();
    interior(9999, before_children.clone(), &mut before_meta);
    interior(19999, after_children.clone(), &mut after_meta);
    for (i, &id) in before_children.iter().enumerate() {
        // Distinct kind and text per leaf, so `ren` cannot relabel them.
        leaf_with_kind(
            id,
            id as u64,
            &format!("before_kind_{i}"),
            &format!("before_text_{i}"),
            &mut before_meta,
        );
    }
    for (i, &id) in after_children.iter().enumerate() {
        leaf_with_kind(
            id,
            id as u64,
            &format!("after_kind_{i}"),
            &format!("after_text_{i}"),
            &mut after_meta,
        );
    }

    let mut diff = ASTDiff::default();
    diff.before_node_map.insert(9999, 19999);
    diff.after_node_map.insert(19999, 9999);

    resolve_residual_forest_via_myers_lcs(
        &before_meta,
        &after_meta,
        9999,
        19999,
        "test_source",
        &mut diff,
    );

    for &id in &before_children {
        assert!(
            diff.mapping.contains_key(&(id, 0)),
            "before leaf {id} should be deleted"
        );
    }
    for &id in &after_children {
        assert!(
            diff.mapping.contains_key(&(0, id)),
            "after leaf {id} should be inserted"
        );
    }
    let identical_count = diff
        .mapping
        .values()
        .filter(|m| m.operation == ASTMappingOperation::Identical)
        .count();
    assert_eq!(
        identical_count, 0,
        "over the edit cap, nothing should be partially aligned"
    );
}

/// `ren` must not price a content change at zero because the changed bytes sit between children
/// rather than in one. Tested on `ren` directly: the cost model is wrong without it whether or not
/// a fixture routes through this arm.
#[test]
fn ren_charges_for_text_a_node_owns_directly() {
    let internal_node = |owned_text_hash: u64| ASTNodeMetadata {
        owned_text_hash,
        ..ASTNodeMetadata::new("AttValue".to_string(), String::new(), vec![1, 2], 0, 0)
    };
    let cost_model = UnitCostModel::new(Language::XML);

    // `role="button"` vs `role="menu"`: same kind, same (quote) children, different value.
    let relabel = cost_model.ren(&internal_node(0xBEEF), &internal_node(0xF00D));
    assert!(
        relabel > 0,
        "a differing attribute value must not be free to relabel"
    );
    // ...but strictly cheaper than delete+insert; at a tie the DP reads "value changed" as
    // "replaced".
    assert!(
        relabel < COST_DELETE + COST_INSERT,
        "relabel {relabel} must beat delete+insert"
    );
    assert_eq!(
        cost_model.ren(&internal_node(0xBEEF), &internal_node(0xBEEF)),
        0
    );
    assert_eq!(cost_model.ren(&internal_node(0), &internal_node(0)), 0);
}

#[test]
fn ren_never_pairs_a_worded_comment_with_a_bare_marker() {
    let comment = |text: &str| ASTNodeMetadata::new("comment".into(), text.into(), vec![], 0, 0);
    let cost_model = UnitCostModel::new(Language::Ruby);
    assert!(
        cost_model.ren(&comment("# @return [Boolean]"), &comment("#")) > COST_DELETE + COST_INSERT
    );
    assert!(
        cost_model.ren(&comment("# old words"), &comment("# new words"))
            < COST_DELETE + COST_INSERT
    );
}

#[test]
fn ren_keeps_leaf_updates_strictly_cheaper_than_delete_plus_insert() {
    let leaf =
        |kind: &str, text: &str| ASTNodeMetadata::new(kind.into(), text.into(), vec![], 0, 0);
    let cost_model = UnitCostModel::new(Language::Rust);
    for (kind, a, b) in [("integer_literal", "1", "2"), ("identifier", "a", "b")] {
        let cost = cost_model.ren(&leaf(kind, a), &leaf(kind, b));
        assert!(
            cost > 0 && cost < COST_DELETE + COST_INSERT,
            "{kind}: {cost}"
        );
    }
}

#[test]
fn apted_whole_tree_hands_an_oversized_pair_to_the_kernel_instead_of_decomposing_it() {
    // Functions rather than bare statements, so the root is not a flat container and only the
    // cell gate keeps the pair out of the kernel.
    let source = |changed: usize| -> String {
        (0..120)
            .map(|i| {
                if i == changed {
                    "def changed():\n    return -1\n".to_string()
                } else {
                    format!("def f{i}():\n    return {i}\n")
                }
            })
            .collect()
    };
    let before = Code::from_string(&source(usize::MAX), &Language::Python);
    let after = Code::from_string(&source(60), &Language::Python);
    let nodes = |code: &Code| code.ast.as_ref().unwrap().root_node().descendant_count();
    let cells = nodes(&before) * nodes(&after);
    assert!(
        cells > APTED_MAX_CELLS,
        "the pair must be oversized for this test to mean anything"
    );

    let reasons = |algorithm: Algorithm| {
        let mut diff = ASTDiff::default();
        for_roots(&before, &after, algorithm, "test", &mut diff);
        let reasons: std::collections::HashSet<_> =
            diff.mapping.values().map(|m| m.reason).collect();
        (diff.mapping.len(), reasons)
    };
    let (gated_len, gated) = reasons(Algorithm::Apted);
    let (whole_len, whole) = reasons(Algorithm::AptedWholeTree);
    assert!(gated_len > 0 && whole_len > 0);
    // The gate decomposes the pair and settles the children as a flat sequence; the whole-tree
    // run never leaves the kernel.
    assert!(
        gated.contains(&ASTMappingReason::FlatSequenceDiff),
        "{gated:?}"
    );
    assert_eq!(
        whole,
        [ASTMappingReason::APTED("test")].into_iter().collect(),
        "{whole:?}"
    );
}

fn matched(diff: &mut ASTDiff, before_id: usize, after_id: usize) {
    diff.add_mapping(
        before_id,
        after_id,
        ASTMapping::identical(ASTMappingReason::APTED("test")),
    );
}

#[test]
fn split_into_anchored_segments_splits_at_matched_children_and_drops_them() {
    let mut diff = ASTDiff::default();
    matched(&mut diff, 3, 12);
    assert_eq!(
        split_into_anchored_segments(&[1, 2, 3, 4, 5], &[11, 12, 13, 14], &diff),
        vec![(vec![1, 2], vec![11]), (vec![4, 5], vec![13, 14])]
    );
}

#[test]
fn split_into_anchored_segments_ignores_an_anchor_whose_partner_is_behind_the_last_split() {
    let mut diff = ASTDiff::default();
    matched(&mut diff, 2, 12);
    matched(&mut diff, 4, 11);
    assert_eq!(
        split_into_anchored_segments(&[1, 2, 3, 4, 5], &[11, 12, 13], &diff),
        vec![(vec![1], vec![]), (vec![3, 5], vec![13])]
    );
}

#[test]
fn anchor_leftovers_by_member_name_zips_equal_counts_and_leaves_unequal_counts_alone() {
    let before = Code::from_string(
        "class A {\n  A(int x) { f(1); }\n  A() { f(2); }\n  void q(int x) { g(1); }\n  \
         void q() { g(2); }\n  void r() { h(1); }\n}\n",
        &Language::Java,
    );
    let after = Code::from_string(
        "class A {\n  A(int x) { f(3); }\n  A() { f(4); }\n  void q() { g(3); }\n  \
         void r() { h(2); }\n}\n",
        &Language::Java,
    );
    let before_meta = crate::code::metadata::metadata_of(&before);
    let after_meta = crate::code::metadata::metadata_of(&after);
    let members = |code: &Code, meta: &ASTMetadata| {
        let body = helper::find_first_of_kind(code.ast.as_ref().unwrap().root_node(), "class_body")
            .unwrap();
        let children = meta.node_info[&body.id()].children.clone();
        let named: Vec<usize> = children
            .iter()
            .copied()
            .filter(|id| meta.node_info[id].kind.ends_with("_declaration"))
            .collect();
        (children, named)
    };
    let (before_children, b) = members(&before, &before_meta);
    let (after_children, a) = members(&after, &after_meta);

    let mut diff = ASTDiff::default();
    let (before_left, after_left) = anchor_leftovers_by_member_name(
        before_children,
        after_children,
        &before_meta,
        &after_meta,
        "test",
        &mut diff,
    );

    // Constructors pair in document order, `r` by its unique name.
    assert_eq!(diff.before_node_map.get(&b[0]), Some(&a[0]));
    assert_eq!(diff.before_node_map.get(&b[1]), Some(&a[1]));
    assert_eq!(diff.before_node_map.get(&b[4]), Some(&a[3]));
    // Two `q`s against one: which was removed is not a question a name can answer.
    assert!(before_left.contains(&b[2]) && before_left.contains(&b[3]));
    assert!(after_left.contains(&a[2]));
    assert!(!diff.before_node_map.contains_key(&b[2]));
}

#[test]
fn widest_statement_sequence_body_takes_the_shallowest_body_over_a_wider_nested_one() {
    let code = Code::from_string(
        "def f():\n    a = 1\n    for i in x:\n        b = 1\n        c = 2\n        d = 3\n",
        &Language::Python,
    );
    let meta = crate::code::metadata::metadata_of(&code);
    let function = helper::find_first_of_kind(
        code.ast.as_ref().unwrap().root_node(),
        "function_definition",
    )
    .unwrap();
    let function_body = helper::find_first_of_kind(function, "block").unwrap();
    assert_eq!(
        widest_statement_sequence_body(function.id(), &meta),
        Some((2, function_body.id()))
    );
}

#[test]
fn prematch_identical_statement_siblings_maps_only_identical_statements() {
    let source = |c: &str| format!("def f():\n    a = 1\n    b = 2\n    c = {c}\n    d = 4\n");
    let before = Code::from_string(&source("3"), &Language::Python);
    let after = Code::from_string(&source("30"), &Language::Python);
    let before_meta = crate::code::metadata::metadata_of(&before);
    let after_meta = crate::code::metadata::metadata_of(&after);
    fn function(code: &Code) -> tree_sitter::Node<'_> {
        helper::find_first_of_kind(
            code.ast.as_ref().unwrap().root_node(),
            "function_definition",
        )
        .unwrap()
    }
    let before_fn = function(&before);
    let changed = helper::find_first_of_kind(before_fn, "block")
        .unwrap()
        .named_child(2)
        .unwrap();

    let mut diff = ASTDiff::default();
    prematch_identical_statement_siblings(
        before_fn.id(),
        function(&after).id(),
        &before_meta,
        &after_meta,
        "test",
        &mut diff,
    );

    assert!(!diff.mapping.is_empty());
    assert!(
        diff.mapping
            .values()
            .all(|m| matches!(m.operation, ASTMappingOperation::Identical))
    );
    assert!(!diff.before_node_map.contains_key(&changed.id()));
}

/// Leaves `id -> kind "stmt"` with a similarity sketch over `hashes`.
fn sketched(entries: &[(usize, &[u64])]) -> ASTMetadata {
    let mut meta = ASTMetadata::default();
    for &(id, hashes) in entries {
        meta.node_info.insert(
            id,
            ASTNodeMetadata::new("stmt".to_string(), String::new(), vec![], id, id),
        );
        meta.node_to_similarity_sketch.insert(
            id,
            crate::code::similarity::SimilaritySketch::merge(
                hashes
                    .iter()
                    .map(|&h| crate::code::similarity::SimilaritySketch::leaf(h)),
            ),
        );
    }
    meta
}

#[test]
fn mutual_similarity_pairs_a_rewrite_and_leaves_the_surplus_as_inserts() {
    let before = sketched(&[(1, &[1, 2, 3])]);
    let after = sketched(&[(10, &[1, 2, 4]), (11, &[7, 8, 9])]);
    assert_eq!(
        align_segment_by_mutual_similarity(&[1], &[10, 11], &before, &after),
        vec![(0, 0)]
    );
}

#[test]
fn mutual_similarity_refuses_a_tie_between_two_candidates() {
    let before = sketched(&[(1, &[1, 2, 3])]);
    let after = sketched(&[(10, &[1, 2, 4]), (11, &[1, 2, 4])]);
    assert!(align_segment_by_mutual_similarity(&[1], &[10, 11], &before, &after).is_empty());
}

#[test]
fn mutual_similarity_pairs_nothing_unless_the_smaller_side_is_fully_paired() {
    let before = sketched(&[(1, &[1, 2, 3]), (2, &[20, 21, 22])]);
    let after = sketched(&[(10, &[1, 2, 4]), (11, &[7, 8, 9]), (12, &[30, 31])]);
    assert!(align_segment_by_mutual_similarity(&[1, 2], &[10, 11, 12], &before, &after).is_empty());
}

#[test]
fn containment_forbids_pairing_a_hollowed_out_ancestor_away_from_its_pruned_descendant() {
    // before: 1 -> 2 -> 3; after: 10 -> (11, 12 -> 13); 3 is already matched to 13.
    let before = synthetic_meta(&[(1, "r", "", &[2]), (2, "p", "", &[3]), (3, "x", "x", &[])]);
    let after = synthetic_meta(&[
        (10, "r", "", &[11, 12]),
        (11, "p", "", &[]),
        (12, "p", "", &[13]),
        (13, "x", "x", &[]),
    ]);
    let mut diff = ASTDiff::default();
    matched(&mut diff, 3, 13);
    let ctx = ContainmentCtx::build(&[1], &[10], &before, &after, &diff, "test");
    assert_eq!(ctx.adjust(2, 11, 0), FORBIDDEN_RENAME_COST);
    assert_eq!(ctx.adjust(2, 12, 0), 0);
}

#[test]
fn longest_increasing_by_second_drops_pairs_that_contradict_the_longest_ordered_run() {
    assert_eq!(
        longest_increasing_by_second(&[(1, 10), (2, 40), (3, 20), (4, 30)]),
        vec![(1, 10), (3, 20), (4, 30)]
    );
}

#[test]
fn slot_lcs_anchor_outweighs_every_promotion_it_blocks() {
    // The anchor (0, 2) crosses three otherwise compatible promotions and still wins.
    let pairs = weighted_lcs_pairs(4, 3, |i, j| match (i, j) {
        (0, 2) => SLOT_LCS_ANCHOR_WEIGHT,
        (0, _) | (_, 2) => 0,
        _ => 1,
    });
    assert_eq!(pairs, vec![(0, 2)]);
}

#[test]
fn for_roots_and_the_fallback_are_no_ops_without_an_ast() {
    let before = Code::from_string("some text", &Language::Unknown);
    let after = Code::from_string("other text", &Language::Unknown);
    let mut diff = ASTDiff::default();
    for_roots(&before, &after, Algorithm::Apted, "test", &mut diff);
    crate::diff::apted::for_roots_fallback(&before, &after, "test", &mut diff);
    assert!(diff.mapping.is_empty());
}
