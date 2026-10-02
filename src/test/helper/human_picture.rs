/*  This file is part of the OmniDiff code diffing tool.
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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Human-authored ground truth for a picture pair: `src/test/data/pictures/<name>/human_picture.json`,
//! written by `human_solver` beside the pair's `before.<ext>.test` / `after.<ext>.test`.
//!
//! **A verdict, not regions.** The human answers the one question [`Verdict`] asks - content
//! change, no visible change, resized or replaced - and the fixture checks the engine's
//! [`PictureDiff::verdict`](crate::diff::picture::PictureDiff::verdict) against it. Which regions
//! changed is postponed: drawing them by hand in a terminal is the expensive part, and the verdict
//! already tests what the engine decides first (whether anything visible changed at all).
//!
//! The picture fixtures live outside `src/test/data/diffs/`, the code corpus, because everything
//! that sweeps that corpus parses its pairs as text.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::diff::picture::Verdict;

/// Where picture fixtures live, one directory per fixture.
pub fn pictures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("pictures")
}

/// The ground truth file of picture fixture `name`.
pub fn picture_path(name: &str) -> PathBuf {
    pictures_root().join(name).join("human_picture.json")
}

/// What a human recorded about a picture pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanPicture {
    pub verdict: Verdict,
}

pub fn load(name: &str) -> Result<HumanPicture> {
    let path = picture_path(name);
    let contents = std::fs::read_to_string(&path).with_context(|| format!("reading {path:?}"))?;
    serde_json::from_str(&contents).with_context(|| format!("parsing {path:?}"))
}

pub fn save(name: &str, picture: &HumanPicture) -> Result<()> {
    let path = picture_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string_pretty(picture)?;
    json.push('\n');
    std::fs::write(&path, json).with_context(|| format!("writing {path:?}"))
}

/// The pair's two files, as stored: `before.<ext>.test` and `after.<ext>.test`.
pub fn pair_bytes(name: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let dir = pictures_root().join(name);
    let mut before = None;
    let mut after = None;
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {dir:?}"))? {
        let path = entry?.path();
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !file.ends_with(".test") {
            continue;
        }
        if file.starts_with("before.") {
            before = Some(std::fs::read(&path)?);
        } else if file.starts_with("after.") {
            after = Some(std::fs::read(&path)?);
        }
    }
    match (before, after) {
        (Some(before), Some(after)) => Ok((before, after)),
        _ => bail!("{dir:?} has no before/after pair"),
    }
}

/// The engine's verdict for picture fixture `name`.
pub fn engine_verdict(name: &str) -> Result<Verdict> {
    let (before, after) = pair_bytes(name)?;
    Ok(crate::diff::picture::diff(&before, &after)?.verdict())
}

/// Fails unless the engine's verdict for `name` is the human's.
pub fn assert_matches_human_verdict(name: &str) -> Result<()> {
    let human = load(name)?.verdict;
    let engine = engine_verdict(name)?;
    if engine != human {
        bail!(
            "picture '{name}': omnidiff says {}, the human says {}",
            engine.label(),
            human.label()
        );
    }
    Ok(())
}

/// For a fixture the engine is known to get wrong: fails unless the engine still says `found`, so
/// a fix (or a different mistake) is noticed and the stub is updated.
pub fn assert_known_verdict_mismatch(name: &str, found: Verdict) -> Result<()> {
    let human = load(name)?.verdict;
    let engine = engine_verdict(name)?;
    if engine == human {
        bail!(
            "picture '{name}': omnidiff now agrees with the human ({}); make the stub \
             assert_matches_human_verdict",
            human.label()
        );
    }
    if engine != found {
        bail!(
            "picture '{name}': omnidiff now says {}, not the {} recorded here (the human says {})",
            engine.label(),
            found.label(),
            human.label()
        );
    }
    Ok(())
}
