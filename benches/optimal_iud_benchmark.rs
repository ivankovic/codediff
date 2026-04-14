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

use codediff::diff::optimal_iud::find;
use codediff::diff::{ASTDiff, NodeCache};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

fn benchmark_optimal_iud_find(c: &mut Criterion) {
    // Load test code files and diffs
    let test_codes =
        codediff::test::helper::handmade_test_code().expect("Failed to load test codes");
    let test_diffs =
        codediff::test::helper::handmade_test_diffs().expect("Failed to load test diffs");

    let mut group = c.benchmark_group("optimal_iud_find");
    group.measurement_time(Duration::from_secs(60));
    group.warm_up_time(Duration::from_secs(2));

    // 1. Simple translation (hello-world.rs -> zdravo-svijete.rs)
    let before_hello = test_codes.get("hello-world.rs").unwrap().clone();
    let after_hello = test_codes.get("zdravo-svijete.rs").unwrap().clone();
    group.bench_function("hello_world_translation", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache = NodeCache::build(black_box(&before_hello), black_box(&after_hello));
            find(
                black_box(&before_hello),
                black_box(&after_hello),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    // 2. Line addition (hello-world-added-message)
    let (before_add, after_add) = test_diffs.get("hello-world-added-message").unwrap().clone();
    group.bench_function("hello_world_added_message", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache = NodeCache::build(black_box(&before_add), black_box(&after_add));
            find(
                black_box(&before_add),
                black_box(&after_add),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    // 3. Line deletion (reverse of hello-world-added-message)
    let (after_del, before_del) = test_diffs.get("hello-world-added-message").unwrap().clone();
    group.bench_function("hello_world_deleted_message", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache = NodeCache::build(black_box(&before_del), black_box(&after_del));
            find(
                black_box(&before_del),
                black_box(&after_del),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    // 4. Complex bugfix (leet-code-1-bugfix)
    let (before_bugfix, after_bugfix) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();
    group.bench_function("leetcode_1_bugfix", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache = NodeCache::build(black_box(&before_bugfix), black_box(&after_bugfix));
            find(
                black_box(&before_bugfix),
                black_box(&after_bugfix),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    // 5. Python if block addition
    let (before_python, after_python) = test_diffs.get("python-added-if-block").unwrap().clone();
    group.bench_function("python_added_if_block", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache = NodeCache::build(black_box(&before_python), black_box(&after_python));
            find(
                black_box(&before_python),
                black_box(&after_python),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    // 6. Python if block deletion (reverse)
    let (after_python_del, before_python_del) =
        test_diffs.get("python-added-if-block").unwrap().clone();
    group.bench_function("python_deleted_if_block", |b| {
        b.iter(|| {
            let mut diff = ASTDiff::default();
            let node_cache =
                NodeCache::build(black_box(&before_python_del), black_box(&after_python_del));
            find(
                black_box(&before_python_del),
                black_box(&after_python_del),
                black_box(&node_cache),
                black_box(&mut diff),
            )
            .expect("find failed");
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().configure_from_args();
    targets = benchmark_optimal_iud_find
}

criterion_main!(benches);
