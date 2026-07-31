# Related works: other code-diffing tools and projects

A survey of tools and papers in the same space as CodeDiff, gathered while writing
`research/papers/introductory-paper/`. Web search results, current as of 2026-07-31. This file
tracks a fast-moving landscape, so re-check before citing anything here as current fact.

## Closest siblings: Rust, tree-sitter, CLI, git-integrated

These use the same core technology as CodeDiff (tree-sitter parsing, Rust CLI, `git diff`
backend), so they are CodeDiff's most direct competitors, even though neither computes a full
tree-edit-distance mapping the way CodeDiff, APTED, or GumTree do.

* **[difftastic](https://github.com/Wilfred/difftastic)** (Wilfred Hughes) - the most popular
  tool in this group. Parses with tree-sitter, converts the parse tree to an s-expression, then
  diffs structurally. Ignores whitespace that carries no syntactic meaning. Its diffing algorithm
  is its own design, closer to a structural Myers diff than to full tree-edit distance - it does
  not compute an APTED- or GumTree-style node-to-node mapping.
* **[diffsitter](https://github.com/afnanenayet/diffsitter)** (afnanenayet) - the same idea as
  difftastic: tree-sitter parsing, a semantic diff instead of a line diff, `git diff`
  integration. Smaller and less actively maintained than difftastic.

Both are now in `benchmark_other.rs`'s own head-to-head comparison against codediff, GumTree, and
Unix `diff` (`ExternalTool::Difftastic`/`ExternalTool::Diffsitter`, added 2026-07-31) - the only
two candidates from this whole survey that are complete, multi-language, installable binaries, as
opposed to a single-language academic tool or a bare algorithm library. On the 98-fixture
ground-truth corpus, both are less accurate than codediff, Unix `diff`, and GumTree at line-level
agreement with the human mapping - expected, since neither computes a full tree-edit-distance
mapping. diffsitter is the fastest AST-aware tool in the whole comparison (median 4.66ms/fixture);
difftastic sits between codediff and GumTree's warm-JVM number (median 54.40ms). See
`research/benchmark_other.csv` and `research/plots/benchmark_other_accuracy.png`. Installed with
`cargo install --root /var/tmp/codediff-tools difftastic diffsitter`, pointed at via `DIFFT_BIN`/
`DIFFSITTER_BIN` - see `CONTRIBUTING.md`'s `benchmark-other` entry.

## The GumTree family (mostly academic, Java-centric)

* **GumTree** itself, and **GumTree-Spoon**, a Java-specific version built on the Spoon AST
  framework.
* **ChangeDistiller**, **ChangeNodes**, **CLDiff**, **LAS**, **IJM** - Java-focused fine-grained
  change-extraction tools in the same research lineage as GumTree.
* A 2024 ACM TOSEM paper, ["A Novel Refactoring and Semantic-Aware AST Differencing
  Tool"](https://arxiv.org/abs/2403.05939) - built on RefactoringMiner. Adds a new benchmark of
  988 commits (800 bug-fix, 188 refactoring) and reports better precision and recall than
  GumTree, especially on refactoring commits.

## Tree-edit-distance algorithm libraries (the algorithm only, not a diff tool)

* Multiple APTED ports, for example `JoaoFelipe/apted` in Python.
* Multiple Zhang-Shasha ports, for example `timtadh/zhang-shasha` in Python and
  `blendmaster/tdiff` in JavaScript.
* `treediff-rs` - a generic Rust tree-diffing library, not specific to source code.

## Other approaches

* **[Diff/AST (diffast)](https://github.com/codinuum/diffast)** - a divide-and-conquer
  approximation of tree edit distance. Exports diffs as RDF/XML facts, not as a human-readable
  diff.
* **[BDiff](https://arxiv.org/pdf/2510.21094)** - block-aware and text-based, not AST-based, but
  aware of code structure. A very recent paper.
* **SDiff** - a hybrid line-based and AST-based approach.
* **truediff** - a more recent academic tree-differencing approach.

## A foundational paper worth adding to the introductory paper's Background section

Chawathe, Rajaraman, Garcia-Molina, and Widom, ["Change Detection in Hierarchically Structured
Information"](https://dl.acm.org/doi/10.1145/235968.233366) (SIGMOD 1996). This is the paper
GumTree's own top-down greedy-matching phase traces back to. It belongs next to Zhang-Shasha as
an ancestor of GumTree's design, not just GumTree itself, in a complete lineage.

## Sources

* [GumTree GitHub](https://github.com/GumTreeDiff/gumtree)
* [Wilfred/difftastic](https://github.com/Wilfred/difftastic)
* [afnanenayet/diffsitter](https://github.com/afnanenayet/diffsitter)
* [codinuum/diffast](https://github.com/codinuum/diffast)
* [A Novel Refactoring and Semantic Aware AST Differencing Tool (arXiv)](https://arxiv.org/abs/2403.05939)
* [BDiff (arXiv)](https://arxiv.org/pdf/2510.21094)
* [Martin Monperrus - Pointers on AST differencing tools](https://www.monperrus.net/martin/tree-differencing)
