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

use crate::code::Type;
use crate::code::language::language_for_extension;

/**
* Returns the type from path. If possible, the subtype is added as the string value to the enums
* gross Code/Config/Data categorization.
*/
pub fn type_from_path(path: &std::path::Path) -> Option<Type> {
    if let Some(filename) = path.file_name()
        && let Some(t) = type_from_filename(&filename.to_string_lossy())
    {
        return Some(t);
    }

    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if let Some(t) = type_from_extension(&ext) {
        return Some(t);
    }

    None
}

/**
* Returns the type from the extension.
*
* If possible, the subtype is added as the string value to the enums gross Code/Config/Data categorization.
*/
pub fn type_from_extension(ext: &str) -> Option<Type> {
    if language_for_extension(ext).is_some() {
        return Some(Type::Code(String::from("Uncategorized")));
    }

    match ext {
        "ini" | "cfg" | "toml" | "conf" => Some(Type::Configuration(String::from(""))),
        "txt" | "csv" | "rst" | "txtpb" | "pbtxt" | "stderr" | "doc" | "diff" => {
            Some(Type::Data(String::from("Text")))
        }
        "gz" | "zip" | "tar" | "bz2" | "xz" | "lz4" | "zst" | "br" | "7z" | "rar" => {
            Some(Type::Data(String::from("Archive")))
        }
        "svg" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif" => {
            Some(Type::Data(String::from("Image")))
        }
        "mp3" | "waw" | "flac" | "acc" | "ogg" | "m4a" => Some(Type::Data(String::from("Audio"))),
        "mp4" | "mkv" | "mov" | "avi" | "webm" => Some(Type::Data(String::from("Video"))),
        "ttf" | "otf" | "woff" | "woff2" => Some(Type::Data(String::from("Font"))),
        "sqlite" | "db" | "mdb" | "accdb" | "parquet" | "feather" | "arrow" | "orc" => {
            Some(Type::Data(String::from("Database")))
        }
        "bin" | "dat" | "exe" | "dll" | "class" | "so" | "o" | "wasm" | "symbols" | "idl"
        | "types" | "po" => Some(Type::Data(String::from("Binary"))),
        _ => None,
    }
}

/**
* Returns the type from the file name.
*
* If possible, the subtype is added as the string value to the enums gross Code/Config/Data categorization.
*/
pub fn type_from_filename(filename: &str) -> Option<Type> {
    match filename {
        "BUILD" | "Makefile" | "Dockerfile" => Some(Type::Code(String::from("Build"))),
        "OWNERS" | ".gitignore" => Some(Type::Configuration(String::from("DevEx"))),
        "LICENSE" => Some(Type::Documentation(String::from("Legal"))),
        "README" | "README.md" => Some(Type::Documentation(String::from("General"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_from_extension_for_invalid_extensions() {
        assert!(type_from_extension("invalid_extension_for_tests").is_none());
    }

    #[test]
    fn type_from_extension_for_valid_extensions() {
        assert!(matches!(
            type_from_extension("ini"),
            Some(Type::Configuration(_))
        ));
        assert!(matches!(type_from_extension("svg"), Some(Type::Data(_))));
    }
}
