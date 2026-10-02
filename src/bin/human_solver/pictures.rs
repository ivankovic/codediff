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

//! The picture session: a sample whose pair is two pictures (`sample_test_diffs --pictures`) is
//! judged here rather than in the tree session, which needs trees.
//!
//! What the human records is a verdict (`test::helper::human_picture`), keys `1`-`4`. The pictures
//! are shown through `PictureViewer` in its annotation mode - side by side, blend and swipe, the
//! files' own metadata, and nothing the engine decided - so the verdict stays the human's. `e`
//! shows the engine's view on request (its verdict, outlined regions, the difference view) and
//! hides it again; each sample starts with it hidden. `s` promotes the sample to `src/test/data/pictures/<name>/` with its verdict and a `verdict()` stub,
//! or, once promoted, saves a changed verdict; `x` rejects it with a reason; `O` opens another
//! sample, and the tree session takes back over for a code one.
//!
//! The terminal is asked once which graphics protocol it speaks, on the first picture session:
//! this binary reads keys synchronously, so the answer cannot be lost to an event reader.

use std::fs;
use std::io::Stdout;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use omnidiff::test::helper::human_picture::{self, HumanPicture, Verdict};
use omnidiff::tui::components::picture_viewer::{PictureColors, PictureViewer};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;

use crate::events::{
    SessionEnd, handle_open_sample_picker, open_sample_picker, reject_sample, update_sample_csv,
};
use crate::render::render_open_sample_picker;
use crate::state::{App, Modal};
use crate::stubs::{LICENSE_HEADER, fixtures_dir, insert_mod_declaration, module_name};
use crate::{SampleSource, promoted_case_name, samples_root, source_json_for_sample};

/// Where promoted picture samples go, and the stub module that lists them.
const PICTURE_DATASET: &str = "pictures";

/// True if sample `name` is a pair of pictures: its `before.<ext>.test` names a picture format.
pub(crate) fn is_picture_sample(name: &str) -> bool {
    pair_paths(&samples_root().join(name))
        .is_some_and(|(before, _)| omnidiff::diff::picture::is_picture_path(&before))
}

/// `dir`'s `before.<ext>.test` and `after.<ext>.test`.
pub(crate) fn pair_paths(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut before = None;
    let mut after = None;
    for entry in fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        let file = path.file_name()?.to_string_lossy().into_owned();
        if !file.ends_with(".test") {
            continue;
        }
        if file.starts_with("before.") {
            before = Some(path);
        } else if file.starts_with("after.") {
            after = Some(path);
        }
    }
    before.zip(after)
}

/// The terminal's graphics protocol, asked once; half blocks if it does not answer.
fn picker() -> Picker {
    static PICKER: OnceLock<Picker> = OnceLock::new();
    PICKER
        .get_or_init(|| Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()))
        .clone()
}

struct PictureSession {
    name: String,
    source: SampleSource,
    /// The picture fixture this sample was promoted to, if it was.
    promoted: Option<String>,
    viewer: PictureViewer,
    verdict: Option<Verdict>,
    /// The verdict on disk, to warn before quitting with an unsaved one.
    saved: Option<Verdict>,
    /// A rejection reason being typed, after `x`.
    reject_input: Option<String>,
    status: String,
}

/// Runs the picture session for sample `name` until the human quits or opens something else.
pub(crate) fn run_picture_session(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    name: &str,
) -> Result<SessionEnd> {
    let dir = samples_root().join(name);
    let source = source_json_for_sample(name)
        .with_context(|| format!("{dir:?} has no readable source.json"))?;
    let (before, after) =
        pair_paths(&dir).with_context(|| format!("{dir:?} has no before/after pair"))?;
    let viewer = PictureViewer::open_for_annotation(&before, &after, picker())
        .with_context(|| format!("{dir:?} is not a pair of pictures that decode"))?;
    let promoted = promoted_case_name(&source);
    let saved = promoted
        .as_deref()
        .and_then(|fixture| human_picture::load(fixture).ok())
        .map(|picture| picture.verdict);
    let mut session = PictureSession {
        name: name.to_string(),
        source,
        promoted,
        viewer,
        verdict: saved,
        saved,
        reject_input: None,
        status: "1-4 records a verdict; s promotes or saves it".to_string(),
    };
    let mut quit_armed = false;

    loop {
        terminal.draw(|frame| draw(frame, &mut session, app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(log) = app.key_log.as_mut() {
            let mode = if session.reject_input.is_some() {
                "picture-typing"
            } else {
                "picture"
            };
            let text = if session.reject_input.is_some() {
                "typed".to_string()
            } else {
                format!("{:?}", key.code)
            };
            log.record(&session.name, mode, &text, false);
        }

        // The `O` picker, while open, takes every key.
        if let Some(Modal::OpenSamplePicker {
            rows,
            selected,
            view,
            name_input,
        }) = app.modal.take()
        {
            if let Some(target) =
                handle_open_sample_picker(app, key.code, rows, selected, view, name_input)
            {
                return Ok(SessionEnd::Open(target));
            }
            continue;
        }

        if let Some(mut reason) = session.reject_input.take() {
            match key.code {
                KeyCode::Enter => {
                    session.status = match reject(&session, &reason) {
                        Ok(message) => message,
                        Err(err) => format!("{err:#}"),
                    };
                }
                KeyCode::Esc => session.status = "Rejection cancelled".to_string(),
                KeyCode::Backspace => {
                    reason.pop();
                    session.reject_input = Some(reason);
                }
                KeyCode::Char(c) => {
                    reason.push(c);
                    session.reject_input = Some(reason);
                }
                _ => session.reject_input = Some(reason),
            }
            continue;
        }

        let quitting = matches!(key.code, KeyCode::Char('q'));
        match key.code {
            KeyCode::Char(digit @ '1'..='4') => {
                let verdict = Verdict::ALL[digit as usize - '1' as usize];
                session.verdict = Some(verdict);
                session.status = format!("Verdict: {} (s to save)", verdict.label());
            }
            KeyCode::Char('s') => {
                session.status = match save(&mut session) {
                    Ok(message) => message,
                    Err(err) => format!("{err:#}"),
                };
            }
            KeyCode::Char('x') => {
                if session.promoted.is_some() {
                    session.status = "A promoted sample cannot be rejected".to_string();
                } else {
                    session.reject_input = Some(String::new());
                }
            }
            KeyCode::Char('e') => {
                let show = session.viewer.annotating();
                session.viewer.set_annotating(!show);
                session.status = if show {
                    format!(
                        "omnidiff says: {} (e hides it)",
                        session.viewer.verdict().label()
                    )
                } else {
                    "omnidiff's view hidden".to_string()
                };
            }
            KeyCode::Char('O') => open_sample_picker(app),
            KeyCode::Char('q') => {
                if session.verdict != session.saved && !quit_armed {
                    session.status =
                        "The verdict is not saved: s saves it, q again quits anyway".to_string();
                    quit_armed = true;
                } else {
                    return Ok(SessionEnd::Quit);
                }
            }
            code => {
                session.viewer.handle_key(code);
            }
        }
        if !quitting {
            quit_armed = false;
        }
    }
}

fn draw(frame: &mut ratatui::Frame, session: &mut PictureSession, app: &App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let state = match &session.promoted {
        Some(fixture) => format!("promoted to {fixture}"),
        None => "sample".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            session.name.as_str().bold(),
            format!("  ({state}, {})", session.source.path).dim(),
        ])),
        rows[0],
    );
    let status = if session.viewer.annotating() {
        Line::from(session.viewer.status())
    } else {
        Line::from(vec![
            format!("omnidiff: {}", session.viewer.verdict().label()).cyan(),
            format!(" · {}", session.viewer.status()).into(),
        ])
    };
    frame.render_widget(Paragraph::new(status), rows[1]);

    let mut choices: Vec<Span> = vec!["Verdict: ".into()];
    for (index, verdict) in Verdict::ALL.iter().enumerate() {
        let text = format!(" {} {} ", index + 1, verdict.label());
        choices.push(if session.verdict == Some(*verdict) {
            Span::styled(text, Style::new().add_modifier(Modifier::REVERSED))
        } else {
            text.into()
        });
        choices.push(" ".into());
    }
    if session.verdict.is_some() && session.verdict != session.saved {
        choices.push("(unsaved)".yellow());
    }

    frame.render_widget(Paragraph::new(Line::from(choices)), rows[2]);

    // The pictures are drawn as they look: no theme to follow, only the two outline colors the
    // annotation view never uses.
    let colors = PictureColors::from_theme(Color::Red, Color::Green, Color::Yellow);
    session.viewer.draw(frame, rows[3], colors);

    let prompt = match &session.reject_input {
        Some(reason) => format!("Reject because: {reason}_ (Enter rejects, Esc cancels)"),
        None => session.status.clone(),
    };
    frame.render_widget(Paragraph::new(prompt), rows[4]);
    frame.render_widget(
        Paragraph::new(
            "1-4 verdict  s promote/save  x reject  e omnidiff's view  t view  h/l swipe  O samples  q quit"
                .dim(),
        ),
        rows[5],
    );

    if let Some(Modal::OpenSamplePicker {
        rows,
        selected,
        view,
        name_input,
    }) = &app.modal
    {
        render_open_sample_picker(frame, area, rows, *selected, view, name_input.as_deref());
    }
}

/// Promotes the sample to a picture fixture with the current verdict, or saves a changed verdict
/// into the fixture it was promoted to. Either way the fixture's stub is rewritten to match what
/// the engine says now.
fn save(session: &mut PictureSession) -> Result<String> {
    let Some(verdict) = session.verdict else {
        bail!("No verdict yet: 1-4 records one");
    };
    let picture = HumanPicture { verdict };
    let fixture = match &session.promoted {
        Some(fixture) => {
            human_picture::save(fixture, &picture)?;
            fixture.clone()
        }
        None => {
            let fixture = session.name.clone();
            promote(&session.name, &fixture, &picture)?;
            if !update_sample_csv(&session.source, &fixture)? {
                bail!("promoted to '{fixture}', but its sample.csv row was not found");
            }
            session.promoted = Some(fixture.clone());
            fixture
        }
    };
    session.saved = Some(verdict);
    let engine = human_picture::engine_verdict(&fixture)?;
    write_stub(&fixture, (engine != verdict).then_some(engine))?;
    Ok(if engine == verdict {
        format!("Saved '{fixture}': {} (omnidiff agrees)", verdict.label())
    } else {
        format!(
            "Saved '{fixture}': {} (omnidiff says {}; recorded in the stub)",
            verdict.label(),
            engine.label()
        )
    })
}

/// Copies sample `sample`'s pair and README into `src/test/data/pictures/<fixture>/` and writes
/// its verdict there.
fn promote(sample: &str, fixture: &str, picture: &HumanPicture) -> Result<()> {
    let dir = human_picture::pictures_root().join(fixture);
    if dir.exists() {
        bail!("{dir:?} already exists");
    }
    let from = samples_root().join(sample);
    let (before, after) =
        pair_paths(&from).with_context(|| format!("{from:?} has no before/after pair"))?;
    fs::create_dir_all(&dir).with_context(|| format!("creating {dir:?}"))?;
    for file in [before, after, from.join("README.md")] {
        let name = file.file_name().context("a file name")?;
        fs::copy(&file, dir.join(name)).with_context(|| format!("copying {file:?}"))?;
    }
    human_picture::save(fixture, picture)
}

/// `fixtures/pictures/<fixture>.rs`: `assert_matches_human_verdict`, or, when the engine
/// disagrees, `assert_known_verdict_mismatch` pinned to what it says now. Rewritten on every
/// save: a verdict stub holds nothing written by hand.
fn write_stub(fixture: &str, mismatch: Option<Verdict>) -> Result<()> {
    let module = module_name(fixture);
    let dir = fixtures_dir(PICTURE_DATASET);
    fs::create_dir_all(&dir).with_context(|| format!("creating {dir:?}"))?;
    let path = dir.join(format!("{module}.rs"));
    fs::write(&path, stub_contents(fixture, mismatch))
        .with_context(|| format!("writing {path:?}"))?;
    // Best effort, as for the tree stubs: the file is valid either way.
    let _ = std::process::Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(&path)
        .status();
    insert_mod_declaration(PICTURE_DATASET, &module)
}

/// The stub `write_stub` writes, without touching the filesystem.
pub(crate) fn stub_contents(fixture: &str, mismatch: Option<Verdict>) -> String {
    let body = match mismatch {
        None => format!("    human_picture::assert_matches_human_verdict(\"{fixture}\")"),
        Some(found) => format!(
            "    // Recorded as found, not examined.\n    human_picture::assert_known_verdict_mismatch(\n        \"{fixture}\",\n        human_picture::Verdict::{found:?},\n    )"
        ),
    };
    format!(
        "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test::helper::human_picture;\n\n#[test]\nfn verdict() -> Result<()> {{\n{body}\n}}\n"
    )
}

fn reject(session: &PictureSession, reason: &str) -> Result<String> {
    let reason = reason.trim();
    if reason.is_empty() {
        bail!("Rejection reason cannot be empty");
    }
    match reject_sample(&session.source, reason)? {
        true => Ok(format!("Rejected '{}': {reason}", session.name)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CaseOrigin;
    use omnidiff::test::helper::human_mapping::HumanMapping;

    #[test]
    fn the_session_shows_the_verdicts_and_nothing_the_engine_decided() {
        let dir = tempfile::tempdir().unwrap();
        let mut after = image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 255, 255, 255]));
        let before = after.clone();
        after.put_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        let (a, b) = (
            dir.path().join("before.png.test"),
            dir.path().join("after.png.test"),
        );
        before
            .save_with_format(&a, image::ImageFormat::Png)
            .unwrap();
        after.save_with_format(&b, image::ImageFormat::Png).unwrap();

        let mut session = PictureSession {
            name: "png-x-repo-1234abcd-logo".to_string(),
            source: SampleSource {
                language: "PNG".to_string(),
                repository: "repo".to_string(),
                commit: "1234abcd".to_string(),
                path: "assets/logo.png".to_string(),
                dataset: PICTURE_DATASET.to_string(),
            },
            promoted: None,
            viewer: PictureViewer::open_for_annotation(&a, &b, Picker::halfblocks()).unwrap(),
            verdict: Some(Verdict::ContentChange),
            saved: None,
            reject_input: None,
            status: String::new(),
        };
        let app = App::new(
            "x".to_string(),
            CaseOrigin::Diffs,
            0,
            0,
            HumanMapping::default(),
        );
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 16)).unwrap();
        terminal
            .draw(|frame| draw(frame, &mut session, &app))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("png-x-repo-1234abcd-logo"), "{screen}");
        assert!(screen.contains("PNG 8x4 RGBA8"), "{screen}");
        assert!(screen.contains("1 content change"), "{screen}");
        assert!(screen.contains("4 replaced"), "{screen}");
        assert!(screen.contains("(unsaved)"), "{screen}");
        assert!(
            !screen.contains("changed"),
            "no engine verdict on screen: {screen}"
        );

        session.viewer.set_annotating(false);
        terminal
            .draw(|frame| draw(frame, &mut session, &app))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("omnidiff: content change"), "{screen}");
        assert!(screen.contains("of pixels changed"), "{screen}");
    }
}
