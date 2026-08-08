Code structure change: Added a new filtering block to the process_data function. This represents adding a significant new code block that changes the function's behavior.

Key changes:
- Added new if block: `if len(result) > 0:`
- Added nested for loop for filtering: `for num in result:`
- Added conditional filtering: `if num < 10:`
- Added early return with filtered results

This pattern shows how functions evolve by adding new processing blocks while maintaining the existing structure.