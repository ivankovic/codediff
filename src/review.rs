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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
//! Git commit review: what a repository has pending (unstaged), staged, and recently committed,
//! and how to turn any one of those changes into the two files on disk the diff engine reads.
//! Shared by both front ends (`tui::components::review_dialog`, `web::session`).
//!
//! Talks to the `git` binary, not to `git2`. `git2` is a `stats`-only dependency here on purpose
//! (its OpenSSL/libssh2 build chain is not something `cargo install codediff` should pay for),
//! and every user of this feature has `git` on `PATH` already - it is what invokes codediff as a
//! difftool in the first place. Three plumbing commands cover everything: `diff --name-status`
//! for the three file lists (one parser for all three), `log` for the commits, `show <rev>:<path>`
//! for the blobs.
//!
//! **A side is materialized under its repository-relative path**, in a temp workspace laid out
//! as `<workspace>/<revision>/<path>`. Both halves of that matter: the basename picks the
//! tree-sitter grammar (`blob-1234` gets a plain line diff, `theme.rs` gets Rust), and the
//! directory keeps two same-named files at one revision (`mod.rs`, `index.ts`) from overwriting
//! each other. The working tree's own file is never copied; the real path is diffed, so `e`
//! edits the real file. An absent side (an added or deleted file) is an empty file at the same
//! path, which parses as a whole-file insert or delete in the right language, rather than
//! `/dev/null`, which does not exist everywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// git's well-known empty tree, the base a root commit is diffed against.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// How many commits [`load`] lists by default - a screenful, and enough to find the one that
/// broke something yesterday.
pub const DEFAULT_COMMIT_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Untracked,
    Unknown,
}

impl FileStatus {
    fn from_letter(letter: char) -> Self {
        match letter {
            'A' => FileStatus::Added,
            'M' => FileStatus::Modified,
            'D' => FileStatus::Deleted,
            'R' => FileStatus::Renamed,
            'C' => FileStatus::Copied,
            'T' => FileStatus::TypeChanged,
            'U' => FileStatus::Unmerged,
            '?' => FileStatus::Untracked,
            _ => FileStatus::Unknown,
        }
    }

    /// The one-letter marker `git status --short` readers know.
    pub fn letter(self) -> char {
        match self {
            FileStatus::Added => 'A',
            FileStatus::Modified => 'M',
            FileStatus::Deleted => 'D',
            FileStatus::Renamed => 'R',
            FileStatus::Copied => 'C',
            FileStatus::TypeChanged => 'T',
            FileStatus::Unmerged => 'U',
            FileStatus::Untracked => '?',
            FileStatus::Unknown => 'X',
        }
    }

    /// Whether a rename or copy carries the previous path.
    fn takes_two_paths(self) -> bool {
        matches!(self, FileStatus::Renamed | FileStatus::Copied)
    }
}

/// One changed file. Paths are repository-relative with forward slashes, exactly as git prints
/// them - they are handed straight back to `git show <rev>:<path>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub status: FileStatus,
    pub path: String,
    /// The previous path of a rename or copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
}

impl ChangedFile {
    /// `M src/main.rs`, or `R old -> new` for a rename.
    pub fn label(&self) -> String {
        match &self.old_path {
            Some(old) => format!("{} {} -> {}", self.status.letter(), old, self.path),
            None => format!("{} {}", self.status.letter(), self.path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub subject: String,
    pub files: Vec<ChangedFile>,
}

impl Commit {
    /// `abc1234 2026-09-10 Subject (Author)`.
    pub fn label(&self) -> String {
        format!(
            "{} {} {} ({})",
            self.short, self.date, self.subject, self.author
        )
    }
}

/// Which list a file was picked from - what decides which two revisions to diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeSet {
    WorkingTree,
    Staged,
    Commit { hash: String },
}

impl ChangeSet {
    /// Short enough for a footer: `working tree`, `staged`, or the abbreviated hash.
    pub fn label(&self) -> String {
        match self {
            ChangeSet::WorkingTree => "working tree".to_string(),
            ChangeSet::Staged => "staged".to_string(),
            ChangeSet::Commit { hash } => hash.chars().take(7).collect(),
        }
    }
}

/// One file to open: which set, which file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewTarget {
    pub set: ChangeSet,
    pub file: ChangedFile,
}

/// Everything the picker shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Review {
    pub root: PathBuf,
    /// Unstaged modifications and deletions, then untracked files.
    pub working_tree: Vec<ChangedFile>,
    pub staged: Vec<ChangedFile>,
    pub commits: Vec<Commit>,
}

impl Review {
    /// The files of `set`, in the picker's order - what `]`/`[` step through.
    pub fn files_of(&self, set: &ChangeSet) -> &[ChangedFile] {
        match set {
            ChangeSet::WorkingTree => &self.working_tree,
            ChangeSet::Staged => &self.staged,
            ChangeSet::Commit { hash } => self
                .commits
                .iter()
                .find(|commit| &commit.hash == hash)
                .map(|commit| commit.files.as_slice())
                .unwrap_or(&[]),
        }
    }
}

/// Runs `git -C <root> <args>` and returns its stdout, or git's own stderr as the error.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim().lines().next().unwrap_or("no message")
        );
    }
    Ok(output.stdout)
}

/// The top of the repository containing `from`, or an error naming the directory when it is not
/// inside one.
pub fn repository_root(from: &Path) -> Result<PathBuf> {
    let out = git(from, &["rev-parse", "--show-toplevel"])
        .with_context(|| format!("{} is not inside a git repository", from.display()))?;
    Ok(PathBuf::from(String::from_utf8_lossy(&out).trim_end()))
}

/// Parses `git diff --name-status -z` output: a status token, a path, and a second path for a
/// rename or copy, all NUL-terminated.
pub fn parse_name_status(bytes: &[u8]) -> Vec<ChangedFile> {
    let mut fields = bytes
        .split(|&b| b == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(status_field) = fields.next() {
        let status = FileStatus::from_letter(status_field.chars().next().unwrap_or('X'));
        let Some(first) = fields.next() else {
            break;
        };
        if status.takes_two_paths() {
            let Some(second) = fields.next() else {
                break;
            };
            files.push(ChangedFile {
                status,
                path: second,
                old_path: Some(first),
            });
        } else {
            files.push(ChangedFile {
                status,
                path: first,
                old_path: None,
            });
        }
    }
    files
}

/// The `git log` format [`load`] asks for: five fields separated by the ASCII unit separator,
/// records NUL-terminated by `-z`.
const LOG_FORMAT: &str = "--format=%H%x1f%h%x1f%an%x1f%ad%x1f%s";

/// Parses [`LOG_FORMAT`] output into commits with no files yet.
pub fn parse_log(bytes: &[u8]) -> Vec<Commit> {
    bytes
        .split(|&b| b == 0)
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let text = String::from_utf8_lossy(record);
            let mut fields = text.trim_end_matches('\n').split('\x1f');
            Some(Commit {
                hash: fields.next()?.to_string(),
                short: fields.next()?.to_string(),
                author: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
                subject: fields.next().unwrap_or("").to_string(),
                files: Vec::new(),
            })
        })
        .collect()
}

/// The first parent of `hash`, or the empty tree for a root commit - what the commit's files are
/// diffed against. A merge is shown against its first parent, as `git log -p` does.
fn diff_base(root: &Path, hash: &str) -> Result<String> {
    let out = git(root, &["rev-list", "--parents", "-n", "1", hash])?;
    let text = String::from_utf8_lossy(&out);
    Ok(text
        .split_whitespace()
        .nth(1)
        .unwrap_or(EMPTY_TREE)
        .to_string())
}

fn has_commits(root: &Path) -> bool {
    git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok()
}

/// Lists the repository containing `from`: unstaged changes and untracked files, staged changes,
/// and the last `commit_limit` commits with their files.
pub fn load(from: &Path, commit_limit: usize) -> Result<Review> {
    let root = repository_root(from)?;
    let mut working_tree = parse_name_status(&git(&root, &["diff", "--name-status", "-z"])?);
    let untracked = git(&root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    working_tree.extend(
        untracked
            .split(|&b| b == 0)
            .filter(|path| !path.is_empty())
            .map(|path| ChangedFile {
                status: FileStatus::Untracked,
                path: String::from_utf8_lossy(path).into_owned(),
                old_path: None,
            }),
    );
    let staged = parse_name_status(&git(
        &root,
        &["diff", "--cached", "--name-status", "-M", "-z"],
    )?);
    let mut commits = Vec::new();
    if has_commits(&root) {
        let limit = commit_limit.to_string();
        commits = parse_log(&git(
            &root,
            &["log", "-z", "-n", &limit, "--date=short", LOG_FORMAT],
        )?);
        for commit in &mut commits {
            let base = diff_base(&root, &commit.hash)?;
            commit.files = parse_name_status(&git(
                &root,
                &["diff", "--name-status", "-M", "-z", &base, &commit.hash],
            )?);
        }
    }
    Ok(Review {
        root,
        working_tree,
        staged,
        commits,
    })
}

/// Where one side of a change comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Side {
    /// `git show <rev>:<path>`; `rev` is `""` for the index.
    Blob { rev: String, path: String },
    /// The real file in the working tree.
    WorkTree(String),
    /// Nothing there - an added file's before, a deleted file's after.
    Absent(String),
}

/// The two sides of `target`, before and after.
fn sides(root: &Path, target: &ReviewTarget) -> Result<(Side, Side)> {
    let file = &target.file;
    let path = file.path.clone();
    let old_path = file.old_path.clone().unwrap_or_else(|| path.clone());
    let blob = |rev: &str, path: String| Side::Blob {
        rev: rev.to_string(),
        path,
    };
    Ok(match &target.set {
        ChangeSet::WorkingTree => {
            let before = match file.status {
                FileStatus::Untracked | FileStatus::Added => Side::Absent(path.clone()),
                // The index holds several stages of an unmerged file; HEAD is the one unambiguous
                // "before".
                FileStatus::Unmerged => blob("HEAD", old_path),
                _ => blob("", old_path),
            };
            let after = match file.status {
                FileStatus::Deleted => Side::Absent(path),
                _ => Side::WorkTree(path),
            };
            (before, after)
        }
        ChangeSet::Staged => {
            let before = match file.status {
                FileStatus::Added => Side::Absent(path.clone()),
                _ => blob("HEAD", old_path),
            };
            let after = match file.status {
                FileStatus::Deleted => Side::Absent(path),
                _ => blob("", path),
            };
            (before, after)
        }
        ChangeSet::Commit { hash } => {
            let base = diff_base(root, hash)?;
            let before = match file.status {
                FileStatus::Added => Side::Absent(path.clone()),
                _ => blob(&base, old_path),
            };
            let after = match file.status {
                FileStatus::Deleted => Side::Absent(path),
                _ => blob(hash, path),
            };
            (before, after)
        }
    })
}

/// A temp directory holding materialized blobs, removed when dropped.
#[derive(Debug)]
pub struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    /// Under the system temp directory, which `tui::theme` treats as throwaway - so a reviewed
    /// pair never lands in the recent-pairs list, where it would dangle after this is dropped.
    pub fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "codediff-review-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes `bytes` to `<workspace>/<label>/<path>`, creating the directories, and returns the
    /// file's path.
    fn write(&self, label: &str, path: &str, bytes: &[u8]) -> Result<PathBuf> {
        let target = self.dir.join(label).join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(&target, bytes)
            .with_context(|| format!("cannot write {}", target.display()))?;
        Ok(target)
    }

    fn materialize_side(&self, root: &Path, side: &Side) -> Result<PathBuf> {
        match side {
            Side::WorkTree(path) => Ok(root.join(path)),
            Side::Absent(path) => self.write("absent", path, b""),
            Side::Blob { rev, path } => {
                let spec = format!("{rev}:{path}");
                let bytes = git(root, &["show", &spec])?;
                let label = if rev.is_empty() {
                    "index"
                } else {
                    rev.as_str()
                };
                let label: String = label
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                self.write(&label, path, &bytes)
            }
        }
    }

    /// The `(before, after)` files for `target`, ready for `compute_diff`.
    pub fn materialize(&self, root: &Path, target: &ReviewTarget) -> Result<(PathBuf, PathBuf)> {
        let (before, after) = sides(root, target)?;
        Ok((
            self.materialize_side(root, &before)?,
            self.materialize_side(root, &after)?,
        ))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    /// A repository with two commits (the second a rename), one staged addition, one unstaged
    /// modification, one unstaged deletion and one untracked file.
    fn sample_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run(root, &["init", "-q", "-b", "main"]);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("gone.rs"), "fn gone() {}\n").unwrap();
        std::fs::write(root.join("src/old.rs"), "fn old() {}\n").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "first"]);
        run(root, &["mv", "src/old.rs", "src/new.rs"]);
        run(root, &["commit", "-q", "-m", "rename old to new"]);
        std::fs::write(root.join("src/a.rs"), "fn a() { 1 }\n").unwrap();
        std::fs::remove_file(root.join("gone.rs")).unwrap();
        std::fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();
        run(root, &["add", "src/b.rs"]);
        std::fs::write(root.join("notes.txt"), "untracked\n").unwrap();
        dir
    }

    #[test]
    fn name_status_parses_plain_and_two_path_records() {
        let files = parse_name_status(b"M\0src/a.rs\0R100\0old.rs\0new.rs\0A\0b.rs\0D\0c.rs\0");
        assert_eq!(files.len(), 4);
        assert_eq!(
            files[0],
            ChangedFile {
                status: FileStatus::Modified,
                path: "src/a.rs".into(),
                old_path: None
            }
        );
        assert_eq!(
            files[1],
            ChangedFile {
                status: FileStatus::Renamed,
                path: "new.rs".into(),
                old_path: Some("old.rs".into())
            }
        );
        assert_eq!(files[1].label(), "R old.rs -> new.rs");
        assert_eq!(files[3].label(), "D c.rs");
        assert!(parse_name_status(b"").is_empty());
        assert!(
            parse_name_status(b"M\0").is_empty(),
            "a truncated record is dropped, not panicked on"
        );
    }

    #[test]
    fn log_records_split_on_the_unit_separator() {
        let commits = parse_log(b"abc\x1fabc1234\x1fAda\x1f2026-09-10\x1fSubject here\0def\x1fdef5678\x1fBob\x1f2026-09-09\x1f\0");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].label(), "abc1234 2026-09-10 Subject here (Ada)");
        assert_eq!(commits[1].subject, "");
    }

    #[test]
    fn change_set_labels_are_footer_sized() {
        assert_eq!(ChangeSet::WorkingTree.label(), "working tree");
        assert_eq!(ChangeSet::Staged.label(), "staged");
        assert_eq!(
            ChangeSet::Commit {
                hash: "0123456789abcdef".into()
            }
            .label(),
            "0123456"
        );
        let json = serde_json::to_string(&ChangeSet::Commit { hash: "x".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"commit","hash":"x"}"#);
    }

    #[test]
    fn load_lists_the_three_sets_of_a_real_repository() {
        let dir = sample_repo();
        let review = load(&dir.path().join("src"), 10).unwrap();
        assert_eq!(review.root, dir.path().canonicalize().unwrap());
        let names =
            |files: &[ChangedFile]| files.iter().map(ChangedFile::label).collect::<Vec<_>>();
        assert_eq!(
            names(&review.working_tree),
            vec!["D gone.rs", "M src/a.rs", "? notes.txt"]
        );
        assert_eq!(names(&review.staged), vec!["A src/b.rs"]);
        assert_eq!(review.commits.len(), 2);
        assert_eq!(review.commits[0].subject, "rename old to new");
        assert_eq!(
            names(&review.commits[0].files),
            vec!["R src/old.rs -> src/new.rs"]
        );
        assert_eq!(
            names(&review.commits[1].files),
            vec!["A gone.rs", "A src/a.rs", "A src/old.rs"],
            "the root commit is diffed against the empty tree"
        );
        assert_eq!(review.files_of(&ChangeSet::Staged).len(), 1);
        assert_eq!(
            review
                .files_of(&ChangeSet::Commit {
                    hash: review.commits[0].hash.clone()
                })
                .len(),
            1
        );
        assert!(
            review
                .files_of(&ChangeSet::Commit {
                    hash: "nope".into()
                })
                .is_empty()
        );
    }

    #[test]
    fn an_empty_repository_lists_nothing_and_does_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        std::fs::write(dir.path().join("x.rs"), "").unwrap();
        run(dir.path(), &["add", "x.rs"]);
        let review = load(dir.path(), 5).unwrap();
        assert!(review.commits.is_empty());
        assert_eq!(review.staged[0].status, FileStatus::Added);
    }

    #[test]
    fn outside_a_repository_is_an_error_naming_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path(), 5).unwrap_err().to_string();
        assert!(err.contains("not inside a git repository"), "{err}");
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn materializing_each_kind_of_change_keeps_the_real_path_and_the_right_contents() {
        let dir = sample_repo();
        let review = load(dir.path(), 10).unwrap();
        let workspace = Workspace::new().unwrap();
        let root = &review.root;

        // Unstaged modification: index blob vs the real working-tree file.
        let modified = ReviewTarget {
            set: ChangeSet::WorkingTree,
            file: review.working_tree[1].clone(),
        };
        let (before, after) = workspace.materialize(root, &modified).unwrap();
        assert_eq!(read(&before), "fn a() {}\n");
        assert!(before.starts_with(workspace.dir()));
        assert!(before.ends_with("index/src/a.rs"), "{}", before.display());
        assert_eq!(
            after,
            root.join("src/a.rs"),
            "the working tree side is the real file"
        );

        // Unstaged deletion: index blob vs an empty file at the same path.
        let deleted = ReviewTarget {
            set: ChangeSet::WorkingTree,
            file: review.working_tree[0].clone(),
        };
        let (before, after) = workspace.materialize(root, &deleted).unwrap();
        assert_eq!(read(&before), "fn gone() {}\n");
        assert_eq!(read(&after), "");
        assert!(after.ends_with("absent/gone.rs"));

        // Untracked: empty vs the real file.
        let untracked = ReviewTarget {
            set: ChangeSet::WorkingTree,
            file: review.working_tree[2].clone(),
        };
        let (before, after) = workspace.materialize(root, &untracked).unwrap();
        assert_eq!(read(&before), "");
        assert_eq!(after, root.join("notes.txt"));

        // Staged addition: empty vs the index blob.
        let staged = ReviewTarget {
            set: ChangeSet::Staged,
            file: review.staged[0].clone(),
        };
        let (before, after) = workspace.materialize(root, &staged).unwrap();
        assert_eq!(read(&before), "");
        assert_eq!(read(&after), "fn b() {}\n");
        assert!(after.ends_with("index/src/b.rs"));

        // A committed rename: the old path at the parent vs the new path at the commit.
        let commit = &review.commits[0];
        let renamed = ReviewTarget {
            set: ChangeSet::Commit {
                hash: commit.hash.clone(),
            },
            file: commit.files[0].clone(),
        };
        let (before, after) = workspace.materialize(root, &renamed).unwrap();
        assert!(before.ends_with("src/old.rs") && after.ends_with("src/new.rs"));
        assert_eq!(read(&before), read(&after));
        assert!(after.to_string_lossy().contains(&commit.hash));

        // The root commit's addition: empty vs the blob.
        let first = &review.commits[1];
        let added = ReviewTarget {
            set: ChangeSet::Commit {
                hash: first.hash.clone(),
            },
            file: first.files[1].clone(),
        };
        let (before, after) = workspace.materialize(root, &added).unwrap();
        assert_eq!(read(&before), "");
        assert_eq!(read(&after), "fn a() {}\n");

        let kept = workspace.dir().to_path_buf();
        drop(workspace);
        assert!(!kept.exists(), "the workspace is removed on drop");
    }

    #[test]
    fn a_missing_blob_is_a_clear_error_not_a_panic() {
        let dir = sample_repo();
        let review = load(dir.path(), 10).unwrap();
        let workspace = Workspace::new().unwrap();
        let bogus = ReviewTarget {
            set: ChangeSet::Staged,
            file: ChangedFile {
                status: FileStatus::Modified,
                path: "no/such.rs".into(),
                old_path: None,
            },
        };
        let err = workspace
            .materialize(&review.root, &bogus)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("git show"), "{err}");
    }
}
