Data structure enhancement: Added methods and traits to a basic struct to make it more functional and useful. This represents a common Rust evolution pattern.

Key changes:
- Added `#[derive(Debug, Clone, Copy)]` for automatic trait implementations
- Added `impl Point` block with methods
- Added constructor method `new(x: i32, y: i32)`
- Added computed method `distance_from_origin()`
- Updated main function to use the new constructor and method
- Enhanced output to include distance information

This shows how basic data structures evolve to become more feature-rich in Rust.