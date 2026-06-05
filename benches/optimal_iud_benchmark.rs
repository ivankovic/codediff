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

use codediff::diff::optimal_iud::for_roots;
use codediff::diff::{ASTDiff, NodeCache};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

fn benchmark_optimal_iud_for_roots(c: &mut Criterion) {
    // Load test code files and diffs
    let test_codes =
        codediff::test::helper::handmade_test_code().expect("Failed to load test codes");
    let test_code_pairs =
        codediff::test::helper::handmade_test_code_pairs().expect("Failed to load test code pairs");

    let mut group = c.benchmark_group("optimal_iud_find");
    group.measurement_time(Duration::from_secs(60));
    group.warm_up_time(Duration::from_secs(2));

    // Automatically create benchmarks for all test code pairs
    for (test_name, (before, after)) in &test_code_pairs {
        // Create a safe benchmark name by replacing non-alphanumeric characters
        let benchmark_name = test_name
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")
            .to_lowercase();

        group.bench_function(benchmark_name, |b| {
            let before = before.clone();
            let after = after.clone();
            b.iter(|| {
                let mut diff = ASTDiff::default();
                let node_cache = NodeCache::build(black_box(&before), black_box(&after));
                for_roots(
                    black_box(&before),
                    black_box(&after),
                    black_box(&node_cache),
                    black_box(&mut diff),
                )
                .expect("find failed");
            });
        });
    }

    // Also benchmark the hello-world translation case which is in test_codes but not test_diffs
    if let (Some(before_hello), Some(after_hello)) = (
        test_codes.get("hello-world.rs"),
        test_codes.get("zdravo-svijete.rs")
    ) {
        group.bench_function("hello_world_translation", |b| {
            let before_hello = before_hello.clone();
            let after_hello = after_hello.clone();
            b.iter(|| {
                let mut diff = ASTDiff::default();
                let node_cache = NodeCache::build(black_box(&before_hello), black_box(&after_hello));
                for_roots(
                    black_box(&before_hello),
                    black_box(&after_hello),
                    black_box(&node_cache),
                    black_box(&mut diff),
                )
                .expect("find failed");
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().configure_from_args();
    targets = benchmark_optimal_iud_find
}

criterion_main!(benches);
