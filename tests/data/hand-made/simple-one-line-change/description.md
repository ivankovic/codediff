In the function Solution::two_sum, the following code was added:

```rust
        for (i, v) in nums.iter().enumerate() {
            indices.insert(v, i);
        }
```
