# Diff Module Bugs - Discovered by AI

This document contains bugs and potential issues identified by AI analysis of the `src/diff/` directory.

## Critical Bugs

### 1. Memory Safety Issue in NodeCache
**File:** diff.rs  
**Lines:** 47-121, specifically 72-76 and 100-104  
**Severity:** CRITICAL  

The `NodeCache` uses `unsafe` code to transmute tree-sitter `Node<'_>` references to `'static` lifetimes. The documentation acknowledges this is a "lie" and relies on a safety invariant that callers must never let the cache outlive the `Code` objects it was built from.

**Bug:** There's no enforcement mechanism. If a `NodeCache` is stored in a struct or returned from a function, it creates undefined behavior - silent memory corruption that could crash or produce wrong results.

**Fix needed:** Either:
- Use `Rc<tree_sitter::Tree>` to share ownership
- Or use `OwnedNode` pattern
- Or add compile-time lifetime guarantees

---

### 2. Integer Index Logic Bug in row_col_to_char_index
**File:** text_range.rs  
**Lines:** 214-242, specifically 236  
**Severity:** HIGH  

The function iterates through `code.chars()` and increments `char_index` for each character. 

**Bug:** Line 236 has inverted logic:
```rust
if current_row == row && current_col <= col {
    return char_index;
}
```
This returns the wrong index when `current_col < col` at the end of the string. If the position is beyond the string, it should return `char_index` (line 241), but the logic is inverted - it returns `char_index` when the position is *before* or *at* the end, not after.

---

## High Severity Bugs

### 3. Duplicate Hash Group Collapse
**File:** solve_identical_trees.rs  
**Lines:** 72-91  
**Severity:** HIGH  

The code has a known issue documented in its own test `duplicate_hash_group_collapses_onto_a_single_after_node` (lines 194-268). When multiple nodes share the same hash:

- All before-nodes with the same hash map to the **same** after-node
- Other equally-valid after-nodes are left unmatched
- This causes incorrect diff results for duplicate code

The comment on lines 76-89 explicitly acknowledges this is a TODO and needs a better matching strategy.

---

## Medium Severity Bugs

### 4. Incorrect Index Out of Bounds Check
**File:** text_range.rs  
**Lines:** 114-117  
**Severity:** MEDIUM  

```rust
if end_row < columns_per_row.len() && columns_per_row[end_row] == end_column {
    end_row += 1;
    end_column = 0;
}
```

**Bug:** If `end_row` equals `columns_per_row.len()` (i.e., the end is exactly at the boundary), this silently does nothing. But if the range end is at the end of the last line, it should still advance to the next row. The condition should be `end_row <= columns_per_row.len()`.

---

### 5. Inconsistent Range Zero Check
**File:** text_range.rs  
**Lines:** 98-100  
**Severity:** MEDIUM  

```rust
pub fn is_zero(&self) -> bool {
    self.start_row == 0 && self.start_column == 0 && self.end_row == 0 && self.end_column == 0
}
```

**Bug:** This only checks for the specific zero range at (0,0). However, an "empty range" can exist at any position where `start_row == end_row && start_column == end_column`. The function should be:
```rust
pub fn is_zero(&self) -> bool {
    self.start_row == self.end_row && self.start_column == self.end_column
}
```

This is also inconsistent with `RangeMatch::is_zero()` (text.rs:354-358) which additionally checks the operation.

---

### 6. Potential Integer Overflow in APTED
**File:** engine.rs  
**Lines:** 240  
**Severity:** MEDIUM  

```rust
sz * (sz + 3) / 2 - desc_sum_total[pre]
```

While `sz` is always at least 1 (line 200 initializes sizes to 1), the formula `sz * (sz + 3) / 2` could overflow for very large subtrees (though unlikely in practice for code ASTs).

---

### 7. Python Method Double-Diffing
**File:** solve_semantically_structural_nodes.rs  
**Lines:** 148-165  
**Severity:** MEDIUM  

For Python class methods, when calling `apted::for_nodes` for individual methods (lines 148-156), if a method doesn't have a match, it's silently skipped. Then later, `for_nodes` is called again on the entire class (lines 158-165). This could lead to:
- Methods being diffed twice
- Inconsistent matching if a method was already partially matched

---

### 8. Ignored Results from for_nodes
**Files:** solve_identical_trees.rs, solve_semantically_structural_nodes.rs, solve_structurally_identical_trees.rs  
**Lines:** Multiple locations  
**Severity:** MEDIUM  

Multiple places ignore return values from `apted::for_nodes` calls. The `for_nodes` function returns a `Result<()>` that is consistently ignored. If an error occurs during APTED computation, it will be silently dropped.

Affected locations:
- solve_identical_trees.rs: 102-110
- solve_semantically_structural_nodes.rs: 148-156, 189-197, 203-210, 225-232, 251-258, 269-277
- solve_structurally_identical_trees.rs: 99-128

---

## Low Severity Issues

### 9. Inconsistent Cost Model
**File:** diff.rs  
**Lines:** 134-137  
**Severity:** LOW  

```rust
pub const COST_INSERT: u64 = 1;
pub const COST_DELETE: u64 = 1;
pub const COST_UPDATE: u64 = 1;
pub const COST_MOVE: u64 = 0;
```

The cost model treats move as free (0), which could lead to the algorithm preferring moves over structurally better matches in some edge cases.

---

### 10. APTED DeltaTable Silent Defaults
**File:** apted/common.rs  
**Lines:** 263-266  
**Severity:** LOW  

```rust
pub(crate) fn get(&self, pre_before: usize, pre_after: usize) -> u64 {
    let v = self.data[pre_before * self.cols + pre_after];
    if v == Self::UNSET { 0 } else { v }
}
```

No bounds checking - returns 0 for unset values, which could hide bugs. Missing validation that indices are within bounds.

---

## Discovery Information

**Discovered by:** AI analysis using Mistral Vibe  
**Date:** 2026-07-01  
**Analysis scope:** All files in src/diff/ directory including subdirectory apted/  
**Files analyzed:**
- src/diff.rs
- src/diff/text.rs
- src/diff/text_range.rs
- src/diff/reference_nodes.rs
- src/diff/semantic_structure_nodes.rs
- src/diff/solve_identical_trees.rs
- src/diff/solve_semantically_structural_nodes.rs
- src/diff/solve_structurally_identical_trees.rs
- src/diff/apted/mod.rs
- src/diff/apted/common.rs
- src/diff/apted/engine.rs
- src/diff/apted/zhang_shasha.rs
