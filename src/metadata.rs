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
    "libavcodec/metasound_data.h",
    // Swift
    "swiftlang-swift.git/test",
];

/**
* Returns true if the path matches one of the well known problematic paths.
*
* Most of these are highly atypical code, usualy an extremely long array, that results in a very
* unbalanced syntax tree with huge depth and can cause a panic.
*
* Or they are code we do not want to consider as code, notably /.git/ and similar dot-directories
* and code that is vendored in the repository but is not owned by the repository.
*/
pub fn is_anomalous(path: &Path) -> bool {
    WELL_KNOWN_ANOMALOUS_PATHS
        .iter()
        .any(|x| path.to_string_lossy().contains(x))
}
