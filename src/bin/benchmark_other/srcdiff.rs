/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

use anyhow::{Context, Result, bail};
use codediff::code::{Code, Language};
use codediff::diff::text_range::TextRange;
use quick_xml::events::Event;
use std::process::Command;

use super::{char_offset_table, external_tool_bin, span_from_char_offsets, write_temp_pair};

/// Path to the `srcdiff` binary, from `SRCDIFF_BIN`. Build with `make -C research install-srcdiff`.
pub(crate) fn srcdiff_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "SRCDIFF_BIN",
        "point it at a built `srcdiff` binary (make -C research install-srcdiff)",
    )
}

/// `(-l value, file extension)` for `language`: the languages srcML marks up (`srcml --version`).
/// `None` everywhere else; srcDiff has no fallback parser. Objective-C is left out because codediff
/// has no Objective-C language to map it from.
pub(crate) fn srcdiff_language(language: Language) -> Option<(&'static str, &'static str)> {
    match language {
        Language::C => Some(("C", "c")),
        Language::CPP => Some(("C++", "cpp")),
        Language::CSharp => Some(("C#", "cs")),
        Language::Java => Some(("Java", "java")),
        Language::Python => Some(("Python", "py")),
        _ => None,
    }
}

/// srcDiff's XML for one pair. `-t UTF-8` because the default source encoding is ISO-8859-1,
/// which would turn every non-ASCII character into two and shift every offset after it.
fn srcdiff_xml(before: &Code, after: &Code) -> Result<String> {
    let language = before.metadata.language.unwrap_or_default();
    let (srcml_language, ext) = srcdiff_language(language)
        .with_context(|| format!("no srcML language for {language:?}"))?;
    let srcdiff = srcdiff_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&srcdiff)
        .args(["-t", "UTF-8", "-l", srcml_language])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {srcdiff:?} -l {srcml_language}"))?;
    if !output.status.success() {
        bail!(
            "srcdiff exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).context("srcdiff output is not UTF-8")
}

/// Which side(s) a stretch of srcDiff text belongs to: the innermost enclosing `diff:common`,
/// `diff:delete` or `diff:insert`, and `Common` outside all three.
/// Half-open character ranges, one list per side: `(before, after)`.
type Regions = (Vec<(usize, usize)>, Vec<(usize, usize)>);

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Common,
    Before,
    After,
}

/// Each side's changed regions from srcDiff's XML, as half-open *character* ranges.
///
/// srcDiff writes one merged srcML document: text under `diff:delete` exists only in the original,
/// under `diff:insert` only in the modified file, and everywhere else in both. `diff:ws` only
/// flags whitespace and moves are a `move` attribute on a delete/insert pair, so neither changes
/// which side text is on. A region is a maximal run of one side's changed text with its leading
/// and trailing whitespace trimmed, so a re-indented line is not a change - the same reading the
/// AST-aware tools give. Reassembles both files as it goes and errors unless they match `before`
/// and `after` exactly, so a markup this walk does not understand cannot shift offsets silently -
/// up to what srcML drops on reading, a leading byte-order mark and the `\r` of every `\r\n`
/// ([`srcml_reading`]). Offsets are mapped back to the file's own.
pub(crate) fn srcdiff_regions(before: &str, after: &str, xml: &str) -> Result<Regions> {
    let mut reader = quick_xml::Reader::from_str(xml);
    // `Some` for the `diff:` elements that set a side; every start tag pushes, every end tag pops.
    let mut stack: Vec<Option<Side>> = Vec::new();
    let mut texts = [String::new(), String::new()];
    let mut regions: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
    // Per side, the start (in chars) of the changed run in progress.
    let mut open: [Option<usize>; 2] = [None, None];
    let mut lengths = [0usize; 2];

    let mut push = |text: &str, side: Side| {
        for (index, is_side) in [(0, Side::Before), (1, Side::After)] {
            if side != Side::Common && side != is_side {
                continue;
            }
            let chars = text.chars().count();
            if side == Side::Common {
                if let Some(start) = open[index].take() {
                    regions[index].push((start, lengths[index]));
                }
            } else if open[index].is_none() {
                open[index] = Some(lengths[index]);
            }
            texts[index].push_str(text);
            lengths[index] += chars;
        }
    };
    let current =
        |stack: &[Option<Side>]| stack.iter().rev().find_map(|s| *s).unwrap_or(Side::Common);

    loop {
        match reader.read_event().context("parsing srcdiff XML")? {
            Event::Start(e) => stack.push(match e.name().as_ref() {
                "diff:common" => Some(Side::Common),
                "diff:delete" => Some(Side::Before),
                "diff:insert" => Some(Side::After),
                _ => None,
            }),
            Event::End(_) => {
                stack.pop();
            }
            // srcML writes a character XML cannot carry as `<escape char="0xc"/>`.
            Event::Empty(e) if e.name().as_ref() == "escape" => {
                let attribute = e
                    .try_get_attribute("char")?
                    .context("srcML <escape> without a `char` attribute")?;
                let value = &attribute.value;
                let code = u32::from_str_radix(value.trim_start_matches("0x"), 16)
                    .with_context(|| format!("srcML <escape char={value:?}>"))?;
                let ch = char::from_u32(code).context("srcML <escape> is not a character")?;
                push(ch.encode_utf8(&mut [0; 4]), current(&stack));
            }
            // Outside the root element is only the newline after the XML declaration.
            Event::Text(_) if stack.is_empty() => {}
            // Raw text, not `xml_content()`: that normalizes `\r\n`, and offsets must count the
            // file's own characters.
            Event::Text(e) => push(&e, current(&stack)),
            Event::GeneralRef(e) => {
                let ch = match e.resolve_char_ref()? {
                    Some(ch) => ch,
                    None => match &*e {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        other => bail!("unknown entity &{other};"),
                    },
                };
                push(ch.encode_utf8(&mut [0; 4]), current(&stack));
            }
            Event::CData(e) => push(&e, current(&stack)),
            Event::Eof => break,
            _ => {}
        }
    }
    for index in 0..2 {
        if let Some(start) = open[index].take() {
            regions[index].push((start, lengths[index]));
        }
    }

    for (index, (name, file)) in [("before", before), ("after", after)].iter().enumerate() {
        let (expected, _) = srcml_reading(file);
        if texts[index] != expected {
            let at = texts[index]
                .chars()
                .zip(expected.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let near = |s: &str| {
                s.chars()
                    .skip(at.saturating_sub(10))
                    .take(20)
                    .collect::<String>()
            };
            bail!(
                "srcdiff's {name} side does not reassemble to the input file: first difference at \
                 character {at}, {:?} where the file has {:?}",
                near(&texts[index]),
                near(&expected)
            );
        }
    }

    let [before_regions, after_regions] = regions;
    Ok((
        trim_regions(before, to_file_offsets(before, before_regions)),
        trim_regions(after, to_file_offsets(after, after_regions)),
    ))
}

/// `contents` as srcML reads it - without a leading byte-order mark or the `\r` of any `\r\n` - and,
/// for each of its characters plus one past the end, that character's index in `contents`.
fn srcml_reading(contents: &str) -> (String, Vec<usize>) {
    let chars: Vec<char> = contents.chars().collect();
    let kept: Vec<usize> = (0..chars.len())
        .filter(|&i| {
            let byte_order_mark = i == 0 && chars[i] == '\u{feff}';
            let crlf_cr = chars[i] == '\r' && chars.get(i + 1) == Some(&'\n');
            !byte_order_mark && !crlf_cr
        })
        .collect();
    let text = kept.iter().map(|&i| chars[i]).collect();
    let mut offsets = kept;
    offsets.push(chars.len());
    (text, offsets)
}

/// Maps `[start, end)` character ranges over [`srcml_reading`]'s text back onto `contents`. A range
/// ending at a line break then takes in its `\r`, which trimming drops.
fn to_file_offsets(contents: &str, regions: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let (_, offsets) = srcml_reading(contents);
    regions
        .into_iter()
        .map(|(start, end)| (offsets[start], offsets[end]))
        .collect()
}

/// Shrinks each `[start, end)` character range past leading and trailing whitespace, dropping
/// ranges that were whitespace only.
fn trim_regions(contents: &str, regions: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let chars: Vec<char> = contents.chars().collect();
    regions
        .into_iter()
        .filter_map(|(mut start, mut end)| {
            while start < end && chars[start].is_whitespace() {
                start += 1;
            }
            while end > start && chars[end - 1].is_whitespace() {
                end -= 1;
            }
            (start < end).then_some((start, end))
        })
        .collect()
}

/// Per-line touched flags: every line a changed region reaches.
pub(crate) fn srcdiff_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let xml = srcdiff_xml(before, after)?;
    let (before_regions, after_regions) = srcdiff_regions(&before.contents, &after.contents, &xml)?;
    Ok((
        touched_lines(&before.contents, &before_regions),
        touched_lines(&after.contents, &after_regions),
    ))
}

fn touched_lines(contents: &str, regions: &[(usize, usize)]) -> Vec<bool> {
    let table = char_offset_table(contents);
    let mut touched = vec![false; contents.split('\n').count()];
    for &(start, end) in regions {
        let span = span_from_char_offsets(&table, start, end);
        for slot in &mut touched[span.start_row..=span.end_row] {
            *slot = true;
        }
    }
    touched
}

/// srcDiff's changed regions as spans, converted from character offsets.
pub(crate) fn srcdiff_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let xml = srcdiff_xml(before, after)?;
    let (before_regions, after_regions) = srcdiff_regions(&before.contents, &after.contents, &xml)?;
    let spans = |contents: &str, regions: &[(usize, usize)]| {
        let table = char_offset_table(contents);
        regions
            .iter()
            .map(|&(start, end)| span_from_char_offsets(&table, start, end))
            .collect()
    };
    Ok((
        spans(&before.contents, &before_regions),
        spans(&after.contents, &after_regions),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str =
        r#"xmlns="http://www.srcML.org/srcML/src" xmlns:diff="http://www.srcML.org/srcDiff""#;

    #[test]
    fn srcdiff_regions_reads_sides_and_trims_whitespace() {
        let before = "a = 1;\nb = 2;\n";
        let after = "a = 1;\n  c = 3;\nb = 4;\n";
        let xml = format!(
            "<unit {NS}><expr_stmt>a = 1;</expr_stmt>\n<diff:insert><diff:ws>  </diff:ws>\
             <expr_stmt>c = 3;</expr_stmt><diff:ws>\n</diff:ws></diff:insert>\
             <expr_stmt>b = <diff:delete>2</diff:delete><diff:insert>4</diff:insert>;</expr_stmt>\n</unit>"
        );
        let (b, a) = srcdiff_regions(before, after, &xml).unwrap();
        assert_eq!(b, vec![(11, 12)]);
        assert_eq!(a, vec![(9, 15), (20, 21)]);
    }

    #[test]
    fn srcdiff_regions_resolves_entities_and_escapes_and_skips_text_outside_the_root() {
        let before = "x < y\u{c}\n";
        let after = "x < y\u{c}\n";
        let xml = format!(
            "<?xml version=\"1.0\"?>\n<unit {NS}>x &lt; y<escape char=\"0xc\"/>\n</unit>\n"
        );
        let (b, a) = srcdiff_regions(before, after, &xml).unwrap();
        assert!(b.is_empty() && a.is_empty());
    }

    #[test]
    fn srcdiff_regions_rejects_output_that_does_not_reassemble() {
        let xml = format!("<unit {NS}>a<diff:delete>b</diff:delete></unit>");
        assert!(srcdiff_regions("ab", "a", &xml).is_ok());
        assert!(srcdiff_regions("abc", "a", &xml).is_err());
    }

    #[test]
    fn srcdiff_regions_keeps_common_inside_delete_on_both_sides() {
        let xml = format!(
            "<unit {NS}><diff:delete>x(<diff:common>y</diff:common>)</diff:delete>y</unit>"
        );
        // Not a shape srcDiff is known to emit with text on both sides, but the stack must not
        // leak the delete past the common.
        let (b, a) = srcdiff_regions("x(y)y", "yy", &xml).unwrap();
        assert_eq!(b, vec![(0, 2), (3, 4)]);
        assert!(a.is_empty());
    }

    #[test]
    fn srcdiff_regions_maps_offsets_back_over_crlf() {
        let before = "a;\r\nb;\r\n";
        let after = "a;\r\nc;\r\n";
        let xml = format!(
            "<unit {NS}>a;\n<diff:delete>b;\n</diff:delete><diff:insert>c;\n</diff:insert></unit>"
        );
        let (b, a) = srcdiff_regions(before, after, &xml).unwrap();
        assert_eq!(b, vec![(4, 6)]);
        assert_eq!(a, vec![(4, 6)]);
    }

    #[test]
    fn srcdiff_regions_maps_offsets_back_over_a_byte_order_mark() {
        let xml = format!("<unit {NS}>a<diff:delete>b</diff:delete></unit>");
        let (b, a) = srcdiff_regions("\u{feff}ab", "\u{feff}a", &xml).unwrap();
        assert_eq!(b, vec![(2, 3)]);
        assert!(a.is_empty());
    }

    #[test]
    fn touched_lines_marks_every_line_a_region_spans() {
        assert_eq!(
            touched_lines("a\nb\nc\nd", &[(2, 5)]),
            vec![false, true, true, false]
        );
    }
}
