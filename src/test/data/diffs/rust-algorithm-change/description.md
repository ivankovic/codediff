Algorithm improvement: Changed from O(n²) brute force approach to O(n) HashSet-based approach for finding duplicates. This represents a significant performance optimization.

Key changes:
- Added `use std::collections::HashSet;` import
- Replaced nested loops with single loop using HashSet
- Changed from index-based access to direct value iteration
- Improved time complexity from O(n²) to O(n)
- More memory efficient approach

This shows how algorithms are often optimized by leveraging appropriate data structures.