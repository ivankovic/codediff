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

use codediff::diff::ASTMetadata;
use codediff::diff::hash::hash_code;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;

fn benchmark_hash_code(c: &mut Criterion) {
    // Load test code files
    let test_codes =
        codediff::test::helper::handmade_test_code().expect("Failed to load test codes");

    let mut group = c.benchmark_group("hash_code");
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(2));

    for (filename, code) in test_codes {
        group.bench_function(filename, |b| {
            b.iter(|| {
                let mut metadata = ASTMetadata::default();
                hash_code(black_box(&code), black_box(&mut metadata)).expect("Hashing failed");
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().configure_from_args();
    targets = benchmark_hash_code
}

criterion_main!(benches);
