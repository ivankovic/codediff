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

//! Paths the corpus tools skip on sight - vendored trees, minified bundles, generated files -
//! so a sample is drawn from code somebody wrote.
use std::path::Path;

static WELL_KNOWN_ANOMALOUS_PATHS: &[&str] = &[
    "/.gitignore",
    "/.gitkeep",
    "/.git/",
    "/.github/",
    "third_party",
    "3rdparty",
    // Firefox
    "security/nss/gtests/freebl_gtest/kat",
    // OpenCV
    "blob/4.x/3rdparty/openexr/Half/toFloat.h",
    // ffmpeg
    "libavcodec/metasound_data.h",
    "libavcodec/on2avcdata.c",
    "libavcodec/dcadata.c",
    "libavcodec/hq_hqadata.h",
    "libavcodec/ralfdata.h",
    "libavcodec/twinvq_data.h",
    // Swift
    "swiftlang-swift.git/test",
];

/// True if `path` contains one of the well-known anomalous path fragments: vendored or dot-directory
/// code the repository does not own, or files (usually one huge array) whose tree is deep enough to
/// panic the parser.
pub fn is_anomalous(path: &Path) -> bool {
    WELL_KNOWN_ANOMALOUS_PATHS
        .iter()
        .any(|x| path.to_string_lossy().contains(x))
}
