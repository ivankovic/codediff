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

//! The TUI's color depth, end to end: the real binary on a pseudo-terminal, in the environment a
//! terminal sets, and the escape sequences it actually writes.
//!
//! A terminal without 24-bit support misreads `ESC[48;2;R;G;Bm` (macOS Terminal.app shows
//! `#ff0000` as black), so the property is byte-level: no 24-bit sequence unless the environment
//! advertises 24-bit color. `tui::color_depth`'s unit tests cover the mapping; this covers that
//! detection reads the real environment and that every frame goes through the fitting.
//!
//! Linux only: the pseudo-terminal comes from util-linux `script`, whose flags BSD `script` (macOS)
//! does not share.
#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The last text of the viewer's footer. Ratatui skips a blank cell it has nothing to change in,
/// so only a run without spaces is sure to arrive contiguous; and it writes rows top to bottom, so
/// once this is out the whole first frame is.
const FRAME_DRAWN: &str = "q:quit";

/// Long enough for a debug build to diff the fixture, short enough that a hang fails the test.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Everything codediff writes to a pseudo-terminal while showing the README's example diff, with
/// the environment cleared down to `TERM`, `COLORTERM` if given, and what keeps it off the real
/// config. Quits with `q` once the first frame is out.
fn run_viewer(term: &str, colorterm: Option<&str>) -> String {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/test/data/diffs/handmade/python-refactoring");
    let home = tempfile::tempdir().expect("temp dir");

    let mut command = Command::new("script");
    // -q: no "Script started" banner, -f: flush every write, -e: exit with codediff's status,
    // -c: the command. The typescript file is not needed; `script` copies the output to stdout.
    command
        .args([
            "-qfec",
            r#"stty cols 230 rows 30; exec "$CODEDIFF" "$BEFORE" "$AFTER""#,
            "/dev/null",
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home.path())
        .env("CODEDIFF_CONFIG", home.path().join("config.toml"))
        .env("TERM", term)
        .env("CODEDIFF", env!("CARGO_BIN_EXE_codediff"))
        .env("BEFORE", fixture.join("before.py.test"))
        .env("AFTER", fixture.join("after.py.test"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(colorterm) = colorterm {
        command.env("COLORTERM", colorterm);
    }
    let mut child = command
        .spawn()
        .expect("util-linux `script` provides the pseudo-terminal; is it installed?");

    let mut stdout = child.stdout.take().expect("piped stdout");
    let (sender, chunks) = mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read) = stdout.read(&mut chunk) {
            if read == 0 || sender.send(chunk[..read].to_vec()).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + TIMEOUT;
    let mut output = Vec::new();
    let mut quit_sent = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match chunks.recv_timeout(remaining) {
            Ok(chunk) => output.extend(chunk),
            // The reader is done: codediff has exited and `script` with it.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                panic!(
                    "no {} within {TIMEOUT:?}; codediff wrote:\n{}",
                    if quit_sent {
                        "exit after q"
                    } else {
                        "first frame"
                    },
                    String::from_utf8_lossy(&output).escape_debug()
                );
            }
        }
        if !quit_sent && String::from_utf8_lossy(&output).contains(FRAME_DRAWN) {
            let stdin = child.stdin.as_mut().expect("piped stdin");
            stdin.write_all(b"q").expect("send q");
            stdin.flush().expect("flush q");
            quit_sent = true;
        }
    }
    let status = child.wait().expect("wait for script");
    assert!(status.success(), "codediff exited with {status}");
    String::from_utf8_lossy(&output).into_owned()
}

/// The colors in the output's SGR sequences.
#[derive(Debug, Default)]
struct Colors {
    /// `38;2;R;G;B` and `48;2;R;G;B`.
    true_color: usize,
    /// The `N` of every `38;5;N` and `48;5;N`.
    indexed: Vec<u8>,
}

fn colors(output: &str) -> Colors {
    let mut colors = Colors::default();
    for sequence in output.split("\u{1b}[").skip(1) {
        let Some(end) = sequence.find(|c: char| !c.is_ascii_digit() && c != ';') else {
            continue;
        };
        if !sequence[end..].starts_with('m') {
            continue;
        }
        let params: Vec<&str> = sequence[..end].split(';').collect();
        let mut i = 0;
        while i < params.len() {
            match (params[i], params.get(i + 1)) {
                ("38" | "48", Some(&"2")) => {
                    colors.true_color += 1;
                    i += 5;
                }
                ("38" | "48", Some(&"5")) => {
                    if let Some(index) = params.get(i + 2).and_then(|n| n.parse().ok()) {
                        colors.indexed.push(index);
                    }
                    i += 3;
                }
                _ => i += 1,
            }
        }
    }
    colors
}

/// Terminal.app's environment: `TERM=xterm-256color` and no `COLORTERM`.
#[test]
fn a_terminal_that_does_not_advertise_24_bit_color_gets_only_256_colors() {
    let colors = colors(&run_viewer("xterm-256color", None));
    assert_eq!(
        colors.true_color, 0,
        "24-bit color sent to a terminal that did not advertise it"
    );
    // Else an empty screen would pass: the bands must be there, fitted to the cube or ramp.
    assert!(
        colors.indexed.iter().any(|&index| index >= 16),
        "no 256-color sequences at all; were the diff bands drawn? {colors:?}"
    );
}

#[test]
fn a_terminal_that_advertises_24_bit_color_gets_it() {
    for colorterm in ["truecolor", "24bit"] {
        let colors = colors(&run_viewer("xterm-256color", Some(colorterm)));
        assert!(
            colors.true_color > 0,
            "COLORTERM={colorterm}: no 24-bit color sent"
        );
    }
}

#[test]
fn the_sgr_scan_reads_both_forms_and_skips_other_parameters() {
    let scanned = colors("\u{1b}[1;38;5;196m x \u{1b}[48;2;0;5;38m y \u{1b}[0m \u{1b}[2J");
    assert_eq!(scanned.true_color, 1);
    assert_eq!(scanned.indexed, vec![196]);
}
