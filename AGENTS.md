# Development Workflow

- Always read the README.md file in the root of the repository. Always.
- Always read README.md in any directory in this repository before you read or write any files in
that directory.

- `make benchmark-quality` (the `benchmark_optimal_solutions` binary) shows a change's effect on
accuracy and speed.

## Markdown files

- Do NOT update the README.md files unless explicitly asked to do so.
- There are no TODO.md or SPECS.md files; do not create them. Working notes - follow-ups,
experiments and their numbers, negative results, and design decisions with their reasons - go in
AGENT_LOG.md at the repository root, which is git-ignored and never committed. Read it before
starting a task; when you complete an item from it, mark it done there.
- A design decision that a reader of the code needs belongs next to the code, in its module-level
doc comment (`//!`), not in a separate document.
- Always clean up REVIEW.md when you complete a task from it.

# Rust

## TUI

- Prefer Stylize helpers: use "text".dim(), .bold(), .cyan(), .italic(), .underlined() instead of manual Style where possible.
- Prefer simple conversions: use "text".into() for spans and vec![…].into() for lines; when inference is ambiguous (e.g., Paragraph::new/Cell::from), use Line::from(spans) or Span::from(text).
- Computed styles: if the Style is computed at runtime, using `Span::styled` is OK (`Span::from(text).set_style(style)` is also acceptable).
- Avoid hardcoded white: do not use `.white()`; prefer the default foreground (no color).
- Chaining: combine helpers by chaining for readability (e.g., url.cyan().underlined()).
- Single items: prefer "text".into(); use Line::from(text) or Span::from(text) only when the target type isn’t obvious from context, or when using .into() would require extra type annotations.
- Building lines: use vec![…].into() to construct a Line when the target type is obvious and no extra type annotations are needed; otherwise use Line::from(vec![…]).
- Avoid churn: don’t refactor between equivalent forms (Span::styled ↔ set_style, Line::from ↔ .into()) without a clear readability or functional gain; follow file‑local conventions and do not introduce type annotations solely to satisfy .into().
- Compactness: prefer the form that stays on one line after rustfmt; if only one of Line::from(vec![…]) or vec![…].into() avoids wrapping, choose that. If both wrap, pick the one with fewer wrapped lines.
