/*  Throwaway diagnostic (2026-08-18): how much source text is owned by *internal* nodes - i.e.
 *  covered by no child node - across the whole corpus, per language and kind. Run with
 *  `cargo test --features test-fixtures -- --ignored gap_survey`.
 */

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::collections::HashMap;
    use std::io::Write;

    const REPORT: &str = "/tmp/claude-1000/-home-m-src-codediff/ca04cc2a-f0ca-46fc-b936-d5056dc2edd3/scratchpad/gap_survey.txt";

    #[test]
    #[ignore = "diagnostic"]
    fn gap_survey() -> Result<()> {
        let pairs = crate::test::helper::handmade_test_code_pairs()?;
        // (language, kind) -> (nodes owning non-whitespace gap text, total bytes so owned)
        let mut owners: HashMap<(String, String), (usize, usize)> = HashMap::new();
        let mut per_language: HashMap<String, (usize, usize, usize)> = HashMap::new();

        for (before, after) in pairs.values() {
            for code in [before, after] {
                let Some(ast) = code.ast.as_ref() else {
                    continue;
                };
                let language = format!("{:?}", code.metadata.language.unwrap_or_default());
                let source = code.contents.as_bytes();
                let mut cursor = ast.root_node().walk();
                let mut stack = vec![ast.root_node()];
                while let Some(node) = stack.pop() {
                    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
                    stack.extend(children.iter().copied());

                    let entry = per_language.entry(language.clone()).or_default();
                    entry.0 += 1; // nodes
                    if children.is_empty() {
                        entry.1 += 1; // leaves
                        continue;
                    }

                    let mut owned = 0usize;
                    let mut gap_start = node.start_byte();
                    for child in &children {
                        owned += content_len(source, gap_start, child.start_byte());
                        gap_start = child.end_byte();
                    }
                    owned += content_len(source, gap_start, node.end_byte());
                    if owned > 0 {
                        entry.2 += 1; // internal nodes owning real text
                        let slot = owners
                            .entry((language.clone(), node.kind().to_string()))
                            .or_default();
                        slot.0 += 1;
                        slot.1 += owned;
                    }
                }
            }
        }

        let mut out = String::new();
        out.push_str("language                nodes    leaves  internal-owning-text\n");
        let mut langs: Vec<_> = per_language.into_iter().collect();
        langs.sort_by_key(|(_, v)| std::cmp::Reverse(v.2));
        for (language, (nodes, leaves, owning)) in langs {
            out.push_str(&format!(
                "{language:<20} {nodes:>8} {leaves:>9} {owning:>10}\n"
            ));
        }

        out.push_str("\ntop (language, kind) by count of internal nodes owning text\n");
        let mut kinds: Vec<_> = owners.into_iter().collect();
        kinds.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
        for ((language, kind), (count, bytes)) in kinds.iter().take(40) {
            out.push_str(&format!(
                "{language:<14} {kind:<32} {count:>7} nodes {bytes:>9} bytes\n"
            ));
        }

        std::fs::File::create(REPORT)?.write_all(out.as_bytes())?;
        Ok(())
    }

    /// Bytes in `source[start..end]` that are real content rather than formatting - the same
    /// "empty or entirely whitespace" test `hash::compute_owned_text_hash` applies.
    fn content_len(source: &[u8], start: usize, end: usize) -> usize {
        if start >= end {
            return 0;
        }
        match std::str::from_utf8(&source[start..end]) {
            Ok(text) if !text.trim().is_empty() => text.len(),
            _ => 0,
        }
    }
}
