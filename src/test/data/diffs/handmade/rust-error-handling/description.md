Error handling improvement: Converted from panic-based error handling (unwrap()) to proper Result-based error handling using the ? operator. This is a common Rust refactoring pattern.

Key changes:
- Changed return type from `String` to `Result<String, std::io::Error>`
- Replaced `.unwrap()` calls with `?` operator
- Updated main function to return `Result<(), std::io::Error>`
- Added proper error propagation

This represents a typical Rust code quality improvement where error handling is made more robust.