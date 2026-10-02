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
//! hides it again; each sample starts with it hidden. `s` promotes the sample to
//! `src/test/data/pictures/<name>/` with its verdict and a `verdict()` stub, or, once promoted,
//! saves a changed verdict; `x` rejects it with a reason.
//!
//! A picture fixture is opened from `o`, which lists them beside the code cases (dataset
//! `pictures`): its verdict is changed and saved the same way, and there is nothing to promote or
//! reject. `o` and `O` work here as in the tree session, which takes back over for a code case or
//! sample.
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
use omnidiff::tui::components::picture_viewer::{self, PictureColors, PictureViewer};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;

use crate::events::{
    SessionEnd, handle_open_diff_picker, handle_open_sample_picker, open_diff_picker,
    open_sample_picker, reject_sample, update_sample_csv,
};
use crate::render::{render_open_diff_picker, render_open_sample_picker};
use crate::state::{App, Modal, OpenTarget};
use crate::stubs::{LICENSE_HEADER, fixtures_dir, insert_mod_declaration, module_name};
use crate::{
    DiffPickerData, SampleSource, promoted_case_name, samples_root, source_json_for_sample,
};

/// Where promoted picture samples go, and the stub module that lists them.
pub(crate) const PICTURE_DATASET: &str = "pictures";

/// True if sample `name` is a pair of pictures: its `before.<ext>.test` names a picture format.
pub(crate) fn is_picture_sample(name: &str) -> bool {
    pair_paths(&samples_root().join(name))
        .is_some_and(|(before, _)| omnidiff::diff::picture::is_picture_path(&before))
}

/// True if `name` is a picture fixture: a directory of `src/test/data/pictures/` holding a pair.
fn is_picture_fixture(name: &str) -> bool {
    pair_paths(&human_picture::pictures_root().join(name))
        .is_some_and(|(before, _)| omnidiff::diff::picture::is_picture_path(&before))
}

/// What a picture session judges.
pub(crate) enum PictureCase {
    /// A picture sample, from `O`.
    Sample(String),
    /// A picture fixture, from `o`.
    Fixture(String),
}

impl PictureCase {
    /// The picture case `end` opens, if it opens one rather than a code case or sample.
    pub(crate) fn opened_by(end: &SessionEnd) -> Option<PictureCase> {
        match end {
            SessionEnd::Open(OpenTarget::Sample(name)) if is_picture_sample(name) => {
                Some(PictureCase::Sample(name.clone()))
            }
            SessionEnd::Open(OpenTarget::Diffs(name)) if is_picture_fixture(name) => {
                Some(PictureCase::Fixture(name.clone()))
            }
            _ => None,
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            PictureCase::Sample(name) | PictureCase::Fixture(name) => name,
        }
    }
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
        .get_or_init(|| picture_viewer::query_graphics().unwrap_or_else(Picker::halfblocks))
        .clone()
}

/// Where the pair being judged lives, and so what `s` and `x` do.
enum Origin {
    /// A sample, and the picture fixture it was promoted to once it is.
    Sample {
        source: SampleSource,
        promoted: Option<String>,
    },
    /// A picture fixture opened from `o`.
    Fixture(String),
}

impl Origin {
    /// The picture fixture a verdict is saved into, once there is one.
    fn fixture(&self) -> Option<&str> {
        match self {
            Origin::Sample { promoted, .. } => promoted.as_deref(),
            Origin::Fixture(fixture) => Some(fixture),
        }
    }
}

struct PictureSession {
    name: String,
    origin: Origin,
    /// The picture's path in its repository, for the title line.
    path: String,
    viewer: PictureViewer,
    verdict: Option<Verdict>,
    /// The verdict on disk, to warn before quitting with an unsaved one.
    saved: Option<Verdict>,
    /// A rejection reason being typed, after `x`.
    reject_input: Option<String>,
    /// `q` was pressed with an unsaved verdict, and was warned about it: another `q` quits.
    quit_armed: bool,
    status: String,
}

/// Runs the picture session for `case` until the human quits or opens something else.
pub(crate) fn run_picture_session(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    case: &PictureCase,
) -> Result<SessionEnd> {
    let (dir, origin) = match case {
        PictureCase::Sample(name) => {
            let dir = samples_root().join(name);
            let source = source_json_for_sample(name)
                .with_context(|| format!("{dir:?} has no readable source.json"))?;
            let promoted = promoted_case_name(&source);
            (dir, Origin::Sample { source, promoted })
        }
        PictureCase::Fixture(name) => (
            human_picture::pictures_root().join(name),
            Origin::Fixture(name.clone()),
        ),
    };
    let path = match &origin {
        Origin::Sample { source, .. } => source.path.clone(),
        Origin::Fixture(_) => readme_file(&dir).unwrap_or_default(),
    };
    let (before, after) =
        pair_paths(&dir).with_context(|| format!("{dir:?} has no before/after pair"))?;
    let viewer = PictureViewer::open_for_annotation(&before, &after, picker())
        .with_context(|| format!("{dir:?} is not a pair of pictures that decode"))?;
    let saved = origin
        .fixture()
        .and_then(|fixture| human_picture::load(fixture).ok())
        .map(|picture| picture.verdict);
    let mut session = PictureSession {
        name: case.name().to_string(),
        origin,
        path,
        viewer,
        verdict: saved,
        saved,
        reject_input: None,
        quit_armed: false,
        status: "1-4 records a verdict; s saves it".to_string(),
    };
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

        if let Some(end) = handle_key(&mut session, app, key.code) {
            return Ok(end);
        }
    }
}

/// One key of the picture session; `Some` when it ends the session.
fn handle_key(session: &mut PictureSession, app: &mut App, code: KeyCode) -> Option<SessionEnd> {
    // The `o` and `O` pickers, while open, take every key. Any other modal is the tree
    // session's and has no place here.
    let picked = match app.modal.take() {
        Some(Modal::OpenDiffPicker {
            options,
            selected,
            view,
            name_input,
        }) => Some(without_unsaved(app, |app| {
            handle_open_diff_picker(app, code, options, selected, view, name_input)
        })),
        Some(Modal::OpenSamplePicker {
            rows,
            selected,
            view,
            name_input,
        }) => Some(without_unsaved(app, |app| {
            handle_open_sample_picker(app, code, rows, selected, view, name_input)
        })),
        _ => None,
    };
    if let Some(target) = picked {
        return target.map(SessionEnd::Open);
    }

    if let Some(mut reason) = session.reject_input.take() {
        match code {
            KeyCode::Enter => {
                session.status = match reject(session, &reason) {
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
        return None;
    }

    let quitting = matches!(code, KeyCode::Char('q'));
    match code {
        KeyCode::Char(digit @ '1'..='4') => {
            let verdict = Verdict::ALL[digit as usize - '1' as usize];
            session.verdict = Some(verdict);
            session.status = format!("Verdict: {} (s to save)", verdict.label());
        }
        KeyCode::Char('s') => {
            session.status = match save(session) {
                Ok(message) => message,
                Err(err) => format!("{err:#}"),
            };
        }
        KeyCode::Char('x') => match &session.origin {
            Origin::Sample { promoted: None, .. } => {
                session.reject_input = Some(String::new());
            }
            Origin::Sample { .. } => {
                session.status = "A promoted sample cannot be rejected".to_string();
            }
            Origin::Fixture(_) => {
                session.status = "A fixture cannot be rejected".to_string();
            }
        },
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
        KeyCode::Char('o') => open_diff_picker(app, &session.name),
        KeyCode::Char('O') => open_sample_picker(app, &session.name),
        KeyCode::Char('q') => {
            if session.verdict != session.saved && !session.quit_armed {
                session.status =
                    "The verdict is not saved: s saves it, q again quits anyway".to_string();
                session.quit_armed = true;
            } else {
                return Some(SessionEnd::Quit);
            }
        }
        code => {
            session.viewer.handle_key(code);
        }
    }
    if !quitting {
        session.quit_armed = false;
    }
    None
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

    let state = match &session.origin {
        Origin::Sample {
            promoted: Some(fixture),
            ..
        } => format!("promoted to {fixture}"),
        Origin::Sample { promoted: None, .. } => "sample".to_string(),
        Origin::Fixture(_) => "fixture".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            session.name.as_str().bold(),
            format!("  ({state}, {})", session.path).dim(),
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
    let keys = match session.origin {
        Origin::Fixture(_) => "1-4 verdict  s save",
        Origin::Sample { .. } => "1-4 verdict  s promote/save  x reject",
    };
    frame.render_widget(
        Paragraph::new(
            format!("{keys}  e omnidiff's view  t view  h/l swipe  o cases  O samples  q quit")
                .dim(),
        ),
        rows[5],
    );

    match &app.modal {
        Some(Modal::OpenDiffPicker {
            options,
            selected,
            view,
            name_input,
        }) => render_open_diff_picker(
            frame,
            area,
            options,
            *selected,
            view,
            name_input.as_deref(),
            DiffPickerData::from_app(app),
            app.diff_comments.as_ref(),
        ),
        Some(Modal::OpenSamplePicker {
            rows,
            selected,
            view,
            name_input,
        }) => render_open_sample_picker(frame, area, rows, *selected, view, name_input.as_deref()),
        _ => {}
    }
}

/// Runs a picker's key handler as if the tree session's case had nothing unsaved. The pickers
/// would otherwise ask whether to discard it, in a modal this session cannot show; and the question
/// was already answered, by the picker that opened this session.
fn without_unsaved<T>(app: &mut App, handle: impl FnOnce(&mut App) -> T) -> T {
    let dirty = std::mem::replace(&mut app.dirty, false);
    let result = handle(app);
    app.dirty = dirty;
    result
}

/// The `File` line of a picture fixture's README: the picture's path in its repository.
fn readme_file(dir: &Path) -> Option<String> {
    let readme = fs::read_to_string(dir.join("README.md")).ok()?;
    readme.lines().find_map(|line| {
        let file = line.strip_prefix("- **File:** `")?;
        Some(file.strip_suffix('`')?.to_string())
    })
}

/// Promotes the sample to a picture fixture with the current verdict, or saves a changed verdict
/// into the fixture it was promoted to or opened as. Either way the fixture's stub is rewritten to match what
/// the engine says now.
fn save(session: &mut PictureSession) -> Result<String> {
    let Some(verdict) = session.verdict else {
        bail!("No verdict yet: 1-4 records one");
    };
    let picture = HumanPicture { verdict };
    let fixture = match &mut session.origin {
        Origin::Fixture(fixture)
        | Origin::Sample {
            promoted: Some(fixture),
            ..
        } => {
            human_picture::save(fixture, &picture)?;
            fixture.clone()
        }
        Origin::Sample {
            source,
            promoted: promoted @ None,
        } => {
            let fixture = session.name.clone();
            promote(&session.name, &fixture, &picture)?;
            if !update_sample_csv(source, &fixture)? {
                bail!("promoted to '{fixture}', but its sample.csv row was not found");
            }
            *promoted = Some(fixture.clone());
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
    let Origin::Sample { source, .. } = &session.origin else {
        bail!("A fixture cannot be rejected");
    };
    let reason = reason.trim();
    if reason.is_empty() {
        bail!("Rejection reason cannot be empty");
    }
    match reject_sample(source, reason)? {
        true => Ok(format!("Rejected '{}': {reason}", session.name)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CaseOrigin;
    use omnidiff::test::helper::human_mapping::HumanMapping;

    /// A session over a generated pair in `dir`: white 8x4 pictures, one pixel apart.
    fn session_over(dir: &Path, name: &str, origin: Origin, path: &str) -> PictureSession {
        let mut after = image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 255, 255, 255]));
        let before = after.clone();
        after.put_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        let (a, b) = (dir.join("before.png.test"), dir.join("after.png.test"));
        before
            .save_with_format(&a, image::ImageFormat::Png)
            .unwrap();
        after.save_with_format(&b, image::ImageFormat::Png).unwrap();
        PictureSession {
            name: name.to_string(),
            origin,
            path: path.to_string(),
            viewer: PictureViewer::open_for_annotation(&a, &b, Picker::halfblocks()).unwrap(),
            verdict: None,
            saved: None,
            reject_input: None,
            quit_armed: false,
            status: String::new(),
        }
    }

    fn sample_origin() -> Origin {
        Origin::Sample {
            source: SampleSource {
                language: "PNG".to_string(),
                repository: "repo".to_string(),
                commit: "1234abcd".to_string(),
                path: "assets/logo.png".to_string(),
                dataset: PICTURE_DATASET.to_string(),
            },
            promoted: None,
        }
    }

    fn test_app() -> App {
        App::new(
            "x".to_string(),
            CaseOrigin::Diffs,
            0,
            0,
            HumanMapping::default(),
        )
    }

    fn screen(session: &mut PictureSession, app: &App, width: u16) -> String {
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, 16)).unwrap();
        terminal.draw(|frame| draw(frame, session, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// The bug this guards: `o` fell through to the viewer, which ignores it, so the only way back
    /// to a code case was `O` or a restart.
    #[test]
    fn o_opens_the_case_picker_and_esc_hands_the_keys_back() {
        // A real fixture, so the picker has its row to put the cursor on.
        let fixture = crate::list_dir_names(&human_picture::pictures_root())
            .unwrap()
            .pop()
            .expect("at least one picture fixture");
        let dir = tempfile::tempdir().unwrap();
        let mut session = session_over(
            dir.path(),
            &fixture,
            Origin::Fixture(fixture.clone()),
            "logo.png",
        );
        let mut app = test_app();

        assert!(handle_key(&mut session, &mut app, KeyCode::Char('o')).is_none());
        let Some(Modal::OpenDiffPicker {
            options, selected, ..
        }) = &app.modal
        else {
            panic!("o must open the case picker, got {:?}", app.modal);
        };
        let visible =
            crate::visible_diff_options(options, &app.diff_view, DiffPickerData::from_app(&app));
        assert_eq!(
            visible[*selected], fixture,
            "the cursor starts on the picture being judged, not the tree session's case"
        );
        assert!(
            options
                .iter()
                .any(|(_, dataset)| *dataset == PICTURE_DATASET),
            "the picture fixtures are listed"
        );
        assert!(
            options
                .iter()
                .any(|(_, dataset)| *dataset != PICTURE_DATASET),
            "and so are the code cases"
        );
        assert!(
            screen(&mut session, &app, 160).contains("Open diff ["),
            "the picker is drawn over the pictures"
        );

        assert!(handle_key(&mut session, &mut app, KeyCode::Esc).is_none());
        assert!(app.modal.is_none());
        handle_key(&mut session, &mut app, KeyCode::Char('3'));
        assert_eq!(
            session.verdict,
            Some(Verdict::ALL[2]),
            "with the picker closed, keys reach the session again"
        );
    }

    /// The tree case left behind was already answered for by the picker that opened this session;
    /// asking again would raise a modal this session cannot show.
    #[test]
    fn enter_in_the_case_picker_opens_the_case_over_an_unsaved_tree_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = session_over(
            dir.path(),
            "png-x-repo-1234abcd-logo",
            sample_origin(),
            "assets/logo.png",
        );
        let mut app = test_app();
        app.dirty = true;
        app.modal = Some(Modal::OpenDiffPicker {
            options: vec![("png-a".to_string(), PICTURE_DATASET)],
            selected: 0,
            view: Default::default(),
            name_input: None,
        });

        let end = handle_key(&mut session, &mut app, KeyCode::Enter);
        assert!(
            matches!(&end, Some(SessionEnd::Open(OpenTarget::Diffs(name))) if name == "png-a"),
            "Enter opens the selected case"
        );
        assert!(app.dirty, "the tree session's own flag is left as it was");
    }

    #[test]
    fn opened_by_sends_picture_fixtures_to_the_picture_session_and_code_cases_back() {
        let fixture = crate::list_dir_names(&human_picture::pictures_root())
            .unwrap()
            .into_iter()
            .next()
            .expect("at least one picture fixture");
        let code = crate::list_available_cases().unwrap()[0].0.clone();

        let open = |name: &str| SessionEnd::Open(OpenTarget::Diffs(name.to_string()));
        assert!(matches!(
            PictureCase::opened_by(&open(&fixture)),
            Some(PictureCase::Fixture(name)) if name == fixture
        ));
        assert!(PictureCase::opened_by(&open(&code)).is_none());
        assert!(PictureCase::opened_by(&SessionEnd::Quit).is_none());
    }

    #[test]
    fn a_fixture_shows_its_repository_path_and_has_nothing_to_promote_or_reject() {
        let fixture = "bmp-x-talamus-solarize-12x29-psf-8a856fdb-solarize-12x29";
        let path = readme_file(&human_picture::pictures_root().join(fixture));
        assert_eq!(path.as_deref(), Some("Solarize.12x29.bmp"));

        let dir = tempfile::tempdir().unwrap();
        let mut session = session_over(
            dir.path(),
            fixture,
            Origin::Fixture(fixture.to_string()),
            &path.unwrap(),
        );
        let mut app = test_app();
        let text = screen(&mut session, &app, 160);
        assert!(text.contains("(fixture, Solarize.12x29.bmp)"), "{text}");
        assert!(text.contains("1-4 verdict  s save  e"), "{text}");
        assert!(!text.contains("reject"), "{text}");

        handle_key(&mut session, &mut app, KeyCode::Char('x'));
        assert!(session.reject_input.is_none());
        assert_eq!(session.status, "A fixture cannot be rejected");
    }

    #[test]
    fn the_session_shows_the_verdicts_and_nothing_the_engine_decided() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = session_over(
            dir.path(),
            "png-x-repo-1234abcd-logo",
            sample_origin(),
            "assets/logo.png",
        );
        session.verdict = Some(Verdict::ContentChange);
        let app = test_app();
        let screen_text = screen(&mut session, &app, 100);
        assert!(
            screen_text.contains("png-x-repo-1234abcd-logo"),
            "{screen_text}"
        );
        assert!(screen_text.contains("PNG 8x4 RGBA8"), "{screen_text}");
        assert!(screen_text.contains("1 content change"), "{screen_text}");
        assert!(screen_text.contains("4 replaced"), "{screen_text}");
        assert!(screen_text.contains("(unsaved)"), "{screen_text}");
        assert!(
            !screen_text.contains("changed"),
            "no engine verdict on screen: {screen_text}"
        );

        session.viewer.set_annotating(false);
        let screen_text = screen(&mut session, &app, 100);
        assert!(
            screen_text.contains("omnidiff: content change"),
            "{screen_text}"
        );
        assert!(screen_text.contains("of pixels changed"), "{screen_text}");
    }
}
