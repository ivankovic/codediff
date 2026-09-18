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

//! The GitHub Pages showcase: the real browser viewer, running on static files.
//!
//! `codediff-web`'s page (`assets/web/app.js` + `model.js`) never computes a diff itself; it asks
//! its server for one as JSON (`web::payload::DiffPayload`) and paints what it gets. That makes it
//! a static site waiting to happen: bake the JSON for a fixed list of changes at build time, serve
//! the same page and scripts as files, and answer the page's `/api/*` calls from those files with
//! a `fetch` shim (`assets/showcase/showcase.js`). Nothing runs on the server, and the viewer the
//! reader gets is the product's own, keys and all, not a screenshot of it.
//!
//! Every case is baked twice from the same two files: once as codediff maps it, once as Unix
//! `diff` marks it (`human_mapping::unix_diff_line_labels`, the real GNU diff, whole touched lines
//! as deletions and insertions), so the reader can flip between the two on the same code. The
//! list is hand-picked in [`CASES`]: ten changes where `diff` marks lines a reader has to re-diff
//! by eye and codediff's mapping matches the human one exactly, and ten where a plain line diff
//! is already the right answer and codediff agrees. "Exactly" and "agrees" are the line-level
//! agreement scores in `research/data/comparison/benchmark_other.csv`, the same measurement the
//! introductory paper reports; the list was drawn from the rows where codediff has zero mismatched
//! lines.
//!
//! Published by `.github/workflows/pages.yml` next to the human-mapping site, under `showcase/`.
//! Nothing this binary produces is committed.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use serde::Serialize;

use codediff::diff::text::{RangeMatch, RenderOptions, TextOperation};
use codediff::diff::text_range::TextRange;
use codediff::test::helper;
use codediff::test::helper::human_mapping;
use codediff::tui::actions::DiffSessionData;
use codediff::tui::app::compute_diff_with_options;
use codediff::tui::theme::{CustomPalette, OverlayTheme, PanelLayout};
use codediff::web::payload::{DiffPayload, RangePayload, diff_payload};
use codediff::web::session::Session;

#[derive(Parser)]
struct Args {
    /// Directory to write the showcase into. Wiped and recreated on every run.
    #[arg(long, default_value = "site/showcase")]
    out: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Group {
    /// `diff` marks whole lines a reader has to re-diff by eye; codediff's mapping matches the
    /// human one line for line.
    DiffWrong,
    /// A plain line diff is already the right answer, and codediff says the same thing.
    BothRight,
}

struct Case {
    /// A fixture directory name under `src/test/data/diffs/<dataset>/`.
    name: &'static str,
    group: Group,
    title: &'static str,
    /// What changed and what each tool makes of it, for the reader who has not looked yet.
    blurb: &'static str,
}

/// The twenty. Order is display order within each group.
const CASES: &[Case] = &[
    Case {
        name: "rust-add-if",
        group: Group::DiffWrong,
        title: "Wrap existing code in a new branch",
        blurb: "An `if` grows an `else if` in front of it. `diff` reports the old condition deleted \
                and three lines inserted; codediff shows the old branch as moved and marks only \
                the new one as inserted.",
    },
    Case {
        name: "python-refactoring",
        group: Group::DiffWrong,
        title: "Replace two loops with built-ins",
        blurb: "Hand-rolled sum/count and max/min loops become `sum`, `len`, `max` and `min`. \
                `diff` shows a block deleted and a block inserted; codediff keeps the four \
                assignments and shows what each right-hand side became.",
    },
    Case {
        name: "typescript-add-type-annotations",
        group: Group::DiffWrong,
        title: "Add type annotations",
        blurb: "A function signature gains parameter and return types and an interface appears \
                above it. `diff` marks the signature line deleted and re-inserted; codediff marks \
                the annotations and the interface as inserted, and shades what merely shifted to \
                make room for them as moved.",
    },
    Case {
        name: "c-genymobile-scrcpy-rename-defines",
        group: Group::DiffWrong,
        title: "Prefix five macro names",
        blurb: "Five `#define`s get an `SC_` prefix. `diff` marks every line deleted and inserted; \
                codediff marks five identifiers as updated.",
    },
    Case {
        name: "lua-neovim-neovim-rename",
        group: Group::DiffWrong,
        title: "Rename a field in three tables",
        blurb: "`buffer = 0` becomes `buf = 0` in three keymap option tables. `diff` marks the \
                three long lines; codediff marks the three words.",
    },
    Case {
        name: "csharp-radarr-radarr-remove-import-and-func",
        group: Group::DiffWrong,
        title: "Delete a method and its import",
        blurb: "One `using` and one method go away. `diff` also charges the blank line before the \
                method; codediff deletes exactly the method and the import.",
    },
    Case {
        name: "python-nvbn-thefuck-add-three-arguments",
        group: Group::DiffWrong,
        title: "Add a parameter and pass it through",
        blurb: "A function gains an `expanded` parameter and forwards it in two calls. `diff` \
                marks three whole lines; codediff marks the three new arguments.",
    },
    Case {
        name: "yaml-twbs-bootstrap-remove-v-semicolon-from-version-numbers",
        group: Group::DiffWrong,
        title: "Drop a key from every list item",
        blurb: "Fifty-odd `- v: \"x.y.z\"` entries become `- \"x.y.z\"`. `diff` marks every \
                line on both sides; codediff keeps every version string and shows the key that \
                was removed from each.",
    },
    Case {
        name: "ruby-jekyll-jekyll-whitespace-only",
        group: Group::DiffWrong,
        title: "Change line endings only",
        blurb: "The file was re-saved with different line endings. `diff` marks every line in \
                the file; codediff reports a whitespace-only change and marks nothing.",
    },
    Case {
        name: "xml-antlr-antlr3-comment-out-part-of-code-interesting-case",
        group: Group::DiffWrong,
        title: "Comment out a block of XML",
        blurb: "A Maven plugin declaration is disabled by wrapping it in a comment. `diff` \
                reports one line replaced and one inserted and shows the plugin itself as \
                untouched; codediff reports the plugin gone and a comment in its place, which is \
                what a reviewer has to notice.",
    },
    Case {
        name: "go-gin-gonic-gin-update-version-string",
        group: Group::BothRight,
        title: "Bump a version string",
        blurb: "`\"v1.6.0\"` becomes `\"v1.6.1\"`. One line, one string; both tools agree.",
    },
    Case {
        name: "java-genymobile-scrcpy-char-to-string-bugfix",
        group: Group::BothRight,
        title: "Fix a char literal that should have been a string",
        blurb: "`'\"'` becomes `\"'\"` in an error message. Both tools mark the one line; codediff \
                marks the one literal.",
    },
    Case {
        name: "kotlin-fix-loop-bug",
        group: Group::BothRight,
        title: "Change a loop's range operator",
        blurb: "`0 until items.size` becomes `0..items.size`. Both tools see one changed line.",
    },
    Case {
        name: "cpp-ladybirdbrowser-ladybird-change-inherited-class-name",
        group: Group::BothRight,
        title: "Change a base class",
        blurb: "A constructor's initializer switches from `FormAssociatedLabelableNode` to \
                `ReplacedBox`. Both tools mark the line; codediff marks the name.",
    },
    Case {
        name: "swift-nextcloud-ios-different-func",
        group: Group::BothRight,
        title: "Rename an overridden method",
        blurb: "`createRightMenu` becomes `createOptionMenu`. Both tools mark the line; codediff \
                marks the name.",
    },
    Case {
        name: "tsx-mitmproxy-mitmproxy-array-to-object",
        group: Group::BothRight,
        title: "Export an object instead of an array",
        blurb: "`export default [OptionModal]` becomes a three-line object literal. The line \
                diff's delete-plus-insert is the right reading, and codediff reads it that way.",
    },
    Case {
        name: "rust-tauri-apps-tauri-add-use-and-function",
        group: Group::BothRight,
        title: "Add a field and the import for its type",
        blurb: "A struct gains a `work_area` field and the `use` line gains `PhysicalRect`. Two \
                touched lines, both tools agree on both.",
    },
    Case {
        name: "python-nvbn-thefuck-stdout-stderr-change",
        group: Group::BothRight,
        title: "Switch four reads from stderr to output",
        blurb: "`command.stderr` becomes `command.output` in four places. Both tools mark the \
                four lines; codediff marks the four attribute names.",
    },
    Case {
        name: "css-wordpress-wordpress-rename-attribute",
        group: Group::BothRight,
        title: "Change a CSS property",
        blurb: "`border-top` becomes `outline` with the same value. One line, both tools agree.",
    },
    Case {
        name: "shellscript-genymobile-scrcpy-insert-only",
        group: Group::BothRight,
        title: "Add one file to a release list",
        blurb: "One more line in a shell script's argument list. Pure insertion, the easy case \
                for every diff.",
    },
];

/// One entry of `cases.json`: what the page needs to list, label and link a case, plus the two
/// numbers the showcase is about.
#[derive(Serialize)]
struct CaseIndex {
    name: &'static str,
    group: Group,
    title: &'static str,
    blurb: &'static str,
    dataset: String,
    language: String,
    /// Lines in the after-side file.
    lines: usize,
    /// Lines GNU `diff` marks, both sides together - the number of lines a reader of its output
    /// is asked to look at.
    diff_marked: usize,
    /// What codediff paints on the same pair under the default options, per operation. Counted
    /// from the baked ranges rather than taken from the payload's `change_counts`, which is the
    /// footer's tally and says "0 updates" for a pure in-line deletion like `buffer` -> `buf`.
    codediff: PaintedCounts,
    /// codediff's one-line reading of the whole change, when it has one ("Whitespace changes
    /// only"), which is the verdict `diff` cannot give.
    summary: Option<String>,
    /// The upstream commit the change was taken from, when the fixture is a sampled one.
    upstream: Option<String>,
    /// The fixture's page on the human-mapping site next door.
    mapping: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.out.exists() {
        fs::remove_dir_all(&args.out)
            .with_context(|| format!("removing existing {:?}", args.out))?;
    }
    let cases_dir = args.out.join("cases");
    fs::create_dir_all(&cases_dir)?;

    // The viewer's own page assets, byte for byte, plus the showcase's shim and chrome. Embedded
    // so the generator is one self-contained binary, as generate_mapping_site is.
    for (name, contents) in [
        (
            "index.html",
            include_str!("../../assets/showcase/index.html"),
        ),
        (
            "showcase.js",
            include_str!("../../assets/showcase/showcase.js"),
        ),
        (
            "showcase.css",
            include_str!("../../assets/showcase/showcase.css"),
        ),
        ("model.js", include_str!("../../assets/web/model.js")),
        ("app.js", include_str!("../../assets/web/app.js")),
        ("style.css", include_str!("../../assets/web/style.css")),
    ] {
        fs::write(args.out.join(name), contents)?;
    }

    // What `/api/state` answers. `Session::from_config` reads whoever's `.codediff.toml` is on
    // this machine, so every setting a reader could notice is pinned here rather than inherited:
    // the site must look the same generated on CI or on a laptop. Dual layout rather than Auto,
    // because Auto's cut-over is the TUI's 220 terminal columns, which is single-panel on most
    // browser windows, and side by side is what a reader came to see (the layout key still
    // cycles it). A pair is "open" so the page starts diffing as soon as it loads; the paths are
    // placeholders, since the shim answers with the selected case whatever the page asks for.
    let mut state = Session::from_config(Some(RenderOptions::default())).state();
    state.before = Some("before".to_string());
    state.after = Some("after".to_string());
    state.settings.theme = OverlayTheme::default();
    state.settings.layout = PanelLayout::Dual;
    state.settings.node_highlight = false;
    state.settings.syntax_theme = None;
    state.settings.custom_palette = CustomPalette::from_palette(&state.settings.theme.palette());
    state.recent_pairs = Vec::new();
    state.config_error = None;
    fs::write(args.out.join("state.json"), serde_json::to_vec(&state)?)?;

    let provenance = helper::sample_provenance()?;
    let repository_urls = helper::repository_urls()?;

    let mut index = Vec::with_capacity(CASES.len());
    for case in CASES {
        let baked = bake(case, &provenance, &repository_urls)
            .with_context(|| format!("baking {}", case.name))?;
        for (suffix, payload) in [
            ("codediff", &baked.codediff),
            ("minimal", &baked.minimal),
            ("full", &baked.full),
            ("diff", &baked.unix),
        ] {
            fs::write(
                cases_dir.join(format!("{}.{suffix}.json", case.name)),
                serde_json::to_vec(payload)?,
            )?;
        }
        index.push(baked.index);
    }
    fs::write(args.out.join("cases.json"), serde_json::to_vec(&index)?)?;

    println!("Showcase written to {:?}: {} cases", args.out, index.len());
    Ok(())
}

struct Baked {
    /// codediff's diff under the default render options, and under each preset the `M` panel
    /// can switch to, so that switch works on the static site too.
    codediff: DiffPayload,
    minimal: DiffPayload,
    full: DiffPayload,
    /// The same two files as GNU `diff` marks them.
    unix: DiffPayload,
    index: CaseIndex,
}

fn bake(
    case: &Case,
    provenance: &HashMap<String, helper::SampleProvenance>,
    repository_urls: &HashMap<String, String>,
) -> Result<Baked> {
    let dir = helper::diffs_case_dir(case.name)
        .ok_or_else(|| anyhow!("no fixture directory named {}", case.name))?;
    let dataset = dir
        .parent()
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (before_path, after_path) = side_files(&dir)?;

    // The very computation codediff-web's server runs for `/api/diff`, then the same re-filter
    // it runs for `/api/render_options` under each preset.
    let (data, large_residual) =
        compute_diff_with_options(&before_path, &after_path, RenderOptions::default())?;
    let codediff = diff_payload(&data, large_residual, RenderOptions::default(), None);
    let minimal = diff_payload(&data, large_residual, RenderOptions::MINIMAL, None);
    let full = diff_payload(&data, large_residual, RenderOptions::FULL, None);

    // GNU diff's verdict over the same bytes, as whole-line deletions and insertions, poured
    // into the same session shape so the same page paints it.
    let pair = helper::handmade_test_code_pair(case.name)?;
    let (before_touched, after_touched) = human_mapping::unix_diff_line_labels(&pair.0, &pair.1)?;
    let (before_ranges, after_ranges) = unix_ranges(&before_touched, &after_touched);
    let unix_data = DiffSessionData {
        before_ranges,
        after_ranges,
        comment_only: false,
        plain_text_fallback: false,
        ..data.clone()
    };
    let mut unix = diff_payload(&unix_data, false, RenderOptions::FULL, None);
    // The summary line ("whitespace only", "comment only") is codediff's reading of the change;
    // `diff` has no such opinion, so its view carries none.
    unix.summary = None;

    let diff_marked = before_touched.iter().filter(|t| **t).count()
        + after_touched.iter().filter(|t| **t).count();
    let upstream = provenance
        .get(case.name)
        .and_then(|sample| helper::upstream_commit_url(sample, repository_urls));

    let index = CaseIndex {
        name: case.name,
        group: case.group,
        title: case.title,
        blurb: case.blurb,
        dataset,
        language: codediff.after.language.clone(),
        lines: codediff.after.lines.len(),
        diff_marked,
        codediff: PaintedCounts::of(&codediff),
        summary: codediff.summary.as_ref().map(|s| s.label.to_string()),
        upstream,
        mapping: format!("../fixtures/{}.html", case.name),
    };

    Ok(Baked {
        codediff,
        minimal,
        full,
        unix,
        index,
    })
}

/// codediff's painted ranges by operation. A deletion lives on the before side and an insertion
/// on the after side; an update or a move is painted on both sides, once each, and a pure
/// in-line deletion (`buffer` -> `buf`) is an update with nothing to paint on the after side, so
/// those two are counted as the larger of the two sides rather than the sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct PaintedCounts {
    insertions: usize,
    deletions: usize,
    updates: usize,
    moves: usize,
}

impl PaintedCounts {
    fn of(payload: &DiffPayload) -> Self {
        let count =
            |ranges: &[RangePayload], op: &str| ranges.iter().filter(|r| r.op == op).count();
        let (before, after) = (&payload.before.ranges[..], &payload.after.ranges[..]);
        Self {
            insertions: count(after, "insert"),
            deletions: count(before, "delete"),
            updates: count(before, "update").max(count(after, "update")),
            moves: count(before, "move").max(count(after, "move")),
        }
    }
}

/// The `before.<ext>.test` and `after.<ext>.test` files of a fixture directory, whatever the
/// extension - the same two `code_pair_from_dir` reads, by path rather than parsed, because
/// `compute_diff_with_options` takes paths like the real front ends do.
fn side_files(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut before = None;
    let mut after = None;
    for entry in fs::read_dir(dir).with_context(|| format!("reading {dir:?}"))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with("before.") {
            before = Some(path);
        } else if name.starts_with("after.") {
            after = Some(path);
        }
    }
    match (before, after) {
        (Some(b), Some(a)) => Ok((b, a)),
        _ => Err(anyhow!("{dir:?} lacks a before.*/after.* pair")),
    }
}

/// `diff`'s per-line verdict as ranges. Untouched lines pair up in order on the two sides, one
/// identical range per pair; each run of touched lines is one deletion (before side) or one
/// insertion (after side), anchored at the row the other side has reached, which is where the
/// viewer's cross-panel cursor lands for it. The same convention `diff::text::plain_text_diff`
/// uses for a gap it cannot pair, minus the pairing: `diff` has none.
fn unix_ranges(
    before_touched: &[bool],
    after_touched: &[bool],
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    let mut before_ranges = Vec::new();
    let mut after_ranges = Vec::new();
    let (mut b, mut a) = (0, 0);
    loop {
        let b0 = b;
        while b < before_touched.len() && before_touched[b] {
            b += 1;
        }
        let a0 = a;
        while a < after_touched.len() && after_touched[a] {
            a += 1;
        }
        if b > b0 {
            before_ranges.push(RangeMatch {
                source: TextRange::new(b0, 0, b, 0),
                destination: TextRange::new(a, 0, a, 0),
                operation: TextOperation::Delete,
            });
        }
        if a > a0 {
            after_ranges.push(RangeMatch {
                source: TextRange::new(a0, 0, a, 0),
                destination: TextRange::new(b, 0, b, 0),
                operation: TextOperation::Insert,
            });
        }
        if b >= before_touched.len() || a >= after_touched.len() {
            break;
        }
        let before_line = TextRange::new(b, 0, b + 1, 0);
        let after_line = TextRange::new(a, 0, a + 1, 0);
        before_ranges.push(RangeMatch {
            source: before_line.clone(),
            destination: after_line.clone(),
            operation: TextOperation::Identical,
        });
        after_ranges.push(RangeMatch {
            source: after_line,
            destination: before_line,
            operation: TextOperation::Identical,
        });
        b += 1;
        a += 1;
    }
    // `diff` leaves the same number of untouched lines on both sides, so both cursors run out
    // together; a leftover is a bug in the labels rather than in the file, but it is still shown
    // rather than dropped.
    if b < before_touched.len() {
        before_ranges.push(RangeMatch {
            source: TextRange::new(b, 0, before_touched.len(), 0),
            destination: TextRange::new(a, 0, a, 0),
            operation: TextOperation::Delete,
        });
    }
    if a < after_touched.len() {
        after_ranges.push(RangeMatch {
            source: TextRange::new(a, 0, after_touched.len(), 0),
            destination: TextRange::new(b, 0, b, 0),
            operation: TextOperation::Insert,
        });
    }
    (before_ranges, after_ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_case_names_a_fixture_that_exists_and_each_group_has_ten() {
        let mut diff_wrong = 0;
        let mut both_right = 0;
        for case in CASES {
            assert!(
                helper::diffs_case_dir(case.name).is_some(),
                "{} is not a fixture directory",
                case.name
            );
            match case.group {
                Group::DiffWrong => diff_wrong += 1,
                Group::BothRight => both_right += 1,
            }
        }
        assert_eq!((diff_wrong, both_right), (10, 10));
    }

    #[test]
    fn case_names_are_unique() {
        let mut names: Vec<_> = CASES.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CASES.len());
    }

    #[test]
    fn unix_ranges_pair_untouched_lines_and_run_touched_ones_together() {
        // before: keep, DEL, DEL, keep ; after: keep, INS, keep
        let (before, after) = unix_ranges(&[false, true, true, false], &[false, true, false]);
        let ops = |ranges: &[RangeMatch]| {
            ranges
                .iter()
                .map(|r| r.operation.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ops(&before),
            [
                TextOperation::Identical,
                TextOperation::Delete,
                TextOperation::Identical
            ]
        );
        assert_eq!(
            ops(&after),
            [
                TextOperation::Identical,
                TextOperation::Insert,
                TextOperation::Identical
            ]
        );
        assert_eq!(before[1].source, TextRange::new(1, 0, 3, 0));
        // The deletion is anchored where the after side stands after its own run: row 2.
        assert_eq!(before[1].destination, TextRange::new(2, 0, 2, 0));
        assert_eq!(after[1].source, TextRange::new(1, 0, 2, 0));
        assert_eq!(before[2].source, TextRange::new(3, 0, 4, 0));
        assert_eq!(before[2].destination, TextRange::new(2, 0, 3, 0));
    }

    #[test]
    fn painted_counts_take_each_two_sided_operation_once() {
        let range = |op: &'static str| RangePayload {
            op,
            source: [0; 4],
            destination: [0; 4],
        };
        let side = |ops: &[&'static str], lines: usize| codediff::web::payload::SidePayload {
            path: String::new(),
            name: String::new(),
            language: String::new(),
            lines: vec![String::new(); lines],
            ranges: ops.iter().map(|op| range(op)).collect(),
            spans: Vec::new(),
        };
        let mut payload = diff_payload(
            &DiffSessionData {
                before_path: PathBuf::new(),
                after_path: PathBuf::new(),
                before_contents: String::new(),
                after_contents: String::new(),
                before_ranges: Vec::new(),
                after_ranges: Vec::new(),
                comment_only: false,
                plain_text_fallback: false,
            },
            false,
            RenderOptions::default(),
            None,
        );
        payload.before = side(
            &["identical", "delete", "update", "update", "update", "move"],
            1,
        );
        payload.after = side(&["identical", "insert", "insert", "move"], 1);
        assert_eq!(
            PaintedCounts::of(&payload),
            PaintedCounts {
                insertions: 2,
                deletions: 1,
                updates: 3,
                moves: 1
            }
        );
    }

    #[test]
    fn unix_ranges_of_a_whole_file_rewrite_is_one_range_per_side() {
        let (before, after) = unix_ranges(&[true, true], &[true, true, true]);
        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_eq!(before[0].source, TextRange::new(0, 0, 2, 0));
        assert_eq!(after[0].source, TextRange::new(0, 0, 3, 0));
    }

    #[test]
    fn unix_ranges_of_identical_files_are_all_identical_pairs() {
        let (before, after) = unix_ranges(&[false, false], &[false, false]);
        assert!(
            before
                .iter()
                .all(|r| r.operation == TextOperation::Identical)
        );
        assert_eq!(before.len(), 2);
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn baking_every_case_matches_the_published_scores() {
        // The whole point of the list: codediff has nothing to hide on any of these, and on the
        // first ten `diff` marks lines the human mapping does not. The line-level scores that
        // decided the list live in research/data/comparison/benchmark_other.csv; this re-derives
        // the codediff half from the mapping itself, so a matcher regression that broke a case
        // fails here before it is published.
        let provenance = helper::sample_provenance().unwrap();
        let repository_urls = helper::repository_urls().unwrap();
        for case in CASES {
            let baked = bake(case, &provenance, &repository_urls).unwrap();
            assert_eq!(baked.unix.summary, None, "{}", case.name);
            assert!(baked.index.lines > 0, "{}", case.name);
            assert!(
                baked.index.diff_marked > 0,
                "{}: diff marks nothing",
                case.name
            );
            let mismatches = human_mapping::compute_mismatches(case.name).unwrap();
            assert_eq!(
                mismatches.len(),
                0,
                "{}: codediff disagrees with the human mapping",
                case.name
            );
        }
    }
}
