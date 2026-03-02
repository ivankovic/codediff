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
use serde::Serialize;
use std::fmt;

pub mod language;
pub mod metadata;
pub mod tip; // Since type is a reserved keyword in Rust, we use Croatian instead.

/**
* The main data structure. It owns and contains the actual code and all metadata.
*
* Any function that accepts this structure or any sub-field should not assume any fields are set
* and should always first check that the required data is actually available, if not it should try
* to construct it, and if that doesn't work it should fail-safe, ideally returning a safe zero
* result. This allows the calling code to extremely efficiently process large files, and files that
* only pretend to be code but are data or configuration.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct Code {
    /// The actual code.
    pub contents: String,
    /// The metadata about the code.
    pub metadata: Metadata,
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
