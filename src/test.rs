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
pub mod fixtures;
pub mod helper;

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;

    /// Clamped stubs that predate the rule below and have no explanation yet.
    ///
    /// A backlog, not an exemption. Every one of these asserts that codediff cannot currently do
    /// better without saying why - which is exactly the state the 49 stale clamps were in before
    /// they were re-measured, and half of those turned out to need no clamp at all. Shrink this
    /// list by writing the explanation (or by tightening the limit until none is needed); never
    /// grow it.
    #[cfg(feature = "test-fixtures")]
    const CLAMPS_WITHOUT_AN_EXPLANATION: &[&str] = &[
        "c-linux-small-change-struct-to-char",
        "cpp-godot-small-bugfix",
        "cpp-ladybird-refactor-variables-if-changes",
        "cpp-tensorflow-switch-to-primitive-types",
        "csharp-cyanfish-naps2-add-condition-to-if",
        "csharp-jellyfin-add-function",
        "go-lazygit-switch-to-strings",
        "html-mozilla-firefox-firefox-remove-li-around-button",
        "javascript-typescript-interesting-small-edit-refactor",
        "json-excalidraw-excalidraw-change-translations-mostly-add",
        "json-kiwix-kiwix-desktop-add-a-few-change-a-few",
        "kotlin-jetbrains-kotlin-remove-one-comment-line",
        "kotlin-nextcloud-a-few-small-removals",
        "php-zetacomponents-consoletools-file-with-parse-errors-and-a-few-deletions",
        "python-ansible-ansible-ridiculously-long-yaml-in-string-constant-and-actual-code-changes",
        "python-portagefilelist-client-remove-one-import-and-update-one-const-string",
        "rust-vercel-nextjs-refactoring-would-require-mulitmap-mapping",
        "scala-com-lihaoyi-mill-add-a-function-call",
        "scala-com-lihaoyi-mill-small-refactoring",
        "shellscript-scikit-learn-scikit-learn-string-to-regex",
        "swift-apple-swift-argument-parser-small-change",
        "swift-nextcloud-ios-move-function-and-refactor-logic",
        "swift-nextcloud-ios-refactor-and-change",
        "typescript-apache-echarts-envelop-2-lines-with-an-if-block",
        "vimscript-neovim-neovim-test-debian-package-parsing-awful-string-matching",
        "xml-gap-packages-toric-remove-two-attributes",
    ];

    /// **A clamped limit must say why it is clamped.**
    ///
    /// See `fixtures`' module doc for the boundary this enforces: `description.md` says
    /// what the fixture demands, a stub comment says why codediff falls short of it, and a limit
    /// with neither is a number nobody can review. New clamps have to carry one; the ones that
    /// already did not are listed above and must only ever shrink.
    #[test]
    #[cfg(feature = "test-fixtures")]
    fn the_clamped_stubs_explain_their_limits() -> Result<()> {
        let mut undocumented = Vec::new();
        let mut stale_allowance = Vec::new();
        for dataset in helper::DIFF_DATASETS {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("test")
                .join("fixtures")
                .join(dataset);
            if !dir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path)?;
                if !source.contains("assert_matches_human_mapping_within_limit") {
                    continue;
                }
                let Some(name) = source
                    .split_once("assert_matches_human_mapping_within_limit(")
                    .and_then(|(_, rest)| rest.split_once('"'))
                    .and_then(|(_, rest)| rest.split_once('"'))
                    .map(|(name, _)| name.to_string())
                else {
                    continue;
                };
                // Scoped to the one test that holds the clamped call, not the whole file. These
                // files carry a `painting()` test too since the two suites were merged, and a few
                // handmade ones carry extra mapping tests under their own names - an explanation
                // of a painting residual, or of a different assertion, says nothing about why
                // *this* number is what it is. Reading the file as a whole let one fixture look
                // explained when it was not.
                //
                // Found by walking back from the call to its `#[test]` rather than by function
                // name: `mapping()` is the standard name, but a handful of fixtures also carry a
                // `mapping_details()` test asserting specific nodes by hand, and the clamped call
                // is not always in the first one. Indented one level, too - the licence header
                // and any `///` docs sit at column zero and explain the file, not this number.
                let call_at = source
                    .find("assert_matches_human_mapping_within_limit")
                    .unwrap_or(0);
                let test_at = source[..call_at].rfind("#[test]").unwrap_or(0);
                let explained = source[test_at..call_at]
                    .lines()
                    .any(|line| line.starts_with("    //"));
                let allowed = CLAMPS_WITHOUT_AN_EXPLANATION.contains(&name.as_str());
                match (explained, allowed) {
                    (false, false) => undocumented.push(name),
                    (true, true) => stale_allowance.push(name),
                    _ => {}
                }
            }
        }
        assert!(
            undocumented.is_empty(),
            "these clamped limits have no comment saying why codediff cannot do better. Write one, \
             or tighten the limit until none is needed:\n    {}",
            undocumented.join("\n    ")
        );
        assert!(
            stale_allowance.is_empty(),
            "these are now explained and must come off CLAMPS_WITHOUT_AN_EXPLANATION - the list \
             only shrinks:\n    {}",
            stale_allowance.join("\n    ")
        );
        Ok(())
    }

    /// Pins what `stub_mapping_limits` can and cannot read, so the projection above is checked
    /// against a parse that is itself checked - a regex that silently matched nothing would make
    /// that test vacuously pass.
    #[test]
    #[cfg(feature = "test-fixtures")]
    fn stub_mapping_limits_reads_both_call_shapes_and_skips_the_hand_written_stub() -> Result<()> {
        let limits = helper::human_mapping::stub_mapping_limits()?;
        assert!(
            limits.len() > 400,
            "expected a limit for most of the corpus, got {}",
            limits.len()
        );
        assert_eq!(
            limits.get("c-awslabs-aws-c-common-only-insert"),
            Some(&(0, 0)),
            "the exact call shape reads as a zero limit"
        );
        assert_eq!(
            limits.get("c-sched-ext-scx-many-many-moves-some-deletes-some-adds"),
            Some(&(17, 17)),
            "the clamped call shape reads its two numbers"
        );
        assert_eq!(
            limits.get("rust-hash-optimization"),
            None,
            "rust_hash_optimization.rs asserts specific mappings by hand rather than calling \
             either helper, so it has no single limit to report and must not be guessed at"
        );
        Ok(())
    }

    /// **`quality_baseline.csv`'s accuracy columns are a projection of the `optimal_solutions`
    /// stubs, not a second opinion about them.**
    ///
    /// The two used to be maintained independently and drifted: 461 of 510 fixtures agreed
    /// exactly, 49 carried a stub limit looser than the baseline's recorded number (one allowed
    /// 66/58 against an actual of 0/0), and a single re-verification of the ground truth meant
    /// editing the same six fixtures in both places. `write_baseline` now fills those columns from
    /// `stub_mapping_limits`, and this fails if the checked-in file stops matching - which is what
    /// makes "derived" a property of the repository rather than of whoever last ran the command.
    ///
    /// Only the accuracy columns are pinned. `elapsed_ms` is a measurement of the machine that
    /// produced it and legitimately changes on every run.
    #[test]
    #[cfg(feature = "test-fixtures")]
    fn the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits() -> Result<()> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("research")
            .join("data")
            .join("quality")
            .join("quality_baseline.csv");
        // Absent in a checkout that has never run the gate - not a failure, same posture the rest
        // of this suite takes toward optional local data.
        if !path.exists() {
            return Ok(());
        }

        let limits = helper::human_mapping::stub_mapping_limits()?;
        let mut reader = csv::Reader::from_path(&path)?;
        let mut disagreeing = Vec::new();
        for record in reader.deserialize::<std::collections::HashMap<String, String>>() {
            let record = record?;
            let name = record.get("solution").cloned().unwrap_or_default();
            let Some(&(total, visible)) = limits.get(&name) else {
                // No stub means no recorded limit to project; `write_baseline` warns about these
                // rather than inventing one.
                continue;
            };
            let field = |key: &str| -> usize {
                record
                    .get(key)
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or_default()
            };
            if (field("mismatches"), field("visible_mismatches")) != (total, visible) {
                disagreeing.push(format!(
                    "{name}: baseline {},{} vs stub limit {total},{visible}",
                    field("mismatches"),
                    field("visible_mismatches")
                ));
            }
        }
        assert!(
            disagreeing.is_empty(),
            "quality_baseline.csv has drifted from the stub limits it is derived from; \
             re-run `make update-quality-baseline`:\n    {}",
            disagreeing.join("\n    ")
        );
        Ok(())
    }

    #[test]
    fn handmade_test_code_loads() -> Result<()> {
        let test_codes = helper::handmade_test_code()?;

        assert!(!test_codes.is_empty());

        Ok(())
    }

    #[test]
    #[cfg(feature = "stats")]
    fn handmade_git_repository_loads() -> Result<()> {
        let test_git_repo_path = helper::handmade_git_repository()?;

        assert!(test_git_repo_path.is_dir());

        let git_dir = test_git_repo_path.join(".git");
        assert!(git_dir.is_dir());

        let repo = git2::Repository::open(&test_git_repo_path)?;
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;

        let mut revwalk = repo.revwalk()?;
        revwalk.push(commit.id())?;
        let commit_count = revwalk.count();
        assert!(
            commit_count >= 2,
            "Expected at least 2 commits, found {}",
            commit_count
        );

        let main_rs_path = test_git_repo_path.join("main.rs");
        assert!(main_rs_path.is_file());
        let content = std::fs::read_to_string(main_rs_path)?;
        assert!(content.contains("Hello World"));

        Ok(())
    }
}
