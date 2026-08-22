# Human Preferences

This document lists what I learned by doing hundreds of diffs by hand on what a good diff looks
like.

The preferences are in no particular order, and some might be subjective. For the subjective rules,
it is more important that the tool is consistent than any cost paid by preference mismatch.

## Delimiters follow the things they are delimiting, if possible

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

## Node kinds can sometimes be updated, not just values

The TreeSitter grammars are not designed with abstract edit distance scripts in mind. They are messy
and don't consistently use node kinds and node values. Most tree edit distance algorithms do not
allow the edit script to update node kinds, only values.

As a general rule, *the algorithm should never match intermediate nodes of different kinds that do
not contain user-visible values*. E.g., it is not okay to match a `binary_expression` node to
a `unary_expression` node, even though they are both expressions and they both have some operator as
a child. It could, however, be okay to match a less-than `<` node to a less-than-or-equals `<=` node,
even if the grammar for the particular language has nodes of different kinds for the two operators.
