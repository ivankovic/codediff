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

use codediff::diff::diff_code;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

fn benchmark_diff_code(c: &mut Criterion) {
    // Load test diffs
    let test_diffs =
        codediff::test::helper::handmade_test_code_pairs().expect("Failed to load test diffs");

    let mut group = c.benchmark_group("diff_code");
    group.measurement_time(Duration::from_secs(60));
    group.warm_up_time(Duration::from_secs(2));

    // Automatically create benchmarks for all test diffs
    for (test_name, (before, after)) in test_diffs.iter() {
        // Create a safe benchmark name by replacing non-alphanumeric characters
        let benchmark_name = test_name
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")
            .to_lowercase();

        group.bench_function(benchmark_name, |b| {
            let before = before.clone();
            let after = after.clone();
            b.iter(|| {
                diff_code(black_box(&before), black_box(&after));
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().configure_from_args();
    targets = benchmark_diff_code
}

criterion_main!(benches);
