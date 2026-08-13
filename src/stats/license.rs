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
//! Attribution/licensing metadata for a single-file sample pulled from a third-party repository
//! (via `sample_test_diffs`/`materialize_test_diffs`). A sampled before/after pair is a verbatim
//! excerpt of someone else's code, not codediff's own - it isn't covered by codediff's own
//! AGPL-3.0 license, and reusing it (even just as test-fixture input committed to this repo)
//! needs its own provenance and license terms recorded alongside it. `render_readme` records that
//! as a commit-pinned link to the license file (`blob_url`) plus a best-effort label
//! (`classify_license`), not a copy of the license text itself - see `materialize_test_diffs`'s
//! module docs and `README.md`'s "Third-party test fixtures" section.

use git2::{Repository, Tree};
use std::fmt::Write as _;

/// A license/notice file found at the root of a sampled repository's tree, at the exact commit
/// the sample was taken from - not HEAD, and not a fresh fetch, so this always matches what the
/// sampled file itself actually shipped under at the time. `text` is only ever used to feed
/// `classify_license` - `render_readme` links to the file (via `blob_url`) rather than
/// reproducing `text` itself.
pub struct LicenseFile {
    pub filename: String,
    pub label: &'static str,
    pub text: String,
}

/// Filenames this repo's root is checked against, case-insensitively, matched as a prefix so
/// `LICENSE.md`/`LICENSE-MIT`/`COPYING.LGPLv2.1`/etc. all match their base name. `NOTICE` is
/// included even though it's not itself a license: Apache-2.0 (the license most likely to ship
/// one) requires any NOTICE file's attribution content be reproduced by downstream redistributors,
/// so it needs to travel with the sample for the same reason the license text itself does.
const LICENSE_FILENAME_PREFIXES: &[&str] =
    &["license", "licence", "copying", "unlicense", "notice"];

/// Top-level directory names (case-insensitive) whose contents get the same
/// `LICENSE_FILENAME_PREFIXES` scan as the repository root itself - some projects (e.g.
/// JetBrains/kotlin) keep licensing under a `license/` subdirectory instead of root-level files.
/// One level deep only, matching `find_license_files`'s own non-recursive root scan.
const LICENSE_DIRECTORY_NAMES: &[&str] = &["license", "licenses", "licence", "licences"];

/// Every blob matching `LICENSE_FILENAME_PREFIXES`, either at `tree`'s root or one level inside
/// a `LICENSE_DIRECTORY_NAMES` directory, read as lossy UTF-8 (license text is always
/// human-readable prose; lossy conversion is fine here even though `blob_text` elsewhere in this
/// codebase requires strict UTF-8, since a mangled byte in a license file's text shouldn't block
/// recording the rest of it) and classified by `classify_license`. No deeper recursion than that:
/// walking every subdirectory of a large repo is expensive, and a project that splits licensing
/// further than one directory down is rare enough not to be worth it. A sample from a repo with
/// genuinely no license file found this way gets an empty `Vec` back, which `render_readme` turns
/// into an explicit "not found" warning rather than silently omitting licensing information.
pub fn find_license_files(repo: &Repository, tree: &Tree) -> Vec<LicenseFile> {
    let mut found = Vec::new();
    collect_license_files_in(repo, tree, "", &mut found);
    for entry in tree.iter() {
        if entry.kind() != Some(git2::ObjectType::Tree) {
            continue;
        }
        let Some(name) = entry.name() else { continue };
        if !LICENSE_DIRECTORY_NAMES.contains(&name.to_lowercase().as_str()) {
            continue;
        }
        let Ok(object) = entry.to_object(repo) else {
            continue;
        };
        let Some(subtree) = object.as_tree() else {
            continue;
        };
        collect_license_files_in(repo, subtree, name, &mut found);
    }
    // Deterministic order (tree.iter() already yields entries sorted by name, but pinning this
    // explicitly means render_readme's output doesn't depend on git2's iteration order staying
    // stable across versions).
    found.sort_by(|a, b| a.filename.cmp(&b.filename));
    found
}

/// Scans `tree`'s direct entries (no further recursion) for blobs matching
/// `LICENSE_FILENAME_PREFIXES`, appending matches to `found`. `dir_prefix` (e.g. `"license"`, or
/// `""` for the repository root itself) is prepended to each recorded filename so `render_readme`
/// can show a reader exactly where in the repository each license text came from.
fn collect_license_files_in(
    repo: &Repository,
    tree: &Tree,
    dir_prefix: &str,
    found: &mut Vec<LicenseFile>,
) {
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        let lower = name.to_lowercase();
        if !LICENSE_FILENAME_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            continue;
        }
        let Ok(object) = entry.to_object(repo) else {
            continue;
        };
        let Some(blob) = object.as_blob() else {
            continue;
        };
        let text = String::from_utf8_lossy(blob.content()).into_owned();
        let label = classify_license(&text);
        let filename = if dir_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{dir_prefix}/{name}")
        };
        found.push(LicenseFile {
            filename,
            label,
            text,
        });
    }
}

/// Best-effort SPDX-style label for `text`, by matching each license's own distinctive
/// boilerplate phrasing - not a general-purpose license classifier, just enough precision to
/// give a human a useful hint without having to follow `render_readme`'s link and read the
/// license file itself. Ordered most-specific first (e.g.
/// LGPL/AGPL checked before the plain GPL phrase they'd otherwise also match).
fn classify_license(text: &str) -> &'static str {
    let has = |needle: &str| text.contains(needle);

    if has("Apache License") && has("Version 2.0") {
        "Apache License 2.0"
    } else if has("GNU AFFERO GENERAL PUBLIC LICENSE") && has("Version 3") {
        "GNU Affero General Public License v3.0"
    } else if has("GNU LESSER GENERAL PUBLIC LICENSE") && has("Version 3") {
        "GNU Lesser General Public License v3.0"
    } else if has("GNU LESSER GENERAL PUBLIC LICENSE") && has("Version 2.1") {
        "GNU Lesser General Public License v2.1"
    } else if has("GNU LIBRARY GENERAL PUBLIC LICENSE") {
        // The LGPL's original name before FSF renamed it "Lesser" at v2.1 - functionally the
        // predecessor to LGPL-2.1, commonly labeled LGPL-2.0 by SPDX and license scanners.
        "GNU Library General Public License v2.0 (LGPL predecessor)"
    } else if has("GNU GENERAL PUBLIC LICENSE") && has("Version 3") {
        "GNU General Public License v3.0"
    } else if has("GNU GENERAL PUBLIC LICENSE") && has("Version 2") {
        "GNU General Public License v2.0"
    } else if has("Mozilla Public License Version 2.0") {
        "Mozilla Public License 2.0"
    } else if has("Redistributions of source code must retain")
        && has("may be used to endorse or promote")
    {
        "BSD 3-Clause License"
    } else if has("Redistributions of source code must retain") {
        "BSD 2-Clause License"
    } else if has("Permission is hereby granted, free of charge") {
        "MIT License"
    } else if has("Permission to use, copy, modify, and/or distribute this software")
        && has("THE SOFTWARE IS PROVIDED \"AS IS\"")
    {
        "ISC License"
    } else if has("This is free and unencumbered software released into the public domain") {
        "The Unlicense"
    } else if has("Boost Software License") {
        "Boost Software License 1.0"
    } else if has("CC0 1.0 Universal") || has("Creative Commons Zero") {
        "CC0 1.0 Universal"
    } else if has("1. The origin of this software must not be misrepresented") {
        "zlib License"
    } else {
        "Unrecognized license text (see linked file)"
    }
}

/// The `origin` remote's URL, if configured - the same URL `research/fetch_data/dataset.sh`
/// originally cloned from, so this always points a reader back at the actual upstream project
/// rather than at this local checkout path.
pub fn origin_remote_url(repo: &Repository) -> Option<String> {
    repo.find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(str::to_string))
}

/// A direct link to `path` inside `repo_url`'s hosted repository at `commit` - pinned to that
/// exact commit (not a branch) so the link keeps pointing at the license text as it actually read
/// when the sample was taken, even if the file is later moved, edited, or removed upstream. Only
/// handles the three hosts `research/fetch_data/dataset.sh` actually clones from (each uses a
/// different blob-URL scheme); any other host - or a URL that doesn't parse - returns `None`
/// rather than guessing, since a wrong link is worse than no link (the repository/commit already
/// recorded above `render_readme`'s license section is still enough to find the file by hand).
fn blob_url(repo_url: &str, commit: &str, path: &str) -> Option<String> {
    let without_scheme = repo_url.trim_end_matches('/').split_once("://")?.1;
    let (host, rest) = without_scheme.split_once('/')?;
    let rest = rest.trim_end_matches(".git");
    match host {
        "github.com" => Some(format!("https://github.com/{rest}/blob/{commit}/{path}")),
        "gitlab.com" => Some(format!("https://gitlab.com/{rest}/-/blob/{commit}/{path}")),
        "codeberg.org" => Some(format!(
            "https://codeberg.org/{rest}/src/commit/{commit}/{path}"
        )),
        _ => None,
    }
}

/// Renders the `README.md` written into every sample directory (and, on promotion, copied
/// alongside its `diffs/` fixture - see `human_solver::action_promote`): where the before/after
/// content came from, and a link to the license it's actually under (via `blob_url`, pinned to
/// the sampled commit) rather than a copy of the license text itself - `classify_license` already
/// gives a same-file label, and linking avoids duplicating (and letting drift) potentially long
/// license text across hundreds of fixture directories.
///
/// `unverifiable_reason`, when set, means the local repository checkout couldn't actually be
/// inspected at this commit (see `materialize_test_diffs::backfill_promoted_readme` - typically a
/// commit that's fallen outside a shallow clone's `--depth` window since the sample was first
/// promoted) - distinct from `license_files` being empty, which means the checkout *was*
/// inspected and genuinely has no license file. `license_files` is ignored when this is set.
pub fn render_readme(
    repo_url: Option<&str>,
    repository: &str,
    commit: &str,
    path: &str,
    dataset: &str,
    license_files: &[LicenseFile],
    unverifiable_reason: Option<&str>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Sample provenance");
    let _ = writeln!(out);
    match repo_url {
        Some(url) => {
            let _ = writeln!(out, "- **Repository:** {url} (`{repository}`)");
        }
        None => {
            let _ = writeln!(
                out,
                "- **Repository:** `{repository}` (origin remote URL unavailable)"
            );
        }
    }
    let _ = writeln!(out, "- **Commit:** `{commit}`");
    let _ = writeln!(out, "- **File:** `{path}`");
    let _ = writeln!(out, "- **Research dataset:** {dataset}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "`before.*.test`/`after.*.test` in this directory are an unmodified excerpt of the file \
         above, copied verbatim from the source repository at the commit above (and its single \
         parent) for use as codediff test-fixture input. This content is **not** part of \
         codediff's own codebase and is **not** covered by codediff's own AGPL-3.0 license - it \
         remains under whatever license the source repository itself applies, linked below \
         exactly as it read in that repository at this commit."
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## License");
    let _ = writeln!(out);

    if let Some(reason) = unverifiable_reason {
        let _ = writeln!(
            out,
            "**Not verified.** The local checkout couldn't be inspected at this commit ({reason}). \
             This is very likely a shallow-clone gap (the commit has aged out of the checkout's \
             `--depth` window since this sample was originally promoted), not evidence the \
             repository lacks a license. Check the repository above directly before reusing this \
             sample outside codediff's own test suite."
        );
        return out;
    }

    if license_files.is_empty() {
        let _ = writeln!(
            out,
            "No LICENSE/COPYING/NOTICE file was found at the repository root at this commit. \
             Licensing terms are unknown from this checkout alone - check the repository above \
             directly before reusing this sample outside codediff's own test suite."
        );
        return out;
    }

    for file in license_files {
        match repo_url.and_then(|url| blob_url(url, commit, &file.filename)) {
            Some(link) => {
                let _ = writeln!(out, "- `{}` - {} ({link})", file.filename, file.label);
            }
            None => {
                let _ = writeln!(out, "- `{}` - {}", file.filename, file.label);
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "License text is not reproduced here - follow the link(s) above (or the repository listed \
         at the top of this file, at the commit above) for the full terms."
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_license_texts() {
        assert_eq!(
            classify_license("Apache License\nVersion 2.0, January 2004"),
            "Apache License 2.0"
        );
        assert_eq!(
            classify_license("Permission is hereby granted, free of charge, to any person..."),
            "MIT License"
        );
        assert_eq!(
            classify_license("GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007"),
            "GNU General Public License v3.0"
        );
        assert_eq!(
            classify_license("GNU LESSER GENERAL PUBLIC LICENSE\nVersion 2.1, February 1999"),
            "GNU Lesser General Public License v2.1"
        );
        assert_eq!(
            classify_license("something nobody has ever written before"),
            "Unrecognized license text (see linked file)"
        );
    }

    #[test]
    fn blob_url_builds_a_commit_pinned_link_per_host() {
        assert_eq!(
            blob_url("https://github.com/example/repo.git", "abc123", "LICENSE"),
            Some("https://github.com/example/repo/blob/abc123/LICENSE".to_string())
        );
        assert_eq!(
            blob_url("https://gitlab.com/example/repo.git", "abc123", "LICENSE"),
            Some("https://gitlab.com/example/repo/-/blob/abc123/LICENSE".to_string())
        );
        assert_eq!(
            blob_url("https://codeberg.org/example/repo.git", "abc123", "LICENSE"),
            Some("https://codeberg.org/example/repo/src/commit/abc123/LICENSE".to_string())
        );
        assert_eq!(
            blob_url("https://example.com/example/repo.git", "abc123", "LICENSE"),
            None
        );
    }

    #[test]
    fn render_readme_without_license_files_warns_explicitly() {
        let readme = render_readme(
            Some("https://github.com/example/repo"),
            "example-repo.git",
            "abc123",
            "src/main.rs",
            "full",
            &[],
            None,
        );
        assert!(readme.contains("No LICENSE/COPYING/NOTICE file was found"));
        assert!(readme.contains("https://github.com/example/repo"));
        assert!(readme.contains("`src/main.rs`"));
    }

    #[test]
    fn render_readme_links_to_the_license_file_instead_of_embedding_its_text() {
        let files = vec![LicenseFile {
            filename: "LICENSE".to_string(),
            label: "MIT License",
            text: "Permission is hereby granted, free of charge...".to_string(),
        }];
        let readme = render_readme(
            Some("https://github.com/example/repo"),
            "example-repo.git",
            "abc123",
            "src/main.rs",
            "small",
            &files,
            None,
        );
        assert!(readme.contains(
            "- `LICENSE` - MIT License (https://github.com/example/repo/blob/abc123/LICENSE)"
        ));
        assert!(!readme.contains("Permission is hereby granted, free of charge..."));
    }

    #[test]
    fn render_readme_lists_the_filename_without_a_link_for_an_unrecognized_host() {
        let files = vec![LicenseFile {
            filename: "LICENSE".to_string(),
            label: "MIT License",
            text: "Permission is hereby granted, free of charge...".to_string(),
        }];
        let readme = render_readme(
            Some("https://example.com/example/repo"),
            "example-repo.git",
            "abc123",
            "src/main.rs",
            "small",
            &files,
            None,
        );
        assert!(readme.contains("- `LICENSE` - MIT License\n"));
    }

    #[test]
    fn render_readme_with_an_unverifiable_reason_explains_why_instead_of_claiming_no_license() {
        let readme = render_readme(
            Some("https://github.com/example/repo"),
            "example-repo.git",
            "abc123",
            "src/main.rs",
            "small",
            &[],
            Some("object not found"),
        );
        assert!(readme.contains("**Not verified.**"));
        assert!(readme.contains("object not found"));
        assert!(!readme.contains("No LICENSE/COPYING/NOTICE file was found"));
    }
}
