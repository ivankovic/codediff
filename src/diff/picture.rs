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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

//! Pictures: what changed between two raster images, for the files a code diff cannot read.
//!
//! The change census (`research/data/corpus_stats/change_census.csv`) puts images at 39% of the
//! binary files commits change (fonts, the other big share, come from about thirty repositories),
//! and PNG in 8.4% of repositories - the most widespread binary format by far. A picture pair gets what a reader asks first: whether the picture changed size
//! or format, how much of it changed, and where.
//!
//! **Which pixels changed** is pixelmatch's perceptual test (Mapbox's `pixelmatch`, after
//! Kotsarenko and Ramos, "Measuring perceived color difference using YIQ NTSC transmission color
//! space"): both pixels blended over white by their alpha, the YIQ distance compared against
//! `THRESHOLD` of its maximum. Unlike exact equality it does not report re-encoding noise as change,
//! and unlike a per-channel tolerance it weighs brightness as the eye does.
//!
//! **Where** is the changed pixels grouped into rectangles: pixels within `REGION_GAP` of each other
//! belong to one region, so one edited icon is one region rather than a scatter of its pixels.
//!
//! Pictures of different sizes are not compared pixel by pixel - nothing says which pixel became
//! which - and are reported as resized. SVG is not a picture here: it is XML, and the XML grammar
//! diffs it structurally (`code::language::XML_FORMAT_EXTENSIONS`).

use anyhow::{Context, Result};
use image::{GenericImageView, RgbaImage};
use serde::Serialize;

/// Extensions treated as pictures, lower-cased: the raster formats `image` decodes here.
pub const PICTURE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff",
];

/// The share of the largest possible YIQ distance above which two pixels differ: pixelmatch's
/// default.
pub const THRESHOLD: f64 = 0.1;

/// Changed pixels this close (in both directions) belong to one region.
pub const REGION_GAP: u32 = 2;

/// The share of pixels above which a change reads as a different picture rather than an edit of
/// the same one ([`Verdict::Replaced`]). A first guess, which the picture fixtures measure.
pub const REPLACED_SHARE: f64 = 0.5;

/// What happened to a picture, in one word: the question a picture fixture's human verdict
/// answers, and the engine's answer to it ([`PictureDiff::verdict`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The same picture, edited: something in it changed.
    ContentChange,
    /// Nothing a reader would see changed: re-encoded, re-compressed, or only metadata.
    NoVisibleChange,
    /// Scaled or re-cropped to a different size.
    Resized,
    /// A different picture altogether.
    Replaced,
}

impl Verdict {
    pub const ALL: [Verdict; 4] = [
        Verdict::ContentChange,
        Verdict::NoVisibleChange,
        Verdict::Resized,
        Verdict::Replaced,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Verdict::ContentChange => "content change",
            Verdict::NoVisibleChange => "no visible change",
            Verdict::Resized => "resized",
            Verdict::Replaced => "replaced",
        }
    }
}

/// True if `path` names a raster picture by its extension.
pub fn is_picture_path(path: &std::path::Path) -> bool {
    let path = match path.extension() {
        // A fixture is `before.png.test`.
        Some(ext) if ext.eq_ignore_ascii_case("test") => {
            std::path::Path::new(path.file_stem().unwrap_or_default())
        }
        _ => path,
    };
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|ext| PICTURE_EXTENSIONS.contains(&ext.as_str()))
}

/// True if `bytes` start like one of the supported raster formats, whatever the file is called:
/// under `git diff` the paths are git's temporary files.
pub fn is_picture(bytes: &[u8]) -> bool {
    use image::ImageFormat::*;
    matches!(
        image::guess_format(bytes),
        Ok(Png | Jpeg | Gif | WebP | Bmp | Ico | Tiff)
    )
}

/// True if a binary pair is a picture pair: every side present is a picture, and one is. An empty
/// side is git's `/dev/null` for an added or deleted file.
pub fn is_picture_pair(before: &[u8], after: &[u8]) -> bool {
    let fits = |bytes: &[u8]| bytes.is_empty() || is_picture(bytes);
    fits(before) && fits(after) && !(before.is_empty() && after.is_empty())
}

/// One side of a picture pair, as the file says it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PictureInfo {
    /// The format the bytes are in, whatever the extension says (`"PNG"`).
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// The stored color type (`"RGBA8"`, `"L8"`): a palette or bit-depth change shows here when
    /// the pixels did not change.
    pub color: String,
    /// The file's size in bytes.
    pub bytes: usize,
}

/// A rectangle of changed pixels, in the pixel coordinates both pictures share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// How many of the rectangle's pixels changed.
    pub changed_pixels: u64,
}

/// What changed between two pictures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Comparison {
    /// One side is missing (an added or deleted file): nothing to compare.
    OneSided,
    /// The same size, compared pixel by pixel. No region means no visible change, even if the
    /// bytes differ (a re-encoding, a metadata change).
    Pixels {
        changed_pixels: u64,
        total_pixels: u64,
        regions: Vec<Region>,
    },
    /// Different sizes: not compared pixel by pixel.
    Resized,
}

/// The whole picture diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PictureDiff {
    pub before: Option<PictureInfo>,
    pub after: Option<PictureInfo>,
    pub comparison: Comparison,
}

impl PictureDiff {
    /// The engine's verdict: resized for a size change, no visible change when no region
    /// changed, replaced when more than [`REPLACED_SHARE`] of the pixels did, and content change
    /// otherwise - also for an added or deleted picture, which the fixtures do not hold.
    pub fn verdict(&self) -> Verdict {
        match &self.comparison {
            Comparison::Resized => Verdict::Resized,
            Comparison::Pixels { regions, .. } if regions.is_empty() => Verdict::NoVisibleChange,
            Comparison::Pixels {
                changed_pixels,
                total_pixels,
                ..
            } if *changed_pixels as f64 > REPLACED_SHARE * *total_pixels as f64 => {
                Verdict::Replaced
            }
            Comparison::Pixels { .. } | Comparison::OneSided => Verdict::ContentChange,
        }
    }

    /// True if a reader would see a difference: a side added or deleted, a size, format or color
    /// change, or changed pixels.
    pub fn differs(&self) -> bool {
        match &self.comparison {
            Comparison::OneSided | Comparison::Resized => true,
            Comparison::Pixels { regions, .. } => {
                !regions.is_empty() || {
                    let (Some(before), Some(after)) = (&self.before, &self.after) else {
                        return true;
                    };
                    before.format != after.format || before.color != after.color
                }
            }
        }
    }
}

/// Decodes `bytes` as a picture: its description and its pixels as 8-bit RGBA. The first frame of
/// an animation.
pub fn decode(bytes: &[u8]) -> Result<(PictureInfo, RgbaImage)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("reading the picture")?;
    let format = reader
        .format()
        .map(|format| format!("{format:?}").to_uppercase())
        .unwrap_or_else(|| "unknown".to_string());
    let picture = reader.decode().context("decoding the picture")?;
    let (width, height) = picture.dimensions();
    let info = PictureInfo {
        format,
        width,
        height,
        color: format!("{:?}", picture.color()).to_uppercase(),
        bytes: bytes.len(),
    };
    Ok((info, picture.to_rgba8()))
}

/// Compares two pictures; an empty side (git's `/dev/null` for an added or deleted file) is
/// absent rather than an error.
pub fn diff(before: &[u8], after: &[u8]) -> Result<PictureDiff> {
    let side = |bytes: &[u8]| -> Result<Option<(PictureInfo, RgbaImage)>> {
        if bytes.is_empty() {
            Ok(None)
        } else {
            decode(bytes).map(Some)
        }
    };
    let (before, after) = (side(before)?, side(after)?);
    let comparison = match (&before, &after) {
        (Some((_, b)), Some((_, a))) if b.dimensions() == a.dimensions() => compare(b, a),
        (Some(_), Some(_)) => Comparison::Resized,
        _ => Comparison::OneSided,
    };
    Ok(PictureDiff {
        before: before.map(|(info, _)| info),
        after: after.map(|(info, _)| info),
        comparison,
    })
}

/// Which pixels of two same-size pictures differ, as a row-major mask.
pub fn changed_mask(before: &RgbaImage, after: &RgbaImage) -> Vec<bool> {
    // pixelmatch's maximum YIQ distance, 35215, scaled by the threshold squared.
    let limit = 35215.0 * THRESHOLD * THRESHOLD;
    before
        .pixels()
        .zip(after.pixels())
        .map(|(b, a)| b != a && yiq_distance(b.0, a.0) > limit)
        .collect()
}

fn compare(before: &RgbaImage, after: &RgbaImage) -> Comparison {
    let (width, height) = before.dimensions();
    let mask = changed_mask(before, after);
    let changed_pixels = mask.iter().filter(|&&changed| changed).count() as u64;
    Comparison::Pixels {
        changed_pixels,
        total_pixels: u64::from(width) * u64::from(height),
        regions: regions(&mask, width, height),
    }
}

/// pixelmatch's `colorDelta`: both pixels blended over white, then the weighted distance in YIQ.
fn yiq_distance(before: [u8; 4], after: [u8; 4]) -> f64 {
    let blend = |[r, g, b, a]: [u8; 4]| {
        let alpha = f64::from(a) / 255.0;
        let over_white = |channel: u8| 255.0 + (f64::from(channel) - 255.0) * alpha;
        (over_white(r), over_white(g), over_white(b))
    };
    let (r1, g1, b1) = blend(before);
    let (r2, g2, b2) = blend(after);
    let y = |r: f64, g: f64, b: f64| r * 0.298_895_31 + g * 0.586_622_47 + b * 0.114_482_23;
    let i = |r: f64, g: f64, b: f64| r * 0.595_977_99 - g * 0.274_176_47 - b * 0.321_801_52;
    let q = |r: f64, g: f64, b: f64| r * 0.211_470_17 - g * 0.522_617_24 + b * 0.311_147_07;
    let dy = y(r1, g1, b1) - y(r2, g2, b2);
    let di = i(r1, g1, b1) - i(r2, g2, b2);
    let dq = q(r1, g1, b1) - q(r2, g2, b2);
    0.5053 * dy * dy + 0.299 * di * di + 0.1957 * dq * dq
}

/// The changed pixels of a `width` x `height` mask grouped into rectangles: pixels within
/// `REGION_GAP` of each other are one region. Largest first, then top to bottom.
pub fn regions(mask: &[bool], width: u32, height: u32) -> Vec<Region> {
    let (w, h) = (width as usize, height as usize);
    let gap = REGION_GAP as usize;
    let mut seen = vec![false; mask.len()];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut changed_pixels = 0u64;
        while let Some(at) = stack.pop() {
            let (x, y) = (at % w, at / w);
            changed_pixels += 1;
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for ny in y.saturating_sub(gap)..(y + gap + 1).min(h) {
                for nx in x.saturating_sub(gap)..(x + gap + 1).min(w) {
                    let next = ny * w + nx;
                    if mask[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        found.push(Region {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0 + 1) as u32,
            height: (y1 - y0 + 1) as u32,
            changed_pixels,
        });
    }
    found.sort_by_key(|region| {
        (
            std::cmp::Reverse(u64::from(region.width) * u64::from(region.height)),
            region.y,
            region.x,
        )
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(picture: &RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        picture
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encodes");
        bytes
    }

    fn filled(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba(color))
    }

    #[test]
    fn identical_pictures_have_no_regions() {
        let picture = png(&filled(8, 8, [10, 20, 30, 255]));
        let diff = diff(&picture, &picture).expect("diffs");
        assert_eq!(
            diff.comparison,
            Comparison::Pixels {
                changed_pixels: 0,
                total_pixels: 64,
                regions: Vec::new()
            }
        );
        assert!(!diff.differs());
        assert_eq!(
            diff.before.as_ref().map(|info| info.format.as_str()),
            Some("PNG")
        );
    }

    #[test]
    fn a_changed_block_is_one_region_where_it_changed() {
        let before = filled(20, 10, [255, 255, 255, 255]);
        let mut after = before.clone();
        for y in 2..5 {
            for x in 6..10 {
                after.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let diff = diff(&png(&before), &png(&after)).expect("diffs");
        let Comparison::Pixels {
            changed_pixels,
            regions,
            ..
        } = diff.comparison
        else {
            panic!("same size, so compared by pixels");
        };
        assert_eq!(changed_pixels, 12);
        assert_eq!(
            regions,
            vec![Region {
                x: 6,
                y: 2,
                width: 4,
                height: 3,
                changed_pixels: 12
            }]
        );
    }

    #[test]
    fn far_apart_changes_are_separate_regions_and_near_ones_merge() {
        let mut mask = vec![false; 30 * 3];
        for x in [0, 2, 20] {
            mask[x] = true;
        }
        let found = regions(&mask, 30, 3);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            (found[0].x, found[0].width, found[0].changed_pixels),
            (0, 3, 2)
        );
        assert_eq!((found[1].x, found[1].width), (20, 1));
    }

    #[test]
    fn a_change_below_the_perceptual_threshold_is_not_a_change() {
        let before = filled(4, 4, [100, 100, 100, 255]);
        let after = filled(4, 4, [101, 100, 100, 255]);
        let diff = diff(&png(&before), &png(&after)).expect("diffs");
        assert!(matches!(
            diff.comparison,
            Comparison::Pixels {
                changed_pixels: 0,
                ..
            }
        ));
    }

    #[test]
    fn transparent_pixels_compare_as_what_they_look_like_over_white() {
        // Fully transparent black and fully transparent red are both just white.
        let before = filled(2, 2, [0, 0, 0, 0]);
        let after = filled(2, 2, [255, 0, 0, 0]);
        assert!(changed_mask(&before, &after).iter().all(|changed| !changed));
    }

    #[test]
    fn a_resized_picture_is_not_compared_pixel_by_pixel() {
        let diff = diff(&png(&filled(4, 4, [0; 4])), &png(&filled(8, 4, [0; 4]))).expect("diffs");
        assert_eq!(diff.comparison, Comparison::Resized);
        assert_eq!(
            diff.after.map(|info| (info.width, info.height)),
            Some((8, 4))
        );
    }

    #[test]
    fn an_empty_side_is_an_added_or_deleted_picture() {
        let diff = diff(b"", &png(&filled(2, 2, [0; 4]))).expect("diffs");
        assert_eq!(diff.comparison, Comparison::OneSided);
        assert!(diff.before.is_none() && diff.differs());
    }

    #[test]
    fn a_picture_pair_is_known_by_its_bytes() {
        let picture = png(&filled(2, 2, [0; 4]));
        assert!(is_picture_pair(&picture, &picture));
        assert!(is_picture_pair(b"", &picture), "an added picture");
        assert!(
            !is_picture_pair(&picture, b"%PDF-1.7\n"),
            "one side is not a picture"
        );
        assert!(!is_picture_pair(b"", b""));
    }

    #[test]
    fn the_engine_verdict_follows_the_comparison() {
        let small = png(&filled(10, 10, [255; 4]));
        let mut dotted = filled(10, 10, [255; 4]);
        dotted.put_pixel(3, 3, image::Rgba([0, 0, 0, 255]));
        let verdict = |a: &[u8], b: &[u8]| diff(a, b).expect("diffs").verdict();
        assert_eq!(verdict(&small, &small), Verdict::NoVisibleChange);
        assert_eq!(verdict(&small, &png(&dotted)), Verdict::ContentChange);
        assert_eq!(
            verdict(&small, &png(&filled(10, 10, [0, 0, 0, 255]))),
            Verdict::Replaced
        );
        assert_eq!(
            verdict(&small, &png(&filled(5, 10, [255; 4]))),
            Verdict::Resized
        );
    }

    #[test]
    fn pictures_are_known_by_extension_and_svg_is_not_one() {
        let is = |name: &str| is_picture_path(std::path::Path::new(name));
        assert!(is("logo.PNG") && is("photo.jpeg") && is("before.png.test"));
        assert!(!is("logo.svg") && !is("main.rs") && !is("Makefile"));
    }
}
