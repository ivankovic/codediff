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

use crate::code::Language;

/// Returns the language for a given path.
pub fn language_for_path(path: &std::path::Path) -> Option<Language> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if ext == "test" {
        // Fixtures are named `before.py.test`; opening one directly still detects Python.
        return language_for_path(std::path::Path::new(path.file_stem()?));
    }
    language_for_extension(ext.as_str())
}

/// Refines [`language_for_path`]'s extension-derived guess by peeking at `content`, for the one
/// collision where a content check is cheap, unambiguous, and worth it: `.ts` is TypeScript in the
/// overwhelming majority of cases, but Qt Linguist also uses `.ts` for its XML translation-source
/// files (`<?xml version="1.0"?><!DOCTYPE TS>...`) - a real, recurring collision (any Qt-based
/// project), not a one-off. Unlike a fuzzy content heuristic, this one is safe to apply
/// unconditionally: a file starting with an XML declaration cannot also be valid TypeScript, so
/// there is no genuine TypeScript source this could misclassify.
///
/// Every other extension collision this project has hit in practice (e.g. `.r` used by one
/// project's C runtime files instead of R) has been a one-off, single-repository convention rather
/// than a recurring pattern, so it's handled by rejecting that one corpus sample rather than by a
/// general content heuristic here - see `sample.csv`'s REJECTED rows and the "language detection"
/// discussion this function came out of.
///
/// Callers that already have the file's content in hand (e.g. [`crate::code::Code::from_file`],
/// which reads the whole file before determining its language anyway) should prefer this over
/// [`language_for_path`] - it can only ever be as good or better, never worse, and costs nothing
/// beyond a `starts_with` check. Callers that only have a path, or where reading content first would
/// add real I/O cost against a large corpus (e.g. `sample_test_diffs`'s commit-delta walk, which
/// checks extension before ever touching a blob), should keep using `language_for_path` alone.
pub fn language_for_path_and_content(path: &std::path::Path, content: &str) -> Option<Language> {
    let guess = language_for_path(path)?;
    if guess == Language::TypeScript && looks_like_xml(content) {
        return Some(Language::XML);
    }
    Some(guess)
}

/// True if `content` opens with an XML declaration, ignoring a leading UTF-8 BOM and whitespace.
/// Not narrowed to Qt Linguist's `<!DOCTYPE TS>`: any XML named `.ts` is not TypeScript.
fn looks_like_xml(content: &str) -> bool {
    content
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("<?xml")
}

/// Returns the best guess language for a given file extension.
///
/// Note that some extensions are not uniquely identifiable so the highest probability result is
/// returned. It may or may not be correct.
/// Every file extension CodeDiff recognises, lower-cased, and the language it means. The one
/// table behind [`language_for_extension`] and the README's language list.
pub const EXTENSIONS: &[(&[&str], Language)] = &[
    (&["bash", "sh"], Language::ShellScript),
    (&["bazel"], Language::Bazel),
    (&["c", "h"], Language::C),
    (&["cc", "cpp", "cxx", "hpp", "hh", "hxx"], Language::CPP),
    (&["cs"], Language::CSharp),
    (&["css", "scss"], Language::CSS),
    (&["dart"], Language::Dart),
    (&["el"], Language::Lisp),
    (&["go"], Language::Go),
    (&["html", "htm"], Language::HTML),
    (&["java"], Language::Java),
    (&["js", "mjs", "cjs", "jsx"], Language::JavaScript),
    (&["json"], Language::JSON),
    (&["kt"], Language::Kotlin),
    (&["lua"], Language::LUA),
    (&["md", "markdown"], Language::MarkDown),
    (&["php"], Language::PHP),
    (&["proto"], Language::ProtoBuf),
    (&["py", "pyi", "pyw"], Language::Python),
    (&["r"], Language::R),
    (&["rb"], Language::Ruby),
    (&["rs"], Language::Rust),
    (&["scala"], Language::Scala),
    (&["sql"], Language::SQL),
    (&["swift"], Language::Swift),
    (&["ts", "mts", "cts"], Language::TypeScript),
    (&["tsx"], Language::TSX),
    (&["vim"], Language::Vimscript),
    (&["yaml", "yml"], Language::YAML),
    (&["xml", "xht", "xhtml"], Language::XML),
];

pub fn language_for_extension(ext: &str) -> Option<Language> {
    EXTENSIONS
        .iter()
        .find(|(extensions, _)| extensions.contains(&ext))
        .map(|(_, language)| *language)
}

/// The language's name as a person writes it, where the variant name is not that.
pub fn human_name(language: Language) -> String {
    match language {
        Language::CPP => "C++".into(),
        Language::CSharp => "C#".into(),
        Language::LUA => "Lua".into(),
        Language::MarkDown => "Markdown".into(),
        Language::ProtoBuf => "Protocol Buffers".into(),
        Language::ShellScript => "Shell (Bash)".into(),
        Language::Lisp => "Emacs Lisp".into(),
        other => other.to_string(),
    }
}

/// The README's "Supported languages" section, generated so it cannot drift from this file: one
/// table of the languages with a grammar, and the extensions that are recognised but diffed as
/// plain text because no grammar is compiled in.
pub fn supported_languages_markdown() -> String {
    use strum::IntoEnumIterator;

    let extensions_of = |language: Language| -> String {
        EXTENSIONS
            .iter()
            .filter(|(_, candidate)| *candidate == language)
            .flat_map(|(extensions, _)| extensions.iter())
            .map(|extension| format!("`.{extension}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut with_grammar: Vec<Language> = Vec::new();
    let mut plain_text: Vec<Language> = Vec::new();
    for language in Language::iter().filter(|language| *language != Language::Unknown) {
        if to_treesitter(&language).is_some() {
            with_grammar.push(language);
        } else {
            plain_text.push(language);
        }
    }
    with_grammar.sort_by_key(|language| human_name(*language).to_lowercase());
    plain_text.sort_by_key(|language| human_name(*language).to_lowercase());

    let mut out = String::new();
    out.push_str(&format!(
        "{} languages are parsed with a tree-sitter grammar and diffed structurally:\n\n",
        with_grammar.len()
    ));
    out.push_str("| Language | File extensions |\n|---|---|\n");
    for language in &with_grammar {
        out.push_str(&format!(
            "| {} | {} |\n",
            human_name(*language),
            extensions_of(*language)
        ));
    }
    out.push_str(
        "\nRecognised by extension but diffed as plain text, since no grammar is compiled in: ",
    );
    out.push_str(
        &plain_text
            .iter()
            .map(|language| format!("{} ({})", human_name(*language), extensions_of(*language)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(".\n");
    out
}

pub fn to_treesitter(language: &Language) -> Option<tree_sitter::Language> {
    match language {
        // alphabetically sorted
        Language::C => Some(tree_sitter_c::LANGUAGE.into()),
        Language::CPP => Some(tree_sitter_cpp::LANGUAGE.into()),
        Language::CSS => Some(tree_sitter_css::LANGUAGE.into()),
        Language::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
        Language::HTML => Some(tree_sitter_html::LANGUAGE.into()),
        Language::JSON => Some(tree_sitter_json::LANGUAGE.into()),
        Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
        Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
        Language::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        Language::LUA => Some(tree_sitter_lua::LANGUAGE.into()),
        Language::PHP => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
        Language::R => Some(tree_sitter_r::LANGUAGE.into()),
        Language::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
        Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        Language::Scala => Some(tree_sitter_scala::LANGUAGE.into()),
        Language::ShellScript => Some(tree_sitter_bash::LANGUAGE.into()),
        Language::Swift => Some(tree_sitter_swift::LANGUAGE.into()),
        Language::TSX => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        Language::Vimscript => Some(tree_sitter_vim::language()),
        Language::YAML => Some(tree_sitter_yaml::LANGUAGE.into()),
        Language::XML => Some(tree_sitter_xml::LANGUAGE_XML.into()),
        // Bazel, Dart, MarkDown, SQL, ProtoBuf and Lisp are not supported at this time.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_for_invalid_extensions() {
        assert!(language_for_extension("").is_none());
        assert!(language_for_extension("invalid_extension_for_test").is_none());
    }

    #[test]
    fn language_for_valid_extensions() {
        // C and C++ share header file extensions. We default to C.
        assert_eq!(language_for_extension("h"), Some(Language::C));
        assert_eq!(language_for_extension("hpp"), Some(Language::CPP));
    }

    #[test]
    fn language_for_path_strips_test_suffix() {
        assert_eq!(
            language_for_path(std::path::Path::new("before.py.test")),
            Some(Language::Python)
        );
        assert_eq!(
            language_for_path(std::path::Path::new("foo.rs.test")),
            Some(Language::Rust)
        );
        // No inner extension to fall back to.
        assert_eq!(language_for_path(std::path::Path::new("foo.test")), None);
    }

    #[test]
    fn ts_extension_with_xml_content_is_qt_linguist_not_typescript() {
        let qt_linguist = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\">\n</TS>\n";
        assert_eq!(
            language_for_path_and_content(
                std::path::Path::new("translations/app_de.ts"),
                qt_linguist
            ),
            Some(Language::XML)
        );
        // A leading UTF-8 BOM or whitespace before the declaration shouldn't defeat the sniff.
        assert_eq!(
            language_for_path_and_content(
                std::path::Path::new("app.ts"),
                "\u{feff}  \n<?xml version=\"1.0\"?><TS></TS>"
            ),
            Some(Language::XML)
        );
    }

    #[test]
    fn ts_extension_with_ts_content_is_still_typescript() {
        assert_eq!(
            language_for_path_and_content(
                std::path::Path::new("app.ts"),
                "export function greet(name: string): string {\n    return `hi ${name}`;\n}\n"
            ),
            Some(Language::TypeScript)
        );
        assert_eq!(
            language_for_path_and_content(std::path::Path::new("app.ts"), ""),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn xml_content_sniff_is_scoped_to_ts_extension() {
        // Qt Linguist only uses `.ts`, so the sniff must not touch `.tsx`.
        assert_eq!(
            language_for_path_and_content(
                std::path::Path::new("app.tsx"),
                "<?xml version=\"1.0\"?>"
            ),
            Some(Language::TSX)
        );
        assert_eq!(
            language_for_path_and_content(
                std::path::Path::new("app.rs"),
                "<?xml version=\"1.0\"?>"
            ),
            Some(Language::Rust)
        );
    }

    /// The README's language section is this file's output between two HTML comment markers, so
    /// adding a grammar or an extension without updating it fails here, with the text to paste.
    #[test]
    fn the_readme_lists_exactly_the_supported_languages() {
        let readme = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"),
        )
        .expect("README.md at the crate root")
        // git's Windows autocrlf default checks the README out with CRLF.
        .replace("\r\n", "\n");
        let start = "<!-- languages:start -->\n";
        let end = "<!-- languages:end -->";
        let from = readme
            .find(start)
            .expect("README has the languages:start marker")
            + start.len();
        let to = readme[from..]
            .find(end)
            .expect("README has the languages:end marker")
            + from;
        let expected = supported_languages_markdown();
        assert_eq!(
            readme[from..to].trim_end(),
            expected.trim_end(),
            "README.md's language section is stale; paste this between the markers:\n\n{expected}"
        );
    }

    #[test]
    fn every_extension_maps_to_exactly_one_language() {
        let mut seen = std::collections::HashSet::new();
        for (extensions, _) in EXTENSIONS {
            for extension in *extensions {
                assert!(
                    seen.insert(*extension),
                    "extension {extension} is listed twice"
                );
                assert_eq!(*extension, extension.to_ascii_lowercase(), "{extension}");
            }
        }
    }
}
