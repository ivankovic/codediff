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

//! A picture pair on screen (see [`crate::diff::picture`]): the two pictures with their changed
//! regions outlined, or one composite of them, in four views `t` cycles through - GitHub's image
//! diff modes:
//!
//! * **side by side**: both, each region outlined in the delete (before) or insert (after) color;
//! * **difference**: the after picture faded out, its changed pixels in the update color;
//! * **blend**: the two mixed half and half, so a shifted element shows twice;
//! * **swipe**: before left of a divider and after right of it, `h`/`l` moving the divider.
//!
//! Drawn by `ratatui-image`: in a terminal that speaks the kitty, sixel or iTerm2 graphics protocol
//! (`tui::app` asks it at startup) as real pixels, and everywhere else as Unicode half blocks - two
//! pixels a cell, coarse but enough to see the layout and where the outlines are. The color-depth
//! pass fits half blocks to a 256-color terminal like any other cell.
//!
//! Composites are rebuilt only when the view, the divider, the pane size or the colors change:
//! encoding a picture for a graphics protocol is the slow part, not drawing it.

use std::path::Path;

use crate::diff::picture::{self, Comparison, PictureDiff, Region};
use crossterm::event::KeyCode;
use image::{DynamicImage, Rgba, RgbaImage};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{
    Resize, StatefulImage,
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
};

/// What pictures are shown over: transparency is compared as over white (`diff::picture`), and
/// half blocks would draw it black. Also what pads a scaled picture to whole cells.
const BACKDROP: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// The four ways a picture pair is shown, in the order `t` steps through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureMode {
    SideBySide,
    Difference,
    Blend,
    Swipe,
}

impl PictureMode {
    fn next(self) -> Self {
        match self {
            Self::SideBySide => Self::Difference,
            Self::Difference => Self::Blend,
            Self::Blend => Self::Swipe,
            Self::Swipe => Self::SideBySide,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SideBySide => "side by side",
            Self::Difference => "difference",
            Self::Blend => "blend",
            Self::Swipe => "swipe",
        }
    }
}

/// The overlay colors a picture view paints with, from the current theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureColors {
    pub delete: [u8; 3],
    pub insert: [u8; 3],
    pub update: [u8; 3],
}

/// The swipe divider: dark, because pictures are shown over white.
const DIVIDER: [u8; 3] = [48, 48, 48];

impl PictureColors {
    /// From theme colors; a color that is not RGB (a named or indexed one) falls back to a vivid
    /// default.
    pub fn from_theme(delete: Color, insert: Color, update: Color) -> Self {
        let rgb = |color: Color, fallback: [u8; 3]| match color {
            Color::Rgb(r, g, b) => [r, g, b],
            _ => fallback,
        };
        Self {
            delete: rgb(delete, [220, 50, 47]),
            insert: rgb(insert, [64, 160, 43]),
            update: rgb(update, [230, 160, 20]),
        }
    }
}

/// What the composites were built for; anything else means rebuilding them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Built {
    mode: PictureMode,
    swipe_percent: u16,
    outline: u32,
    colors: PictureColors,
}

pub struct PictureViewer {
    before_name: String,
    after_name: String,
    before: Option<RgbaImage>,
    after: Option<RgbaImage>,
    diff: PictureDiff,
    /// Which pixels changed, for a same-size pair.
    mask: Option<Vec<bool>>,
    mode: PictureMode,
    swipe_percent: u16,
    picker: Picker,
    shown: Option<(Built, Vec<(String, Option<StatefulProtocol>)>)>,
}

impl PictureViewer {
    /// The view of `before` and `after` if they are a picture pair that decodes; `None` otherwise,
    /// and the caller reports them as any other binary pair.
    pub fn open(before: &Path, after: &Path, picker: Picker) -> Option<Self> {
        let read = |path: &Path| std::fs::read(path).ok();
        let (before_bytes, after_bytes) = (read(before)?, read(after)?);
        if !picture::is_picture_pair(&before_bytes, &after_bytes) {
            return None;
        }
        let diff = picture::diff(&before_bytes, &after_bytes).ok()?;
        let pixels = |bytes: &[u8]| {
            (!bytes.is_empty())
                .then(|| picture::decode(bytes).ok().map(|(_, pixels)| pixels))
                .flatten()
        };
        let (before_pixels, after_pixels) = (pixels(&before_bytes), pixels(&after_bytes));
        let mask = match (&before_pixels, &after_pixels, &diff.comparison) {
            (Some(b), Some(a), Comparison::Pixels { .. }) => Some(picture::changed_mask(b, a)),
            _ => None,
        };
        Some(Self {
            before_name: before.display().to_string(),
            after_name: after.display().to_string(),
            before: before_pixels,
            after: after_pixels,
            diff,
            mask,
            mode: PictureMode::SideBySide,
            swipe_percent: 50,
            picker: {
                let mut picker = picker;
                picker.set_background_color(Some(BACKDROP));
                picker
            },
            shown: None,
        })
    }

    /// Switches to a different drawing protocol (the terminal answered the graphics query).
    pub fn set_picker(&mut self, mut picker: Picker) {
        picker.set_background_color(Some(BACKDROP));
        self.picker = picker;
        self.shown = None;
    }

    pub fn mode(&self) -> PictureMode {
        self.mode
    }

    /// Handles a picture view key: `t` cycles the view, `h`/`l` or the arrows move the swipe
    /// divider. False for any other key, which the viewer leaves to the rest of the app.
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('t') => self.mode = self.mode.next(),
            KeyCode::Char('h') | KeyCode::Left if self.mode == PictureMode::Swipe => {
                self.swipe_percent = self.swipe_percent.saturating_sub(5);
            }
            KeyCode::Char('l') | KeyCode::Right if self.mode == PictureMode::Swipe => {
                self.swipe_percent = (self.swipe_percent + 5).min(100);
            }
            _ => return false,
        }
        true
    }

    /// One line for the status bar: what each side is, how much changed, and the view.
    pub fn status(&self) -> String {
        let side = |info: &Option<picture::PictureInfo>| match info {
            Some(info) => format!(
                "{} {}x{} {}",
                info.format, info.width, info.height, info.color
            ),
            None => "nothing".to_string(),
        };
        let change = match &self.diff.comparison {
            Comparison::OneSided if self.diff.before.is_none() => "added".to_string(),
            Comparison::OneSided => "deleted".to_string(),
            Comparison::Resized => "resized".to_string(),
            Comparison::Pixels { regions, .. } if regions.is_empty() => {
                "no pixel changed".to_string()
            }
            Comparison::Pixels {
                changed_pixels,
                total_pixels,
                regions,
            } => format!(
                "{:.2}% of pixels changed, in {} region{}",
                100.0 * *changed_pixels as f64 / (*total_pixels).max(1) as f64,
                regions.len(),
                if regions.len() == 1 { "" } else { "s" }
            ),
        };
        format!(
            "{} -> {} · {change} · view: {} (t)",
            side(&self.diff.before),
            side(&self.diff.after),
            self.mode.label()
        )
    }

    /// Draws the current view into `area`.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, colors: PictureColors) {
        let panes: Vec<Rect> = if self.mode == PictureMode::SideBySide {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area)
                .to_vec()
        } else {
            vec![area]
        };
        let built = Built {
            mode: self.mode,
            swipe_percent: self.swipe_percent,
            outline: self.outline_width(Block::default().borders(Borders::ALL).inner(panes[0])),
            colors,
        };
        if self.shown.as_ref().map(|(was, _)| *was) != Some(built) {
            let shown = self
                .composites(built)
                .into_iter()
                .map(|(title, pixels)| {
                    let protocol = pixels.map(|pixels| {
                        self.picker
                            .new_resize_protocol(DynamicImage::ImageRgba8(over_backdrop(pixels)))
                    });
                    (title, protocol)
                })
                .collect();
            self.shown = Some((built, shown));
        }
        let Some((_, shown)) = self.shown.as_mut() else {
            return;
        };
        for (pane, (title, protocol)) in panes.iter().zip(shown.iter_mut()) {
            let block = Block::default().borders(Borders::ALL).title(title.as_str());
            let inner = block.inner(*pane);
            frame.render_widget(block, *pane);
            match protocol {
                Some(protocol) => frame.render_stateful_widget(
                    StatefulImage::default().resize(Resize::Scale(None)),
                    inner,
                    protocol,
                ),
                None => frame.render_widget(
                    Paragraph::new("nothing").style(Style::new().fg(Color::DarkGray)),
                    inner,
                ),
            }
        }
    }

    /// How many picture pixels wide an outline must be to show once the picture is scaled to fit
    /// `pane`: a whole cell in half blocks, whose cell is one picture pixel wide, and two screen
    /// pixels under a graphics protocol.
    fn outline_width(&self, pane: Rect) -> u32 {
        let half_blocks = self.picker.protocol_type() == ProtocolType::Halfblocks;
        let (per_cell, screen_pixels) = if half_blocks {
            (1, 1)
        } else {
            (u32::from(self.picker.font_size().width.max(1)), 2)
        };
        let pane_pixels = u32::from(pane.width.max(1)) * per_cell;
        let widest = [&self.before, &self.after]
            .iter()
            .filter_map(|side| side.as_ref().map(|pixels| pixels.width()))
            .max()
            .unwrap_or(1);
        (widest.div_ceil(pane_pixels) * screen_pixels).max(1)
    }

    /// One `(title, picture)` per pane of `built.mode`; `None` for a side that has no picture.
    fn composites(&self, built: Built) -> Vec<(String, Option<RgbaImage>)> {
        let regions: &[Region] = match &self.diff.comparison {
            Comparison::Pixels { regions, .. } => regions,
            _ => &[],
        };
        let outlined = |pixels: &Option<RgbaImage>, color: [u8; 3]| {
            pixels.as_ref().map(|pixels| {
                let mut pixels = pixels.clone();
                for region in regions {
                    outline(&mut pixels, region, built.outline, color);
                }
                pixels
            })
        };
        let before_title = format!("before: {}", self.before_name);
        let after_title = format!("after: {}", self.after_name);
        // The other three need both sides at one size: before is scaled to after's.
        let (Some(before), Some(after)) = (&self.before, &self.after) else {
            return vec![
                (before_title, outlined(&self.before, built.colors.delete)),
                (after_title, outlined(&self.after, built.colors.insert)),
            ];
        };
        let before_at_after_size = || {
            if before.dimensions() == after.dimensions() {
                before.clone()
            } else {
                image::imageops::resize(
                    before,
                    after.width(),
                    after.height(),
                    image::imageops::FilterType::Triangle,
                )
            }
        };
        match built.mode {
            PictureMode::SideBySide => vec![
                (before_title, outlined(&self.before, built.colors.delete)),
                (after_title, outlined(&self.after, built.colors.insert)),
            ],
            PictureMode::Difference => {
                let title = match &self.mask {
                    Some(_) => "difference: changed pixels on the faded after picture",
                    None => "difference: not comparable pixel by pixel (resized)",
                };
                vec![(
                    title.to_string(),
                    Some(difference(after, self.mask.as_deref(), built.colors.update)),
                )]
            }
            PictureMode::Blend => {
                vec![(
                    "blend: before and after, half and half".to_string(),
                    Some(blend(&before_at_after_size(), after)),
                )]
            }
            PictureMode::Swipe => vec![(
                format!("swipe: before | after at {}% (h/l)", built.swipe_percent),
                Some(swipe(
                    &before_at_after_size(),
                    after,
                    built.swipe_percent,
                    built.outline,
                    DIVIDER,
                )),
            )],
        }
    }
}

/// `pixels` flattened over [`BACKDROP`], fully opaque.
fn over_backdrop(mut pixels: RgbaImage) -> RgbaImage {
    for pixel in pixels.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in 0..3 {
            let over = u16::from(BACKDROP[channel]);
            pixel[channel] =
                ((u16::from(pixel[channel]) * alpha + over * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
    pixels
}

/// Draws `region`'s border, `width` pixels thick and outside the region where there is room.
fn outline(pixels: &mut RgbaImage, region: &Region, width: u32, color: [u8; 3]) {
    let (w, h) = pixels.dimensions();
    let x0 = region.x.saturating_sub(width);
    let y0 = region.y.saturating_sub(width);
    let x1 = (region.x + region.width + width).min(w);
    let y1 = (region.y + region.height + width).min(h);
    let color = Rgba([color[0], color[1], color[2], 255]);
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = x >= region.x
                && x < region.x + region.width
                && y >= region.y
                && y < region.y + region.height;
            if !inside {
                pixels.put_pixel(x, y, color);
            }
        }
    }
}

/// `after` faded three quarters of the way to white, its changed pixels in `color`.
fn difference(after: &RgbaImage, mask: Option<&[bool]>, color: [u8; 3]) -> RgbaImage {
    let mut out = after.clone();
    for (index, pixel) in out.pixels_mut().enumerate() {
        if mask.is_some_and(|mask| mask[index]) {
            *pixel = Rgba([color[0], color[1], color[2], 255]);
        } else {
            let fade = |channel: u8| (u16::from(channel) / 4 + 191) as u8;
            *pixel = Rgba([
                fade(pixel[0]),
                fade(pixel[1]),
                fade(pixel[2]),
                pixel[3].max(64),
            ]);
        }
    }
    out
}

/// The two same-size pictures mixed half and half.
fn blend(before: &RgbaImage, after: &RgbaImage) -> RgbaImage {
    let mut out = after.clone();
    for (pixel, other) in out.pixels_mut().zip(before.pixels()) {
        for channel in 0..4 {
            pixel[channel] = ((u16::from(pixel[channel]) + u16::from(other[channel])) / 2) as u8;
        }
    }
    out
}

/// Before left of the divider at `percent` of the width, after right of it, the divider `width`
/// pixels in `color`.
fn swipe(
    before: &RgbaImage,
    after: &RgbaImage,
    percent: u16,
    width: u32,
    color: [u8; 3],
) -> RgbaImage {
    let mut out = after.clone();
    let split = after.width() * u32::from(percent) / 100;
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        if x < split {
            *pixel = *before.get_pixel(x, y);
        }
        if x >= split.saturating_sub(width / 2) && x < split + width.div_ceil(2) {
            *pixel = Rgba([color[0], color[1], color[2], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: PictureColors = PictureColors {
        delete: [200, 0, 0],
        insert: [0, 200, 0],
        update: [250, 150, 0],
    };

    fn png(pixels: &RgbaImage, dir: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        pixels.save(&path).expect("writes");
        path
    }

    /// A 20x10 white picture, and the same with a black 4x3 block at (6, 2).
    fn pair(dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        let before = RgbaImage::from_pixel(20, 10, Rgba([255, 255, 255, 255]));
        let mut after = before.clone();
        for y in 2..5 {
            for x in 6..10 {
                after.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        (png(&before, dir, "a.png"), png(&after, dir, "b.png"))
    }

    fn viewer(before: &Path, after: &Path) -> PictureViewer {
        PictureViewer::open(before, after, Picker::halfblocks()).expect("a picture pair")
    }

    fn built(mode: PictureMode) -> Built {
        Built {
            mode,
            swipe_percent: 50,
            outline: 1,
            colors: COLORS,
        }
    }

    #[test]
    fn t_cycles_the_views_and_the_divider_moves_only_in_swipe() {
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = pair(&dir);
        let mut viewer = viewer(&a, &b);
        assert!(
            !viewer.handle_key(KeyCode::Char('l')),
            "no divider outside swipe"
        );
        for expected in [
            PictureMode::Difference,
            PictureMode::Blend,
            PictureMode::Swipe,
        ] {
            assert!(viewer.handle_key(KeyCode::Char('t')));
            assert_eq!(viewer.mode(), expected);
        }
        assert!(viewer.handle_key(KeyCode::Right));
        assert_eq!(viewer.swipe_percent, 55);
        assert!(viewer.handle_key(KeyCode::Char('t')));
        assert_eq!(viewer.mode(), PictureMode::SideBySide);
        assert!(!viewer.handle_key(KeyCode::Char('x')));
    }

    #[test]
    fn side_by_side_outlines_each_region_in_its_sides_color() {
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = pair(&dir);
        let panes = viewer(&a, &b).composites(built(PictureMode::SideBySide));
        let [(_, Some(before)), (_, Some(after))] = &panes[..] else {
            panic!("two panes, both pictures: {}", panes.len());
        };
        // The outline runs one pixel outside the 4x3 region at (6, 2).
        assert_eq!(before.get_pixel(5, 1).0, [200, 0, 0, 255]);
        assert_eq!(after.get_pixel(10, 5).0, [0, 200, 0, 255]);
        assert_eq!(
            after.get_pixel(7, 3).0,
            [0, 0, 0, 255],
            "inside is left alone"
        );
        assert_eq!(
            after.get_pixel(0, 9).0,
            [255, 255, 255, 255],
            "far away too"
        );
    }

    #[test]
    fn difference_paints_changed_pixels_over_the_faded_picture() {
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = pair(&dir);
        let panes = viewer(&a, &b).composites(built(PictureMode::Difference));
        let [(_, Some(difference))] = &panes[..] else {
            panic!("one pane");
        };
        assert_eq!(difference.get_pixel(7, 3).0, [250, 150, 0, 255]);
        assert_eq!(difference.get_pixel(0, 0).0, [254, 254, 254, 255]);
    }

    #[test]
    fn swipe_shows_before_left_of_the_divider_and_blend_mixes_the_two() {
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = pair(&dir);
        let mut viewer = viewer(&a, &b);
        let mut at = built(PictureMode::Swipe);
        at.swipe_percent = 80; // the divider at x=16, right of the block
        let panes = viewer.composites(at);
        let [(_, Some(swiped))] = &panes[..] else {
            panic!("one pane")
        };
        assert_eq!(
            swiped.get_pixel(7, 3).0,
            [255, 255, 255, 255],
            "before: no block yet"
        );
        assert_eq!(swiped.get_pixel(16, 3).0, [48, 48, 48, 255], "the divider");

        viewer.mode = PictureMode::Blend;
        let panes = viewer.composites(built(PictureMode::Blend));
        let [(_, Some(blended))] = &panes[..] else {
            panic!("one pane")
        };
        assert_eq!(blended.get_pixel(7, 3).0, [127, 127, 127, 255]);
    }

    #[test]
    fn a_resized_pair_still_blends_and_an_added_picture_shows_one_side() {
        let dir = tempfile::tempdir().expect("dir");
        let small = png(
            &RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255])),
            &dir,
            "s.png",
        );
        let large = png(
            &RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255])),
            &dir,
            "l.png",
        );
        let resized = viewer(&small, &large);
        assert!(resized.status().contains("resized"), "{}", resized.status());
        let panes = resized.composites(built(PictureMode::Blend));
        assert!(matches!(&panes[..], [(_, Some(blended))] if blended.dimensions() == (8, 8)));

        let empty = dir.path().join("empty.png");
        std::fs::write(&empty, b"").expect("writes");
        let added = viewer(&empty, &large);
        assert!(added.status().contains("added"), "{}", added.status());
        let panes = added.composites(built(PictureMode::Difference));
        assert!(
            matches!(&panes[..], [(_, None), (_, Some(_))]),
            "{}",
            panes.len()
        );
    }

    #[test]
    fn a_pair_that_is_not_two_pictures_is_not_opened() {
        let dir = tempfile::tempdir().expect("dir");
        let (a, _) = pair(&dir);
        let text = dir.path().join("a.txt");
        std::fs::write(&text, "text").expect("writes");
        assert!(PictureViewer::open(&a, &text, Picker::halfblocks()).is_none());
    }
}
