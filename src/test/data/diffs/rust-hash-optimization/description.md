Performance optimization: Replaced simple Vec<usize> hash keys with a custom SubtreeKey struct that pre-computes hashes for faster memoization lookups. This optimization makes the hash computation more efficient by caching the hash value.

Key changes:
- Added SubtreeKey struct with pre-computed hash
- Implemented Hash trait for SubtreeKey
- Changed memoization key from (Vec<usize>, Vec<usize>) to (SubtreeKey, SubtreeKey)
- Added proper hash computation in SubtreeKey::new()

This represents the optimization commit 90494d2 "faster hash key"