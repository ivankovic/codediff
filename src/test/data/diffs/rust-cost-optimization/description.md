Algorithm optimization: Added early termination check in the solve function to skip expensive computations when the current cost already exceeds the best known cost. This represents the optimization from commit 81bf47c "Best cost optimization, 10% faster".

Key changes:
- Added `if cost >= best_cost { continue; }` check before updating best_cost
- This allows the algorithm to skip entire branches of subproblems that cannot possibly result in a better solution
- The optimization is particularly effective in pruning the search space

This pattern is common in optimization algorithms where early termination can significantly improve performance.