# What a VS Code extension needs

Short answer: **the codediff side is already done.** `codediff --mode json BEFORE AFTER` emits
everything an extension needs, and this commit's man page / completions work adds nothing it
depends on. What is missing is the extension itself, which belongs in its own repository —
`codediff-vscode`, mirroring how [codediff.nvim](https://github.com/ivankovic/codediff.nvim) is
kept separate.

## What codediff already provides

```jsonc
{
  "before": { "path": "old.rs", "language": "Rust", "hunks": [ /* ranges in the before file */ ] },
  "after":  { "path": "new.rs", "language": "Rust", "hunks": [ /* ranges in the after file  */ ] },
  "large_residual": false,
  "summary": "comment_only"
}
```

Each hunk carries `operation` (`insert`/`delete`/`update`/`move`), a `range`, an optional
`move_target` (the real counterpart range in the other file — set for `move` only, and deliberately
omitted elsewhere rather than filled with a synthetic anchor), and an optional `reference_line`
(the nearest enclosing declaration, the `@` breadcrumb headless mode prints). That is a complete
basis for decoration: four colours, a "moved from/to line N" affordance, and a gutter breadcrumb.

Files with no tree-sitter grammar fall back to a plain line diff; binary files answer with
`{"binary": true}` and empty hunks. Neither case needs special handling in the extension beyond
not crashing on an empty `hunks` list.

## The one thing that will silently break it

**Columns in the JSON are byte offsets. VS Code's `Position.character` is UTF-16 code units.**

Measured, not assumed — for the line `x = "ααα" + bbb`, codediff reports `start_column: 15` where
VS Code needs `12`. Every range on every line containing a non-ASCII character lands in the wrong
place, and nothing about the failure points at the cause.

This is documented at the top of `src/tui/json_output.rs`, and it is not going to change: byte
columns are what tree-sitter reports and what Neovim consumes directly, so the conversion belongs
in the extension. Per line:

```ts
const toUtf16 = (line: string, byteColumn: number): number =>
  Buffer.from(line, "utf8").subarray(0, byteColumn).toString("utf8").length;
```

Do this once per hunk endpoint against the document's own line text. Getting it right on day one
costs five lines; finding it later costs a bug report from somebody diffing Cyrillic or CJK.

## Design decisions to make before writing code

**1. Where the diff is rendered.** Three options, in increasing order of effort:

- *Decorate VS Code's native diff editor.* Both sides appear in `vscode.window.visibleTextEditors`,
  so `TextEditorDecorationType` ranges can be layered over the built-in line diff. Users keep every
  native affordance (staging, inline editing, navigation) and gain codediff's structural verdicts
  on top. **Recommended starting point** — smallest surface, largest share of the value.
- *A `TextDocumentContentProvider` virtual document*, opened side by side and decorated. Full
  control over what is shown; loses the native diff editor's features.
- *A webview* rendering codediff's own output. Complete control, and the only route to reproducing
  the TUI's exact look — but it reimplements a diff viewer, including scroll sync and theming.

**2. How the binary is acquired.** `codediff` on `PATH` is the simplest contract and the one the
Neovim plugin already uses. The alternative — bundling per-platform binaries via
`vsce package --target linux-x64` and friends — means five VSIX artifacts of ~37 MB each, since
every tree-sitter grammar is statically linked. Recommendation: require it on `PATH`, detect its
absence, and offer a command that downloads the matching release asset. Shelling out also keeps the
extension a separate work from AGPL-licensed codediff, which is the cleaner licensing story; AGPL
itself is no obstacle to publishing on the Marketplace either way.

**3. Diffing against git revisions.** VS Code's SCM diffs use `git:` URIs, not files on disk, and
`--mode json` takes two real paths. The extension has to materialize both sides into temp files
first. Not hard, but it is the difference between "works on two open files" and "works where people
actually read diffs".

## Publishing

- `vsce publish` → Visual Studio Marketplace (needs an Azure DevOps publisher and a PAT).
- `ovsx publish` → Open VSX, which is what VSCodium, Cursor and Windsurf install from. Skipping it
  cuts out a real share of users for one extra CI step.
- Both belong in a release workflow in the extension's own repo, not this one.

## What would need to change here

Nothing, to build a working extension. Two things worth considering only if the extension surfaces
a need for them:

- `--mode json` reads two paths. If materializing temp files proves awkward, a `--stdin` pair would
  remove that step — but it is not a blocker, and no other integration has asked for it.
- The JSON has no schema version field. Worth adding before a second consumer exists, so the two
  can disagree about the schema explicitly rather than by crashing.
