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
pub mod language;
pub mod metadata;
pub mod tip; // Since type is a reserved keyword in Rust, we use Croatian instead.

use anyhow;
use serde::Serialize;
use std::fmt;

/**
* The main data structure. It owns and contains the actual code and all metadata.
*
* Any function that accepts this structure or any sub-field should not assume any fields are set
* and should always first check that the required data is actually available, if not it should try
* to construct it, and if that doesn't work it should fail-safe, ideally returning a safe zero
* result. This allows the calling code to extremely efficiently process large files, and files that
* only pretend to be code but are data or configuration. To help the compiler enforce this, most
* leaf fields should be wrapped in Option. Ideally, the only non-Option wrapped field is the code
* itself.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct Code {
    /// The actual code.
    pub contents: String,
    /// The metadata about the code.
    pub metadata: Metadata,
}

/**
* Constructs a Code structure from the given string and language.
*
* Note that the metadata type will be assumed to be Code. If for some reason you want to use this
* to construct configuration, data or documentation, make sure to update the metadata accordingly
* after construction.
*
* TODO: Make this code auto-recognize type based on contents to correctly construct Code objects
* that are actually Configuration, e.g. docker-compose YAML files. It will require expanding the
* language.rs detection to support content aware metadata expansion.
*/
pub fn from_string(contents: &str, language: &Language) -> Code {
    Code {
        contents: contents.to_string(),
        metadata: Metadata {
            path: None,
            tip: Some(Type::Code("Code".to_string())),
            language: Some(language.clone()),
        },
    }
}

/**
* Constructs a Code structure from the given file path.
*
* The language is automatically detected from the file extension. If the extension is not
* recognized, the language will be set to Unknown.
*
* Note that the metadata type will be assumed to be Code. The path will be stored in the metadata.
*
* TODO: Use the hermetic expansion from metadata.rs to expand the metadata.
*/
pub fn from_file(path: &std::path::Path) -> anyhow::Result<Code> {
    use std::fs;

    let contents = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?;

    // Determine language from file extension
    let language = language::language_for_path(path).unwrap_or(Language::Unknown);

    let mut code = from_string(&contents, &language);
    code.metadata.path = Some(path.to_path_buf());

    Ok(code)
}

/**
* The metadata around the code, but not the code itself.
*
* This is only the metadata that is necessary for the diffing. Statistics and test data should not
* be added here.
*
* Most fields in this class should be optional, to allow for efficient computation.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metadata {
    /// The path to the code, if one exists.
    pub path: Option<std::path::PathBuf>,
    /// The gross type of the "code".
    /// Since type is a reserved keyword in Rust, we use Croatian instead.
    pub tip: Option<Type>,
    /// The language.
    pub language: Option<Language>,
}

/**
* The programming language.
*
* Implemented as a crate enum instead of reusing someting like TreeSitter language to allow for
* better error handling of unknown or not-supported languages.
*/
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub enum Language {
    #[default]
    Unknown,
    // Alphabetically ordered.
    Bazel,
    C,
    CPP,
    CSS,
    CSharp,
    Dart,
    Go,
    HTML,
    JSON,
    Java,
    JavaScript,
    Kotlin,
    LUA,
    Lisp,
    MarkDown,
    PHP,
    ProtoBuf,
    Python,
    R,
    Ruby,
    Rust,
    SQL,
    Scala,
    ShellScript,
    Swift,
    TSX,
    TypeScript,
    Vimscript,
    YAML,
    XML,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/**
* The type of "code". Very often, code files are not actually code, but rather configuration or
* data, or, much more rarely, documentation.
*
* The enums gross values separate the four big areas. Each instantiation can further contain an
* arbitrary string that provides fine grained information on what type of code, configuration,
* data or documentation exactly the file contents are.
*/
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub enum Type {
    #[default]
    Unknown,
    Code(String),
    Configuration(String),
    Data(String),
    Documentation(String),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_from_empty_string() {
        let c = from_string("", &Language::Rust);

        assert_eq!(c.contents, "");
    }

    #[test]
    fn code_from_file() -> anyhow::Result<()> {
        use crate::test::helper::handmade_test_code_as_paths;

        let paths = handmade_test_code_as_paths()?;
        let hello_world_path = paths
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let code = from_file(hello_world_path)?;

        assert_eq!(code.metadata.language, Some(Language::Rust));
        assert!(code.metadata.path.is_some());
        assert!(code.contents.contains("fn main()"));
        assert!(code.contents.contains("Hello, World"));

        Ok(())
    }
}
