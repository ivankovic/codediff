Bug fix: Fixed array iteration logic in find_duplicate function. The original code had a subtle bug where it directly iterated over nums, which could cause issues with certain input types. The fix uses proper indexing with range(len(nums)) to ensure correct iteration.

Key changes:
- Changed `for num in nums:` to `for i in range(len(nums)):`
- Added explicit indexing: `num = nums[i]`
- Added additional test case for edge case with single element

This represents a typical bug fix pattern where iteration logic is corrected.