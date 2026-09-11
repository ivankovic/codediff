# Rules and preferences

This document lists what I learned by doing hundreds of diffs by hand on what a good diff looks
like.

The rules and preferences are in no particular order. The rules are what I believe 99% of the
developers would choose as optimal. Preferences are multiple possible equally correct options to
show the same diff.

## Rules

These are optimal 99.99% of the time. In no particular order.

### Never show end of line whitespace

Modern editors, linters and formatters automatically trim end-of-line whitespace. It is visual noise
to highlight them in the diff.

CodeDiff will never highlight them.

### Delimiters follow the things they are delimiting, if possible

Consider the following code:

```Rust
fn test() {
    let x = !vec[
        "One",
        "Two",
    ]
}
```

and change it to:

```Rust
fn test() {
    let x = !vec[
        "One",
        "THIS LINE WAS ADDED",
        "Two",
    ]
}
```

From the perspective of a pure tree-edit distance, it is not relevant which comma matches an
existing comma and which comma is marked as inserted. However, a human would always prefer that the
comma immediately **after** the modified line is also marked as modified.

Similarly, if a repeated comment, e.g. ```// THIS CODE WAS AUTO-GENERATED``` is present in the file,
or there is a repeated annotation, e.g. ```[#cfg(test)]```, the human always prefers the comment or
annotation that is immediately **before** the modified line to be marked as modified.

These can come in conflict. Here is a real example, the following line:

```Ruby
return size if size == nil || size == Float::INFINITY || size == 0
```

changed into:

```Ruby
return size if size == 0 || size == nil || size == Float::INFINITY || size == -Float::INFINITY
```

The question now is how should the three "||" in the new code map to the two "||" in the old code?
One that is clear is the one between nil and positive Float::INFINITY check. Those two are still in
the same order. However, the one following positive Float::INFINITY used to be before 0, but is now
before negative INFINITY, so that one could be consisdered added. On the other hand, 0 was not
followed by any, and is now followed by one, so that one could be consisdered added.

In such cases, the realistic answer is that it is actually irrelevant to a human being. They would
not care either way. So a consistent mapping is simply to map any irrelevant mappings in order of
apperance. With this rule, the final "||" would be missing a match and would be considered added.

### Anchoring an ambiguous edit inside a value

Inside a node whose value we examine character by character - an identifier, a string constant, a
comment - an added or deleted run can sometimes sit in more than one place and produce exactly the
same text. Deleting the first `y-` or the second from `y-y-foo` leaves `y-foo` either way. In such
cases the edit is anchored **left**: the leftmost run of bytes that produces the intended result is
the one that gets painted.

This applies only *inside* such a node. Ambiguity outside one - a comma that follows a newly added
identifier, one of two identical `::` in a path - is structural, the AST maps it, and this rule has
nothing to say about it.

Two caveats, both learned from measuring the corpus (see TODO.md for the numbers):

-   The rule is subordinate to token boundaries. Inserting `,"k":"v"` into JSON-inside-a-comment
    can be slid one byte left to `","k":"v` - the same length, the same resulting text, and a
    split string literal. A leftmost run that cuts a token in half is not the intended reading, and
    a human will not pick it.
-   CodeDiff does not currently implement the rule. `intra_node_update_ranges` takes the longest
    common prefix first and then the suffix of what remains, which makes it right-anchored by
    construction. Flipping it was measured and rejected: it would agree with 4 more fixtures in the
    corpus and disagree with 12 more.

## Preferences

These always present a choice between two or more equally correct ways to show the same diff. The
developers individual preferences will dictate what they find better. In some cases, developers
could have preferences that change from one file to the other. We support that by making switching
between options quick and easy in the TUI, but in batch mode we do ask the developers to make a
choice once for the entire batch.

You can choose your prefered options in the config panel.

In no particular order.

### Indentation

There are four ways to highlight the following code, with respect to indentation:

```Python
    def added_function():
        print("This code was added")
```

1.  Highlight only the visible characters as added.
2.  Highlight the visible characters and any whitespace **in the same line** between them as added.
3.  Highlight the visible characters and whitespace between **in all lines** between them as added.
4.  Highlight the entire block, including the whitespace preceeding `def` as added

Of these, CodeDiff does **not** support the first choice (no whitespace highlighted). It leads to
ambiguity with string constants that I believe only a tiny minority would find acceptable. The
remaining three options are all acceptable and subjective preference.

Obviously, the same applies if the code block was deleted. If the code block was updated, similar
options exist, but the inserted or deleted whitespace should always be highlighted at the start of
the line, and existing matching whitespace should be marked as matching between that and the first
character.

### Identifier updates

Consider the following change:

```Python
some_function(argument)
```

to

```Python
some_function(i_am_an_argument)
```

The user might prefer that only the `i_am_an_` part of the code is highlighted as added, or they
might prefer that the entire pair (`argument`, `i_am_an_argument`) is marked as a matched updated
region.
