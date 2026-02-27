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
use anyhow::Result;
use crossbeam_channel::Sender;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::metadata;

pub fn all_files_from_path(root: &Path, path_tx: Sender<PathBuf>) -> Result<()> {
    if root.is_file() {
        // Ignore error if no receivers (program shutting down)
        if !metadata::is_anomalous(root) {
            let _ = path_tx.send(PathBuf::from(root));
        }
    } else if root.is_dir() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                if metadata::is_anomalous(entry.path()) {
                    continue;
                }

                // This will block when the queue is full.
                if path_tx.send(entry.into_path()).is_err() {
                    // All workers are gone, stop producing.
                    break;
                }
            }
        }
    }
    drop(path_tx);

    Ok(())
}
