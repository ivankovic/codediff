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

/**
* Returns the language for a given path.
*/
pub fn language_for_path(path: &std::path::Path) -> Option<Language> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if ext == "test" {
        // Test fixtures are named e.g. `before.py.test` so the test harness can strip the
        // `.test` suffix before treating them as real source; recognize the inner extension too,
        // so opening one directly (e.g. in the TUI) still detects its real language.
        return language_for_path(std::path::Path::new(path.file_stem()?));
    }
    language_for_extension(ext.as_str())
}

/**
* Returns the best guess language for a given file extension.
*
* Note that some extensions are not uniquely identifiable so the highest probability result is
* returned. It may or may not be correct.
*/
pub fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        // Sorted alphabetically.
        "bash" | "sh" => Some(Language::ShellScript),
        "bazel" => Some(Language::Bazel),
        "c" | "h" => Some(Language::C),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some(Language::CPP),
        "cs" => Some(Language::CSharp),
        "css" | "scss" => Some(Language::CSS),
        "dart" => Some(Language::Dart),
        "el" => Some(Language::Lisp),
        "go" => Some(Language::Go),
        "html" => Some(Language::HTML),
        "java" => Some(Language::Java),
        "js" | "mjs" | "cjs" => Some(Language::JavaScript),
        "json" => Some(Language::JSON),
        "kt" => Some(Language::Kotlin),
        "lua" => Some(Language::LUA),
        "md" => Some(Language::MarkDown),
        "php" => Some(Language::PHP),
        "proto" => Some(Language::ProtoBuf),
        "py" => Some(Language::Python),
        "r" => Some(Language::R),
        "rb" => Some(Language::Ruby),
        "rs" => Some(Language::Rust),
        "scala" => Some(Language::Scala),
        "sql" => Some(Language::SQL),
        "swift" => Some(Language::Swift),
        "ts" => Some(Language::TypeScript),
        "tsx" => Some(Language::TSX),
        "vim" => Some(Language::Vimscript),
        "yaml" | "yml" => Some(Language::YAML),
        "xml" | "xht" | "xhtml" => Some(Language::XML),
        _ => None,
    }
}

/**
* Returns the treesitter language structure, if supported.
*/
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
        // Test fixtures are named e.g. `before.py.test`; opening one directly should still
        // detect the real (inner) language rather than failing on the literal `.test` extension.
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
}
