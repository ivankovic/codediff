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

//! Codediff's *rendering* against the human-painted text ground truth, one test per painted
//! fixture, in both [`RenderMode`]s.
//!
//! The sibling of `optimal_solutions`, one level down. That module checks the node **mapping**
//! against `human_mapping.json`'s `entries`; this checks what a reader actually sees against its
//! `text_mappings` - two independent ground truths, and neither implies the other (see
//! `HumanTextMapping`). A fixture can map every node exactly right and still paint the wrong
//! bytes, which is precisely the gap these tests exist to measure.
//!
//! **Every fixture is clamped, and every clamp is large.** Nothing agrees exactly yet: the rates
//! below run from 0.16% to 62%, measured 2026-08-26 over the 26 fixtures painted so far. These are
//! recorded distances, not targets - the same posture `assert_matches_human_mapping_within_limit`
//! takes, and for the same reason. Lower one when a change earns it; a rise is a regression.
//!
//! **What the percentage is.** Bytes where codediff's rendering and the human's painting disagree
//! about what happened to that text, over the bytes in both files. A rate rather than a count
//! because the fixtures span three orders of magnitude in size, and a count would let one large
//! fixture's residual dwarf every small fixture's exactness.
//!
//! **Which painting each mode answers to.** A fixture painted twice has one answer per mode:
//! `Minimal` mode against the `Minimal` painting, `Full` against `Full`. A fixture painted once is
//! asserting its rendering is unambiguous, so *both* modes are held to that single painting -
//! which is exactly what painting it once claims.
//!
//! To re-measure, run these tests: a failure reports the current rate for both modes, which is the
//! number to record. See `research/data/quality/text_painting_findings.md` for what the two
//! painting styles mean and where they came from.

use anyhow::Result;

use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;

#[test]
fn cpp_add_const_correctness() -> Result<()> {
    // measured 2026-08-26: minimal 22.103%, full 22.103%
    assert_matches_human_painting_within_limit("cpp-add-const-correctness", 22.12)
}

#[test]
fn cpp_add_memory_management() -> Result<()> {
    // measured 2026-08-26: minimal 7.963%, full 1.667%
    assert_matches_human_painting_within_limit("cpp-add-memory-management", 7.98)
}

#[test]
fn cpp_add_templates() -> Result<()> {
    // measured 2026-08-26: minimal 31.440%, full 6.288%
    assert_matches_human_painting_within_limit("cpp-add-templates", 31.45)
}

#[test]
fn cpp_fix_segfault() -> Result<()> {
    // measured 2026-08-26: minimal 2.817%, full 2.817%
    assert_matches_human_painting_within_limit("cpp-fix-segfault", 2.83)
}

#[test]
fn cpp_optimize_algorithm() -> Result<()> {
    // measured 2026-08-26: minimal 0.160%, full 0.799%
    assert_matches_human_painting_within_limit("cpp-optimize-algorithm", 0.81)
}

#[test]
fn java_add_exception_handling() -> Result<()> {
    // measured 2026-08-26: minimal 57.770%, full 57.770%
    assert_matches_human_painting_within_limit("java-add-exception-handling", 57.78)
}

#[test]
fn java_add_interface() -> Result<()> {
    // measured 2026-08-26: minimal 0.332%, full 0.997%
    assert_matches_human_painting_within_limit("java-add-interface", 1.01)
}

#[test]
fn java_add_logging() -> Result<()> {
    // measured 2026-08-26: minimal 0.400%, full 0.400%
    assert_matches_human_painting_within_limit("java-add-logging", 0.41)
}

#[test]
fn java_fix_array_index() -> Result<()> {
    // measured 2026-08-26: minimal 7.266%, full 7.266%
    assert_matches_human_painting_within_limit("java-fix-array-index", 7.28)
}

#[test]
fn java_refactor_constants() -> Result<()> {
    // measured 2026-08-26: minimal 13.805%, full 13.805%
    assert_matches_human_painting_within_limit("java-refactor-constants", 13.82)
}

#[test]
fn javascript_add_array_method() -> Result<()> {
    // measured 2026-08-26: minimal 0.260%, full 1.299%
    assert_matches_human_painting_within_limit("javascript-add-array-method", 1.31)
}

#[test]
fn javascript_add_destructuring() -> Result<()> {
    // measured 2026-08-26: minimal 26.269%, full 26.269%
    assert_matches_human_painting_within_limit("javascript-add-destructuring", 26.28)
}

#[test]
fn javascript_add_event_listener() -> Result<()> {
    // measured 2026-08-26: minimal 16.708%, full 16.708%
    assert_matches_human_painting_within_limit("javascript-add-event-listener", 16.72)
}

#[test]
fn javascript_fix_promises() -> Result<()> {
    // measured 2026-08-26: minimal 7.420%, full 2.698%
    assert_matches_human_painting_within_limit("javascript-fix-promises", 7.43)
}

#[test]
fn javascript_refactor_arrow_func() -> Result<()> {
    // measured 2026-08-26: minimal 3.955%, full 6.780%
    assert_matches_human_painting_within_limit("javascript-refactor-arrow-func", 6.79)
}

#[test]
fn kotlin_add_data_class() -> Result<()> {
    // measured 2026-08-26: minimal 3.546%, full 5.201%
    assert_matches_human_painting_within_limit("kotlin-add-data-class", 5.22)
}

#[test]
fn kotlin_add_null_check() -> Result<()> {
    // measured 2026-08-26: minimal 0.239%, full 0.239%
    assert_matches_human_painting_within_limit("kotlin-add-null-check", 0.25)
}

#[test]
fn kotlin_add_validation() -> Result<()> {
    // measured 2026-08-26: minimal 0.163%, full 0.163%
    assert_matches_human_painting_within_limit("kotlin-add-validation", 0.18)
}

#[test]
fn kotlin_fix_loop_bug() -> Result<()> {
    // measured 2026-08-26: minimal 3.800%, full 3.800%
    assert_matches_human_painting_within_limit("kotlin-fix-loop-bug", 3.81)
}

#[test]
fn kotlin_refactor_function() -> Result<()> {
    // measured 2026-08-26: minimal 62.178%, full 61.605%
    assert_matches_human_painting_within_limit("kotlin-refactor-function", 62.19)
}

#[test]
fn python_add_remove_block() -> Result<()> {
    // measured 2026-08-26: minimal 1.479%, full 1.479%
    assert_matches_human_painting_within_limit("python-add-remove-block", 1.49)
}

#[test]
fn python_added_if_block() -> Result<()> {
    // measured 2026-08-26: minimal 4.045%, full 0.213%
    assert_matches_human_painting_within_limit("python-added-if-block", 4.06)
}

#[test]
fn python_added_if_block_small() -> Result<()> {
    // measured 2026-08-26: minimal 20.567%, full 2.128%
    assert_matches_human_painting_within_limit("python-added-if-block-small", 20.58)
}

#[test]
fn python_api_change() -> Result<()> {
    // measured 2026-08-26: minimal 27.160%, full 27.945%
    assert_matches_human_painting_within_limit("python-api-change", 27.96)
}

#[test]
fn python_bugfix_loop() -> Result<()> {
    // measured 2026-08-26: minimal 0.698%, full 0.853%
    assert_matches_human_painting_within_limit("python-bugfix-loop", 0.87)
}

#[test]
fn python_refactoring() -> Result<()> {
    // measured 2026-08-26: minimal 2.611%, full 2.611%
    assert_matches_human_painting_within_limit("python-refactoring", 2.63)
}

#[test]
fn rust_add_if() -> Result<()> {
    // measured 2026-08-26: minimal 44.203%, full 0.725%
    assert_matches_human_painting_within_limit("rust-add-if", 44.22)
}

#[test]
fn rust_add_to_existing_use() -> Result<()> {
    // measured 2026-08-26: minimal 11.189%, full 4.196%
    assert_matches_human_painting_within_limit("rust-add-to-existing-use", 11.20)
}

#[test]
fn rust_add_value_to_enum() -> Result<()> {
    // measured 2026-08-26: minimal 0.069%, full 0.069%
    assert_matches_human_painting_within_limit("rust-add-value-to-enum", 0.08)
}

#[test]
fn rust_cost_optimization() -> Result<()> {
    // measured 2026-08-26: minimal 5.495%, full 5.495%
    assert_matches_human_painting_within_limit("rust-cost-optimization", 5.51)
}

#[test]
fn rust_sniffnet_protocol() -> Result<()> {
    // measured 2026-08-26: minimal 0.105%, full 0.315%
    assert_matches_human_painting_within_limit("rust-sniffnet-protocol", 0.33)
}

#[test]
fn rust_tauri_api_build_1() -> Result<()> {
    // measured 2026-08-26: minimal 0.028%, full 0.028%
    assert_matches_human_painting_within_limit("rust-tauri-api-build-1", 0.04)
}

#[test]
fn typescript_async_await() -> Result<()> {
    // measured 2026-08-26: minimal 30.474%, full 33.634%
    assert_matches_human_painting_within_limit("typescript-async-await", 33.65)
}
