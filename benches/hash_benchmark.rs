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

use codediff::code::{Code, Language};
use codediff::diff::hash::hash_code;
use codediff::diff::ASTMetadata;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

fn benchmark_hash_code(c: &mut Criterion) {
    // Load test code files
    let test_codes = codediff::test::helper::handmade_test_code().expect("Failed to load test codes");

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

fn benchmark_large_file_hashing(c: &mut Criterion) {
    // Create a larger test file for more realistic benchmarking
    let large_code = r#"
fn factorial(n: u32) -> u32 {
    if n <= 1 {
        return 1;
    }
    n * factorial(n - 1)
}

fn fibonacci(n: u32) -> u32 {
    if n <= 1 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

fn main() {
    let mut results = Vec::new();
    for i in 0..20 {
        results.push(factorial(i));
        results.push(fibonacci(i));
    }
    
    for (i, result) in results.iter().enumerate() {
        println!("Result {}: {}", i, result);
    }
}

struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

enum Shape {
    Circle { radius: f64, center: Point },
    Rectangle { width: f64, height: f64, top_left: Point },
    Triangle { points: [Point; 3] },
}

fn process_shapes(shapes: &[Shape]) -> f64 {
    let mut total_area = 0.0;
    for shape in shapes {
        match shape {
            Shape::Circle { radius, .. } => {
                total_area += std::f64::consts::PI * radius * radius;
            }
            Shape::Rectangle { width, height, .. } => {
                total_area += width * height;
            }
            Shape::Triangle { points } => {
                // Simple area calculation for triangle
                let a = points[0].distance(&points[1]);
                let b = points[1].distance(&points[2]);
                let c = points[2].distance(&points[0]);
                let s = (a + b + c) / 2.0;
                total_area += (s * (s - a) * (s - b) * (s - c)).sqrt();
            }
        }
    }
    total_area
}
"#;

    let code = Code::from_string(large_code, &Language::Rust);

    c.bench_function("large_file_hashing", |b| {
        b.iter(|| {
            let mut metadata = ASTMetadata::default();
            hash_code(black_box(&code), black_box(&mut metadata)).expect("Hashing failed");
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().configure_from_args();
    targets = benchmark_hash_code, benchmark_large_file_hashing
}

criterion_main!(benches);
