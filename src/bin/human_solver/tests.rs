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
//! Every test for the `human_solver` binary.
//!
//! One file rather than a `mod tests` per module: the suite shares a substantial set of fixtures
//! (`test_app`, `press_on_case`, `press_in_diff_picker`, the picker view builders), and those are
//! used by tests that exercise different modules. Splitting the tests to sit beside the code they
//! cover is the better end state and remains worth doing - it needs the shared fixtures lifted
//! into their own `#[cfg(test)]` module first, and the tests routed by hand, since a test's name
//! and its body disagree often enough that automatic routing gets it wrong.

use super::*;
use tempfile::NamedTempFile;

#[test]
fn validate_new_case_name_rejects_empty() {
    assert!(validate_new_case_name("").is_err());
}

#[test]
fn validate_new_case_name_rejects_leading_digit() {
    assert!(validate_new_case_name("1rust-add-if").is_err());
}

#[test]
fn validate_new_case_name_rejects_unsafe_characters() {
    assert!(validate_new_case_name("rust/add-if").is_err());
    assert!(validate_new_case_name("rust add if").is_err());
    assert!(validate_new_case_name("rust.add.if").is_err());
}

#[test]
fn validate_new_case_name_accepts_letters_digits_hyphen_underscore() {
    assert!(validate_new_case_name("rust-add-if_2").is_ok());
}

#[test]
fn validate_new_case_name_rejects_rust_keywords() {
    assert!(validate_new_case_name("match").is_err());
    assert!(validate_new_case_name("type").is_err());
    assert!(validate_new_case_name("self").is_err());
    // A keyword as a substring of a longer name is fine -- only an exact module-name
    // collision matters.
    assert!(validate_new_case_name("matches-guard").is_ok());
}

#[test]
fn stub_test_contents_has_no_comment_block_when_none_given() {
    let contents = stub_test_contents("rust-add-if", None);
    assert!(!contents.contains("//\n") && !contents.contains("    // "));
    assert!(contents.contains(
        "fn mapping() -> Result<()> {\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
    ));
}

#[test]
fn stub_test_contents_has_no_comment_block_when_comment_is_empty_or_whitespace() {
    assert_eq!(
        stub_test_contents("rust-add-if", Some("")),
        stub_test_contents("rust-add-if", None)
    );
    assert_eq!(
        stub_test_contents("rust-add-if", Some("   \n  ")),
        stub_test_contents("rust-add-if", None)
    );
}

#[test]
fn stub_test_contents_includes_a_wrapped_comment_block_right_before_the_assert() {
    let contents = stub_test_contents("rust-add-if", Some("A short note."));
    assert!(contents.contains(
        "fn mapping() -> Result<()> {\n    // A short note.\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
    ));
}

#[test]
fn wrap_comment_lines_keeps_a_short_comment_on_one_line() {
    assert_eq!(
        wrap_comment_lines("A short note."),
        "    // A short note.\n"
    );
}

#[test]
fn wrap_comment_lines_wraps_long_comments_at_word_boundaries() {
    let long = "one two three four five six seven eight nine ten eleven twelve thirteen \
                 fourteen fifteen sixteen seventeen eighteen nineteen twenty";
    let wrapped = wrap_comment_lines(long);
    assert!(
        wrapped.lines().count() > 1,
        "expected a comment this long to wrap onto multiple lines"
    );
    for line in wrapped.lines() {
        assert!(
            line.len() <= 96,
            "line exceeds the 96-column wrap width: {:?} ({} chars)",
            line,
            line.len()
        );
        assert!(
            line.starts_with("    // "),
            "line missing the expected prefix: {:?}",
            line
        );
    }
    // Every word survives the wrap, in order, none dropped or duplicated - strip each line's
    // "    // " prefix first so it doesn't get counted as a word of its own.
    let rejoined: Vec<&str> = wrapped
        .lines()
        .flat_map(|line| {
            line.strip_prefix("    // ")
                .unwrap_or(line)
                .split_whitespace()
        })
        .collect();
    let original: Vec<&str> = long.split_whitespace().collect();
    assert_eq!(rejoined, original);
}

#[test]
fn wrap_comment_lines_never_splits_a_single_word_even_if_it_exceeds_the_width() {
    let word = "x".repeat(200);
    let wrapped = wrap_comment_lines(&word);
    assert_eq!(wrapped, format!("    // {word}\n"));
}

#[test]
fn is_state_preserving_key_is_true_only_for_typing_in_the_three_text_input_modals() {
    assert!(is_state_preserving_key(
        Some(&Modal::PromptSearch {
            input: String::new()
        }),
        KeyCode::Char('a')
    ));
    assert!(is_state_preserving_key(
        Some(&Modal::PromptSearch {
            input: "x".to_string()
        }),
        KeyCode::Backspace
    ));
    assert!(is_state_preserving_key(
        Some(&Modal::PromptPromoteName {
            input: String::new(),
            error: None
        }),
        KeyCode::Char('a')
    ));
    assert!(is_state_preserving_key(
        Some(&Modal::PromptRejectReason {
            input: String::new(),
            error: None
        }),
        KeyCode::Char('a')
    ));
}

#[test]
fn is_state_preserving_key_is_false_for_enter_esc_on_the_same_three_modals() {
    // Enter/Esc can search-and-move-the-cursor, promote/save, reject, or close the modal --
    // all things that can change what the cached `FrameState` would report, unlike plain
    // typing.
    let search = Some(Modal::PromptSearch {
        input: "x".to_string(),
    });
    assert!(!is_state_preserving_key(search.as_ref(), KeyCode::Enter));
    assert!(!is_state_preserving_key(search.as_ref(), KeyCode::Esc));

    let promote = Some(Modal::PromptPromoteName {
        input: "x".to_string(),
        error: None,
    });
    assert!(!is_state_preserving_key(promote.as_ref(), KeyCode::Enter));
    assert!(!is_state_preserving_key(promote.as_ref(), KeyCode::Esc));

    let reject = Some(Modal::PromptRejectReason {
        input: "x".to_string(),
        error: None,
    });
    assert!(!is_state_preserving_key(reject.as_ref(), KeyCode::Enter));
    assert!(!is_state_preserving_key(reject.as_ref(), KeyCode::Esc));
}

#[test]
fn is_state_preserving_key_is_false_for_every_other_modal_and_for_no_modal_at_all() {
    assert!(!is_state_preserving_key(
        Some(&Modal::Help { scroll: 0 }),
        KeyCode::Char('a')
    ));
    assert!(!is_state_preserving_key(None, KeyCode::Char('a')));
}

#[test]
fn is_state_preserving_key_is_true_for_pure_navigation_and_display_keys_with_no_modal_open() {
    for code in [
        KeyCode::Up,
        KeyCode::Char('k'),
        KeyCode::Down,
        KeyCode::Char('j'),
        KeyCode::Tab,
        KeyCode::Char('g'),
        KeyCode::Char('G'),
        KeyCode::Char('n'),
        KeyCode::Char('N'),
        KeyCode::Char('x'),
        KeyCode::Char('c'),
        KeyCode::Char('p'),
        KeyCode::Char('r'),
        KeyCode::Char('t'),
        KeyCode::Char('T'),
        KeyCode::Char('/'),
        KeyCode::Char('?'),
        KeyCode::Char('q'),
        KeyCode::Esc,
    ] {
        assert!(
            is_state_preserving_key(None, code),
            "{code:?} should not force a FrameState rebuild with no modal open"
        );
    }
}

#[test]
fn is_state_preserving_key_is_false_for_keys_that_can_mutate_mapping_or_collapse_state() {
    // `h`/`l`/`a`/`A` sometimes mutate a collapsed set; `m`/`M`/`f`/`d`/`D`/`i`/`I`/`u` mutate
    // the mapping directly; `H` toggles hide_solved; `s`/`R`/`o`/`O`/`C` are left on the
    // conservative (full-rebuild) path deliberately, per `is_navigation_or_display_key`'s doc
    // comment - none of these must be silently added to the fast path without also auditing
    // what they touch.
    for code in [
        KeyCode::Left,
        KeyCode::Char('h'),
        KeyCode::Right,
        KeyCode::Char('l'),
        KeyCode::Char('a'),
        KeyCode::Char('A'),
        KeyCode::Char('m'),
        KeyCode::Char('M'),
        KeyCode::Char('f'),
        KeyCode::Char('d'),
        KeyCode::Char('D'),
        KeyCode::Char('i'),
        KeyCode::Char('I'),
        KeyCode::Char('u'),
        KeyCode::Char('H'),
        KeyCode::Char('s'),
        KeyCode::Char('R'),
        KeyCode::Char('o'),
        KeyCode::Char('O'),
        KeyCode::Char('C'),
    ] {
        assert!(
            !is_state_preserving_key(None, code),
            "{code:?} must still force a FrameState rebuild with no modal open"
        );
    }
}

#[test]
fn count_unmarked_counts_only_nodes_with_no_match_or_delete_mark() {
    let source = "fn main() {\n    a();\n    b();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let (stmt_a, _) = two_statements(root);
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));

    let mut caches = Caches::default();
    let before_unmarked = count_unmarked(&flat, &caches, status_before);

    // Marking one statement's whole subtree matched must drop the unmarked count by exactly
    // the number of nodes under it, and by nothing else.
    let mut subtree_ids = Vec::new();
    collect_subtree_ids(stmt_a, &mut subtree_ids);
    mark_subtree_matched(stmt_a, &mut caches);

    let after_marking = count_unmarked(&flat, &caches, status_before);
    assert_eq!(before_unmarked - after_marking, subtree_ids.len());
}

#[test]
fn render_panel_only_scans_the_visible_window_not_the_whole_flat_list() {
    // A tree deep enough that only a handful of its nodes fit in a tiny terminal; asserts that
    // `render_panel` never touches (and never renders) anything outside that window, and that
    // the header's count comes from the caller-supplied `total_unmarked`, not a fresh scan.
    let mut source = String::from("fn main() {\n");
    for i in 0..200 {
        source.push_str(&format!("    stmt_{i}();\n"));
    }
    source.push_str("}\n");
    let tree = parse_rust(&source);
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    assert!(
        flat.len() > 200,
        "fixture should be far bigger than any plausible terminal height"
    );

    let caches = Caches::default();
    let mut panel = PanelState::new(root.id());
    // Tall enough to reach past the file's fixed preamble (source_file, function_item, fn,
    // identifier, parameters, (, ), block) down into the first statement's own leaves, but
    // nowhere near tall enough to reach the 199th one.
    let backend = ratatui::backend::TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 40, 20);

    // A deliberately wrong `total_unmarked` -- if `render_panel` still scanned the whole list
    // itself it would recompute (and show) the real count instead of trusting this value.
    terminal
        .draw(|f| {
            render_panel(
                f,
                area,
                "Before",
                &flat,
                &mut panel,
                &caches,
                Side::Before,
                source.as_bytes(),
                true,
                None,
                false,
                424242,
                &std::collections::BTreeSet::new(),
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("424242 unmarked"),
        "header should show the passed-in count verbatim: {text}"
    );
    assert!(
        text.contains("stmt_0"),
        "the first visible node should render: {text}"
    );
    assert!(
        !text.contains("stmt_199"),
        "a node far past the tiny viewport must not be scanned or rendered: {text}"
    );
}

#[test]
fn sample_source_deserializes_a_legacy_source_json_missing_dataset_as_small() {
    // No "dataset" key at all - the exact shape `materialize_test_diffs` wrote before
    // provenance tracking existed, still sitting in every sample materialized before then.
    let json = r#"{"language":"Rust","repository":"repo","commit":"abc123","path":"src/a.rs"}"#;
    let source: SampleSource = serde_json::from_str(json).unwrap();
    assert_eq!(source.dataset, "small");
}

#[test]
fn sample_source_deserializes_an_explicit_dataset_field() {
    let json = r#"{"language":"Rust","repository":"repo","commit":"abc123","path":"src/a.rs","dataset":"full"}"#;
    let source: SampleSource = serde_json::from_str(json).unwrap();
    assert_eq!(source.dataset, "full");
}

#[test]
fn default_promoted_name_lowercases_language_and_strips_dot_git() {
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "rustdesk-rustdesk.git".to_string(),
        commit: "abc12345".to_string(),
        path: "src/lang/kz.rs".to_string(),
        dataset: "small".to_string(),
    };
    assert_eq!(default_promoted_name(&source), "rust-rustdesk-rustdesk");
}

#[test]
fn default_promoted_name_leaves_a_repository_with_no_dot_git_suffix_alone() {
    let source = SampleSource {
        language: "Kotlin".to_string(),
        repository: "nextcloud-android".to_string(),
        commit: "abc12345".to_string(),
        path: "app/src/main/Foo.kt".to_string(),
        dataset: "small".to_string(),
    };
    assert_eq!(default_promoted_name(&source), "kotlin-nextcloud-android");
}

#[test]
fn default_promoted_name_for_path_lowercases_the_detected_language() {
    assert_eq!(default_promoted_name_for_path("src/diff.rs"), "rust-");
    assert_eq!(default_promoted_name_for_path("scripts/tool.py"), "python-");
}

#[test]
fn default_promoted_name_for_path_is_empty_for_an_undetected_language() {
    // Not a real reachable case in practice (`load_git_commit_file` already requires a
    // working `to_treesitter` mapping before a case can be opened at all), but this should
    // degrade to "no prefix", not panic, if it's ever called on something else.
    assert_eq!(default_promoted_name_for_path("README"), "");
}

#[test]
fn promote_target_dataset_is_none_for_diffs_and_handmade_for_a_git_commit_file() {
    assert_eq!(promote_target_dataset(&CaseOrigin::Diffs), None);
    assert_eq!(
        promote_target_dataset(&CaseOrigin::GitCommitFile {
            path: "src/diff.rs".to_string(),
        }),
        Some("handmade")
    );
}

#[test]
fn promote_target_dataset_is_the_samples_own_recorded_dataset() {
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc12345".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "full".to_string(),
    };
    assert_eq!(
        promote_target_dataset(&CaseOrigin::Sample(source)),
        Some("full")
    );
}

#[test]
fn action_reject_bails_when_current_case_is_not_a_sample() {
    let app = App::new(
        "some-case".to_string(),
        CaseOrigin::Diffs,
        0,
        0,
        HumanMapping::default(),
    );
    let err = action_reject(&app, "not a real reason").unwrap_err();
    assert!(format!("{:#}", err).contains("Only a sample"));
}

#[test]
fn action_reject_bails_on_an_empty_reason() {
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let app = App::new(
        "sample-name".to_string(),
        CaseOrigin::Sample(source),
        0,
        0,
        HumanMapping::default(),
    );
    // Whitespace-only trims to empty, same as a bare empty string would.
    let err = action_reject(&app, "   ").unwrap_err();
    assert!(format!("{:#}", err).contains("cannot be empty"));
}

#[test]
fn short_hash_takes_the_first_eight_characters() {
    assert_eq!(
        short_hash("58a776ecdef0123456789abcdef0123456789ab"),
        "58a776ec"
    );
}

#[test]
fn short_hash_does_not_panic_on_a_shorter_input() {
    assert_eq!(short_hash("abc"), "abc");
}

#[test]
fn advance_to_next_search_match_finds_the_next_leaf_containing_the_query_and_wraps_around() {
    let source = "fn main() {\n    alpha();\n    beta();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(root.id());

    let found = advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "beta").unwrap();
    assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "beta");
    assert_eq!(panel.cursor_id, found.id());

    // Searching again from `beta` for a query only `alpha` matches must wrap around past the
    // end of the file back to it, not report "not found" just because it's earlier in the
    // document.
    let found =
        advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "alpha").unwrap();
    assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "alpha");
}

#[test]
fn advance_to_next_search_match_only_matches_leaf_nodes_not_a_containers_concatenated_text() {
    // The `block`'s own text is the concatenation of everything inside it, so it "contains"
    // both "alpha" and "beta" -- but it must never be reported as a match, only the actual
    // leaf tokens should be.
    let source = "fn main() {\n    alpha();\n    beta();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(root.id());

    let found =
        advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "alpha").unwrap();
    assert_eq!(found.child_count(), 0);
    assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "alpha");
}

#[test]
fn advance_to_next_search_match_returns_none_and_leaves_the_cursor_put_when_nothing_matches() {
    let source = "fn main() {\n    alpha();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(root.id());
    let original_cursor = panel.cursor_id;

    let found = advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "nonexistent");
    assert!(found.is_none());
    assert_eq!(panel.cursor_id, original_cursor);
}

#[test]
fn action_search_reports_the_matched_nodes_kind_and_moves_the_focused_panels_cursor() {
    let before_source = "fn main() {\n    alpha();\n}\n";
    let after_source = "fn main() {\n    beta();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));

    let msg = action_search(
        &mut app,
        Focus::Before,
        &before_flat,
        &after_flat,
        before_source.as_bytes(),
        after_source.as_bytes(),
        "alpha",
    )
    .unwrap();
    assert!(msg.contains("Found 'alpha'") || msg.contains("Found \"alpha\""));
    assert_eq!(
        before_flat
            .iter()
            .find(|(n, _)| n.id() == app.before.cursor_id)
            .unwrap()
            .0
            .utf8_text(before_source.as_bytes())
            .unwrap(),
        "alpha"
    );
    // The After panel's cursor must be untouched -- the search only ever moves the focused
    // panel's cursor.
    assert_eq!(app.after.cursor_id, after_root.id());
}

#[test]
fn action_search_errors_when_nothing_in_the_focused_panel_matches() {
    let source = "fn main() {\n    alpha();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));

    let err = action_search(
        &mut app,
        Focus::Before,
        &flat,
        &flat,
        source.as_bytes(),
        source.as_bytes(),
        "nonexistent",
    )
    .unwrap_err();
    assert!(format!("{err}").contains("No node containing"));
}

#[test]
fn handle_modal_key_prompt_search_enter_finds_a_match_and_remembers_the_query() {
    let source = "fn main() {\n    alpha();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::PromptSearch {
        input: "alpha".to_string(),
    });

    handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert_eq!(app.last_search.as_deref(), Some("alpha"));
    assert!(
        app.status.as_deref().unwrap_or("").contains("Found"),
        "expected a 'Found' status message, got {:?}",
        app.status
    );
    assert!(app.modal.is_none(), "the modal should close either way");
}

#[test]
fn handle_modal_key_prompt_search_enter_on_empty_input_cancels_without_touching_last_search() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    app.last_search = Some("previous".to_string());
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::PromptSearch {
        input: "   ".to_string(),
    });

    handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert_eq!(
        app.last_search.as_deref(),
        Some("previous"),
        "an empty/whitespace-only query must not overwrite the remembered last search"
    );
    assert_eq!(app.status.as_deref(), Some("Search cancelled: empty query"));
    assert!(app.modal.is_none());
}

#[test]
fn handle_modal_key_prompt_search_esc_cancels_without_searching() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::PromptSearch {
        input: "alpha".to_string(),
    });

    handle_modal_key(
        &mut app,
        KeyCode::Esc,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(app.last_search.is_none());
    assert_eq!(app.status.as_deref(), Some("Cancelled"));
    assert!(app.modal.is_none());
}

#[test]
fn handle_modal_key_prompt_search_backspace_and_char_edit_the_input_and_keep_the_modal_open() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::PromptSearch {
        input: "alp".to_string(),
    });

    handle_modal_key(
        &mut app,
        KeyCode::Backspace,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    match &app.modal {
        Some(Modal::PromptSearch { input }) => assert_eq!(input, "al"),
        other => panic!("expected Modal::PromptSearch to stay open, got {other:?}"),
    }

    handle_modal_key(
        &mut app,
        KeyCode::Char('x'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    match &app.modal {
        Some(Modal::PromptSearch { input }) => assert_eq!(input, "alx"),
        other => panic!("expected Modal::PromptSearch to stay open, got {other:?}"),
    }
}

#[test]
fn render_modal_prompt_search_shows_the_prefilled_query_and_instructions() {
    let backend = ratatui::backend::TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 80, 20);
    let modal = Modal::PromptSearch {
        input: "alpha".to_string(),
    };

    terminal
        .draw(|f| {
            render_modal(
                f,
                area,
                &modal,
                "test",
                None,
                "",
                "",
                &HumanMapping::default(),
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                DiffPickerData::default(),
                None,
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("Search node text"),
        "title missing from render: {text}"
    );
    assert!(text.contains("alpha"), "prefilled query missing: {text}");
    assert!(text.contains("find next"), "instructions missing: {text}");
}

#[test]
fn render_modal_prompt_promote_name_shows_the_actual_target_dataset_not_a_fixed_one() {
    // Regression guard for the bug this replaced: the prompt used to name a hardcoded
    // "small" folder no matter what `promote_dataset` (i.e. `app.origin`) actually was.
    let backend = ratatui::backend::TestBackend::new(90, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 90, 20);
    let modal = Modal::PromptPromoteName {
        input: "rust-".to_string(),
        error: None,
    };

    terminal
        .draw(|f| {
            render_modal(
                f,
                area,
                &modal,
                "test",
                Some("handmade"),
                "",
                "",
                &HumanMapping::default(),
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                DiffPickerData::default(),
                None,
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("diffs/handmade/"),
        "target dataset missing or wrong from render: {text}"
    );
    assert!(
        !text.contains("diffs/small/"),
        "stale dataset in render: {text}"
    );
}

#[test]
fn run_unix_diff_reports_no_differences_for_identical_content() {
    let output = run_unix_diff(b"fn main() {}\n", b"fn main() {}\n").unwrap();
    assert_eq!(output, "(no textual differences)");
}

#[test]
fn run_unix_diff_shows_added_and_removed_lines() {
    let output = run_unix_diff(b"line one\nline two\n", b"line one\nline three\n").unwrap();
    assert!(output.contains("-line two"));
    assert!(output.contains("+line three"));
    assert!(output.contains("--- before"));
    assert!(output.contains("+++ after"));
}

#[test]
fn raw_before_after_reads_the_before_and_after_test_files_from_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before.rs.test"), "fn old() {}\n").unwrap();
    fs::write(dir.path().join("after.rs.test"), "fn new() {}\n").unwrap();
    fs::write(dir.path().join("source.json"), "{}").unwrap();

    let (before, after) = raw_before_after(dir.path()).expect("both files should be found");
    assert_eq!(before, "fn old() {}\n");
    assert_eq!(after, "fn new() {}\n");
}

#[test]
fn raw_before_after_is_none_when_a_file_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before.rs.test"), "fn old() {}\n").unwrap();
    // No after.*.test written.

    assert!(raw_before_after(dir.path()).is_none());
}

#[test]
fn sample_diff_line_count_counts_only_changed_lines_not_context_or_headers() {
    let dir = tempfile::tempdir().unwrap();
    // Same directory layout `sample_diff_line_count` reads via `samples_root().join(name)` -
    // exercised directly against a real temp directory here instead, via `raw_before_after` +
    // `run_unix_diff` (the same two calls `sample_diff_line_count` itself makes), since
    // `samples_root()` is hardcoded to this crate's own `src/test/data/samples`.
    fs::write(
        dir.path().join("before.rs.test"),
        "fn main() {\n    old();\n    same();\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("after.rs.test"),
        "fn main() {\n    new();\n    same();\n}\n",
    )
    .unwrap();

    let (before, after) = raw_before_after(dir.path()).unwrap();
    let diff = run_unix_diff(before.as_bytes(), after.as_bytes()).unwrap();
    let count = diff
        .lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .count();
    assert_eq!(
        count, 2,
        "one removed + one added line, not the 2 unchanged: {diff}"
    );
}

#[test]
fn sample_diff_line_count_is_zero_for_a_nonexistent_sample() {
    assert_eq!(sample_diff_line_count("does-not-exist-at-all"), 0);
}

#[test]
fn sample_diff_line_count_is_nonzero_for_a_real_sample_on_disk() {
    // Full integrated path (`samples_root().join(name)`, not a crafted temp dir) against
    // whatever's actually checked in under src/test/data/samples/ - skips rather than fails if
    // none exist yet in this checkout (materialize_test_diffs hasn't run), matching
    // `list_dir_names`'s own "not an error" treatment of a missing/empty samples/ directory.
    let Ok(names) = list_dir_names(&samples_root()) else {
        return;
    };
    let Some(name) = names.first() else {
        return;
    };
    assert!(
        sample_diff_line_count(name) > 0,
        "sample '{name}' should have at least one changed line"
    );
}

/// Concatenates every cell's symbol in a `TestBackend`'s buffer, so a rendered frame's
/// content can be checked with a plain `contains`.
/// Builds a view with the cursor on `column`, everything else default.
fn column_view(column: DiffColumn) -> DiffPickerView {
    DiffPickerView {
        column,
        ..DiffPickerView::default()
    }
}

/// Builds a view sorted ascending by `column`, everything else default.
fn sort_view(column: DiffColumn) -> DiffPickerView {
    DiffPickerView {
        sort: DiffSort::default().toggled(column),
        ..DiffPickerView::default()
    }
}

/// Builds a view with only the `Dataset` filter set.
fn dataset_view(dataset: Option<&'static str>) -> DiffPickerView {
    DiffPickerView {
        filters: DiffFilters {
            dataset,
            ..DiffFilters::default()
        },
        ..DiffPickerView::default()
    }
}

/// Builds a view with only the `Name` substring filter set.
fn name_view(needle: &str) -> DiffPickerView {
    DiffPickerView {
        filters: DiffFilters {
            name: Some(needle.to_string()),
            ..DiffFilters::default()
        },
        ..DiffPickerView::default()
    }
}

/// Builds a view with one flag filter set, everything else default.
fn flag_view(column: DiffColumn, filter: FlagFilter) -> DiffPickerView {
    let mut view = DiffPickerView::default();
    *view
        .filters
        .flag_mut(column)
        .expect("column has a flag filter") = filter;
    view
}

/// The `Paint` column's filter narrows in both directions, and a case the scan never reached
/// survives *either* of them - see `FlagFilter::keeps` for why that fail-open rule is uniform
/// across columns and directions rather than argued per filter.
#[test]
fn visible_diff_options_can_narrow_to_painted_or_unpainted_cases() {
    let options = vec![
        ("painted".to_string(), "handmade"),
        ("unpainted".to_string(), "handmade"),
        ("never-scanned".to_string(), "handmade"),
    ];
    let painted = std::collections::HashMap::from([
        ("painted".to_string(), true),
        ("unpainted".to_string(), false),
    ]);
    let data = DiffPickerData {
        text_painted: Some(&painted),
        ..DiffPickerData::default()
    };

    assert_eq!(
        visible_diff_options(&options, &DiffPickerView::default(), data),
        vec!["never-scanned", "painted", "unpainted"],
        "the filter off shows everything"
    );
    assert_eq!(
        visible_diff_options(
            &options,
            &flag_view(DiffColumn::Paint, FlagFilter::No),
            data
        ),
        vec!["never-scanned", "unpainted"],
        "'unpainted only' hides only cases confirmed painted"
    );
    assert_eq!(
        visible_diff_options(
            &options,
            &flag_view(DiffColumn::Paint, FlagFilter::Yes),
            data
        ),
        vec!["never-scanned", "painted"],
        "'painted only' hides only cases confirmed unpainted"
    );
}

/// Two columns' filters are separate queues over separate ground truths, so turning both on
/// shows only what matches both - the AND the picker's whole compound-query story rests on.
#[test]
fn filters_on_different_columns_combine_as_an_and() {
    let options = vec![
        ("both".to_string(), "handmade"),
        ("tree-only".to_string(), "handmade"),
        ("text-only".to_string(), "handmade"),
        ("done".to_string(), "handmade"),
    ];
    let unmarked = std::collections::HashMap::from([
        ("both".to_string(), 3),
        ("tree-only".to_string(), 7),
        ("text-only".to_string(), 0),
        ("done".to_string(), 0),
    ]);
    let painted = std::collections::HashMap::from([
        ("both".to_string(), false),
        ("tree-only".to_string(), true),
        ("text-only".to_string(), false),
        ("done".to_string(), true),
    ]);
    let data = DiffPickerData {
        unmarked: Some(&unmarked),
        text_painted: Some(&painted),
        ..DiffPickerData::default()
    };

    let mut view = flag_view(DiffColumn::Cmpl, FlagFilter::Yes);
    view.filters.paint = FlagFilter::No;

    assert_eq!(
        visible_diff_options(&options, &view, data),
        vec!["both"],
        "only the case needing both an unmarked node and a painting survives"
    );
}

/// `s` on a column takes the sort over and orders by it; pressing it again flips the
/// direction. The name is always the tiebreak, so rows that tie on the sorted column - every
/// row, when its scan hasn't run - still come out alphabetically rather than in whatever
/// order `options` happened to arrive in.
#[test]
fn visible_diff_options_sorts_by_the_selected_column_with_a_name_tiebreak() {
    let options = vec![
        ("charlie".to_string(), "handmade"),
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
        ("delta".to_string(), "handmade"),
    ];
    // `delta` is deliberately absent: an unscanned/unloadable case sorts after every known
    // one ascending, and to the front when the direction flips.
    let unmarked = std::collections::HashMap::from([
        ("charlie".to_string(), 1),
        ("alpha".to_string(), 9),
        ("bravo".to_string(), 1),
    ]);
    let data = DiffPickerData {
        unmarked: Some(&unmarked),
        ..DiffPickerData::default()
    };

    let mut view = sort_view(DiffColumn::Unmarked);
    assert_eq!(
        visible_diff_options(&options, &view, data),
        vec!["bravo", "charlie", "alpha", "delta"],
        "ascending by count, ties broken by name, unknown last"
    );

    view.sort = view.sort.toggled(DiffColumn::Unmarked);
    assert_eq!(
        visible_diff_options(&options, &view, data),
        vec!["delta", "alpha", "bravo", "charlie"],
        "pressing s again reverses the column, but ties still break by name ascending"
    );

    let name_sorted = DiffPickerView::default();
    assert_eq!(
        visible_diff_options(&options, &name_sorted, data),
        vec!["alpha", "bravo", "charlie", "delta"],
        "the default sort is the Name column, A-Z"
    );
}

/// The `Name` filter is a case-insensitive substring over the case name, and is the one filter
/// that needs no corpus scan at all.
#[test]
fn visible_diff_options_narrows_by_a_name_substring() {
    let options = vec![
        ("rust-add-if".to_string(), "handmade"),
        ("java-add-exception".to_string(), "small"),
        ("rust-hash".to_string(), "handmade"),
    ];
    let view = name_view("rust-");

    assert_eq!(
        visible_diff_options(&options, &view, DiffPickerData::default()),
        vec!["rust-add-if", "rust-hash"]
    );
}

/// The picker offers this fixture's own paintings first, then whichever suggestions it lacks -
/// so resaving the painting being edited is at the top, not buried under three constants.
#[test]
fn the_solution_picker_lists_existing_paintings_before_suggestions() {
    let mut app = test_app();
    app.mapping.text_mappings = vec![NamedTextMapping {
        name: "Full".to_string(),
        mapping: HumanTextMapping::default(),
    }];

    assert_eq!(
        solution_picker_names(&app.mapping),
        vec!["Full", "Minimal", "Only one solution"],
        "an existing name must not be offered twice"
    );
}

/// Presses one key on the main view of a case with `origin`, and hands back the App.
///
/// Drives `handle_key`'s dispatch rather than `action_comment`, for the reason the solution
/// picker's harness gives: `x` and `c` were once implemented, unit-tested through their
/// actions, and unreachable because the key arm was never written.
fn press_on_case(origin: CaseOrigin, name: &str, code: KeyCode) -> App {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        name.to_string(),
        origin,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    let hashes = rustc_hash::FxHashMap::default();
    handle_key(
        &mut app,
        code,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &hashes,
        &hashes,
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    app
}

/// `e` on a diff opens the comment prompt. It used to refuse - comments lived only in
/// sample.csv, which a handmade fixture has no row in at all.
#[test]
fn e_on_a_diff_opens_the_comment_prompt() {
    let app = press_on_case(CaseOrigin::Diffs, "rust-no-change", KeyCode::Char('e'));
    match &app.modal {
        Some(Modal::PromptComment { input, .. }) => assert!(
            input.contains("identical"),
            "must pre-fill from the existing description.md, got {input:?}"
        ),
        other => panic!("expected Modal::PromptComment, got {other:?}"),
    }
}

/// A diff with no description.md yet still gets the prompt - just an empty one.
#[test]
fn e_on_an_un_noted_diff_opens_an_empty_prompt() {
    let app = press_on_case(
        CaseOrigin::Diffs,
        "rust-hello-world-added-message",
        KeyCode::Char('e'),
    );
    match &app.modal {
        Some(Modal::PromptComment { input, .. }) => assert_eq!(input, ""),
        other => panic!("expected Modal::PromptComment, got {other:?}"),
    }
}

/// A case that is neither a diff nor a sample - `C`'s git-commit view - still has nowhere to
/// put a comment, and says so rather than opening a prompt that could not be saved.
#[test]
fn e_on_a_git_commit_case_refuses_with_a_message() {
    let app = press_on_case(
        CaseOrigin::GitCommitFile {
            path: "src/main.rs".to_string(),
        },
        "abc1234",
        KeyCode::Char('e'),
    );
    assert!(app.modal.is_none(), "no prompt should open");
    assert!(
        app.status
            .as_deref()
            .unwrap_or_default()
            .contains("diff (o) or sample (O)"),
        "{:?}",
        app.status
    );
}

/// **A promoted fixture's note lives in its own `description.md`, and nowhere else.**
///
/// Stronger than the rule this replaced, which only asked that a sample.csv comment also have
/// a file - that allowed two copies, and two copies drift: of the 19 promoted rows that
/// carried a comment, 18 matched their `description.md` byte for byte and one had already
/// diverged. Promotion now *moves* the note (see `update_sample_csv_at`) rather than copying
/// it, so the correct state is no comment at all on a promoted row, and that is what this
/// checks.
///
/// A rejected or still-untriaged row keeps its comment: there is no fixture directory to put
/// it in, and for a rejection the reason is the only record of the decision.
#[test]
fn no_promoted_row_carries_a_comment() {
    let rows = read_sample_csv_rows(&sample_csv_path()).expect("sample.csv");
    let duplicated: Vec<&str> = rows
        .iter()
        .filter(|row| !row.promoted_to.trim().is_empty() && !row.comment.trim().is_empty())
        .map(|row| row.promoted_to.as_str())
        .collect();
    assert!(
        duplicated.is_empty(),
        "these promoted fixtures still carry a sample.csv comment, a second home for a note \
         that belongs only in their own description.md: {duplicated:?}"
    );
}

/// Opens the solution picker over a fixture holding `paintings`, and presses `keys` in it.
///
/// Drives `handle_modal_key` rather than `action_delete_solution`, because that is the layer
/// where this feature has already failed once: `x` and `c` were implemented, unit-tested
/// through their actions, and unreachable for a week because the key arm was never added.
fn press_in_solution_picker(paintings: &[&str], keys: &[KeyCode]) -> App {
    press_in_solution_picker_editing(paintings, None, keys)
}

/// As above, but editing `editing` rather than whichever painting `starting_solution` picks -
/// the only way to reach the case where the deleted painting is *not* the current one.
fn press_in_solution_picker_editing(
    paintings: &[&str],
    editing: Option<&str>,
    keys: &[KeyCode],
) -> App {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = test_app();
    app.mapping.text_mappings = paintings
        .iter()
        .map(|name| NamedTextMapping {
            name: (*name).to_string(),
            mapping: HumanTextMapping::default(),
        })
        .collect();
    app.text_solution = editing
        .map(str::to_string)
        .unwrap_or_else(|| starting_solution(&app.mapping));

    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::SolutionPicker {
        names: solution_picker_names(&app.mapping),
        selected: 0,
        saving: false,
        new_name: None,
        confirm_delete: None,
        state: TextPaintState::default(),
    });

    for &code in keys {
        handle_modal_key(
            &mut app,
            code,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
    }
    app
}

fn painting_names(app: &App) -> Vec<String> {
    app.mapping
        .text_mappings
        .iter()
        .map(|named| named.name.clone())
        .collect()
}

/// One `D` arms, the second fires. There is no undo for a painting that may have taken an
/// hour, so a single keystroke next to `d` (delete-range) must not be able to destroy one.
#[test]
fn deleting_a_painting_takes_two_presses_of_d() {
    let armed = press_in_solution_picker(&["Full", "Minimal"], &[KeyCode::Char('D')]);
    assert_eq!(
        painting_names(&armed),
        vec!["Full", "Minimal"],
        "one D must only arm the confirmation"
    );
    match &armed.modal {
        Some(Modal::SolutionPicker { confirm_delete, .. }) => {
            assert_eq!(confirm_delete.as_deref(), Some("Full"))
        }
        other => panic!("expected the picker to stay open, got {other:?}"),
    }

    let deleted = press_in_solution_picker(
        &["Full", "Minimal"],
        &[KeyCode::Char('D'), KeyCode::Char('D')],
    );
    assert_eq!(painting_names(&deleted), vec!["Minimal"]);
}

/// The case this exists for: a fixture painted once turns out to need a Minimal and a Full
/// answer, so the single painting has to go before the pair can be made.
#[test]
fn deleting_the_last_painting_leaves_the_fixture_unpainted() {
    let app = press_in_solution_picker(
        &["Only one solution"],
        &[KeyCode::Char('D'), KeyCode::Char('D')],
    );

    assert!(
        app.mapping.text_mappings.is_empty(),
        "the fixture goes back to unpainted, not to an empty painting - those are different \
         states, and only the first is what `X` and diffs.csv should report"
    );
    assert!(app.dirty, "the deletion still has to be saved");
    assert!(
        app.status
            .as_deref()
            .unwrap_or_default()
            .contains("unpainted"),
        "the reader has to be told the fixture is now unpainted: {:?}",
        app.status
    );
}

/// The confirmation is about a painting, not about a row. Moving the cursor between the two
/// presses must not let the second `D` land on whatever ended up highlighted instead.
#[test]
fn moving_the_cursor_cancels_a_pending_deletion() {
    let app = press_in_solution_picker(
        &["Full", "Minimal"],
        &[KeyCode::Char('D'), KeyCode::Char('j'), KeyCode::Char('D')],
    );

    assert_eq!(
        painting_names(&app),
        vec!["Full", "Minimal"],
        "j cleared the confirmation, so the second D only re-armed - on Minimal"
    );
    match &app.modal {
        Some(Modal::SolutionPicker { confirm_delete, .. }) => {
            assert_eq!(confirm_delete.as_deref(), Some("Minimal"))
        }
        other => panic!("expected the picker to stay open, got {other:?}"),
    }
}

/// Deleting a painting you are not editing must not move you. The harness's default always
/// deletes the current one (the picker lists it first), so this is the case the other tests
/// structurally cannot reach.
#[test]
fn deleting_another_painting_leaves_you_editing_your_own() {
    // Three paintings, not two: `starting_solution` returns the *first* survivor, so with two
    // it happens to return the one being edited anyway and an unguarded reset would look
    // correct. Deleting "Full" while editing "Custom" leaves "Minimal" first, so the two
    // behaviours finally differ.
    let app = press_in_solution_picker_editing(
        &["Full", "Minimal", "Custom"],
        Some("Custom"),
        &[KeyCode::Char('D'), KeyCode::Char('D')],
    );

    assert_eq!(painting_names(&app), vec!["Minimal", "Custom"]);
    assert_eq!(
        app.text_solution, "Custom",
        "deleting 'Full' has nothing to do with which painting the reader is in"
    );
    assert!(
        app.status
            .as_deref()
            .unwrap_or_default()
            .contains("still editing"),
        "{:?}",
        app.status
    );
}

/// The picker lists suggestions the fixture has not used yet alongside its real paintings.
/// Pressing `D` on one of those is a misunderstanding, not a request, and must not silently
/// look like it worked.
#[test]
fn d_on_an_unused_suggestion_deletes_nothing() {
    // "Full" exists; "Minimal" and "Only one solution" are offered but unused, so j lands on a
    // suggestion.
    let app = press_in_solution_picker(
        &["Full"],
        &[KeyCode::Char('j'), KeyCode::Char('D'), KeyCode::Char('D')],
    );

    assert_eq!(painting_names(&app), vec!["Full"]);
    assert!(
        app.status
            .as_deref()
            .unwrap_or_default()
            .contains("only a suggestion"),
        "saying so beats an armed confirmation for something that cannot be deleted: {:?}",
        app.status
    );
    assert!(!app.dirty, "nothing changed, so nothing needs saving");
}

/// The property this whole feature exists for: a fixture can hold more than one painting at
/// once. An earlier version *moved* the ranges to the new name and dropped the old one, so
/// however many times you saved you still ended up with exactly one.
#[test]
fn branching_keeps_both_paintings_on_file() {
    let (before_src, after_src) = ("gone\n", "\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 3);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );
    assert_eq!(app.text_solution, "Minimal");

    action_save_solution_as(&mut app, "Full", true);

    assert_eq!(
        app.text_solution, "Full",
        "editing continues on the new one"
    );
    assert_eq!(
        app.mapping.text_mappings.len(),
        2,
        "both paintings must survive: {:?}",
        app.mapping.text_mappings
    );
    assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
    assert_eq!(
        solution_entries(&app.mapping, "Full").len(),
        1,
        "a copy starts from what was painted"
    );
}

/// Two answers to one edit usually share most of their spans, so `e` (start empty) is the
/// rarer option and Enter copies - but both have to be reachable.
#[test]
fn branching_empty_starts_the_new_painting_from_nothing() {
    let (before_src, after_src) = ("gone\n", "\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 3);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    action_save_solution_as(&mut app, "Full", false);

    assert_eq!(app.mapping.text_mappings.len(), 2);
    assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
    assert!(solution_entries(&app.mapping, "Full").is_empty());
}

/// Picking a name that already exists must never write: merging would leave overlapping
/// duplicates, replacing would silently discard somebody's work, and there is no undo here.
#[test]
fn branching_to_an_existing_name_switches_without_overwriting_it() {
    let mut app = test_app();
    app.mapping.text_mappings = vec![
        NamedTextMapping {
            name: "Minimal".to_string(),
            mapping: HumanTextMapping::default(),
        },
        NamedTextMapping {
            name: "Full".to_string(),
            mapping: HumanTextMapping {
                entries: vec![HumanTextEntry {
                    operation: HumanTextOperation::Delete,
                    before: vec![HumanTextSpan {
                        start_row: 0,
                        start_column: 0,
                        end_row: 0,
                        end_column: 4,
                    }],
                    after: vec![],
                }],
            },
        },
    ];

    action_save_solution_as(&mut app, "Full", true);

    assert_eq!(app.text_solution, "Full");
    assert_eq!(app.mapping.text_mappings.len(), 2);
    assert_eq!(
        solution_entries(&app.mapping, "Full").len(),
        1,
        "the existing painting must be untouched, not merged into or replaced"
    );
    assert!(
        app.status
            .as_deref()
            .unwrap_or("")
            .contains("already exists"),
        "the reader has to be told nothing was written: {:?}",
        app.status
    );
}

#[test]
fn loading_switches_which_painting_is_edited_without_touching_any() {
    let mut app = test_app();
    app.mapping.text_mappings = vec![
        NamedTextMapping {
            name: "Minimal".to_string(),
            mapping: HumanTextMapping::default(),
        },
        NamedTextMapping {
            name: "Full".to_string(),
            mapping: HumanTextMapping::default(),
        },
    ];

    action_load_solution(&mut app, "Full");

    assert_eq!(app.text_solution, "Full");
    assert_eq!(app.mapping.text_mappings.len(), 2);
    assert!(!app.dirty, "switching which one you edit changes nothing");
}

/// Two paintings must not bleed into each other: painting under one name and switching leaves
/// the other empty.
#[test]
fn ranges_painted_under_one_name_stay_out_of_another() {
    let (before_src, after_src) = ("gone\n", "\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 3);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    action_load_solution(&mut app, "Full");

    assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
    assert!(solution_entries(&app.mapping, "Full").is_empty());
}

/// The overlay cycles through all four and back, so `p` is always the only key needed.
#[test]
fn the_text_overlay_cycles_human_codediff_disagreements_tree_disagreement() {
    assert_eq!(TextOverlay::default(), TextOverlay::Human);
    assert_eq!(TextOverlay::Human.next(), TextOverlay::CodeDiff);
    assert_eq!(TextOverlay::CodeDiff.next(), TextOverlay::Disagreements);
    assert_eq!(
        TextOverlay::Disagreements.next(),
        TextOverlay::TreeDisagreement
    );
    assert_eq!(TextOverlay::TreeDisagreement.next(), TextOverlay::Human);
}

/// codediff's own rendering comes through `TextDiff`, the projection the real TUI and the
/// mapping site both draw - so what the solver shows is what codediff produces, not a second
/// reading of its node mapping.
#[test]
fn codediff_text_spans_reports_the_changed_regions_of_a_real_diff() {
    let before = Code::from_string("fn main() {\n    foo();\n}\n", &Language::Rust);
    let after = Code::from_string("fn main() {\n    bar();\n}\n", &Language::Rust);

    let [before_spans, after_spans] = codediff_text_spans(&before, &after);

    assert!(
        !before_spans.is_empty(),
        "a renamed call must show as changed"
    );
    assert!(!after_spans.is_empty());
    // Identical text contributes nothing: the untouched `fn main() {` line is not painted.
    assert!(
        before_spans.iter().all(|(span, _)| span.start_row > 0),
        "row 0 is unchanged and should carry no span: {before_spans:?}"
    );
}

/// The disagreement overlay is empty exactly when the two accounts agree, which is the signal
/// a person painting ground truth is actually looking for.
#[test]
fn the_disagreement_overlay_is_empty_when_the_two_accounts_match() {
    let (before_src, after_src) = ("alpha\n", "beta\n");
    let spans = [
        vec![(
            HumanTextSpan {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 5,
            },
            HumanTextVerdict::Update,
        )],
        vec![(
            HumanTextSpan {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 4,
            },
            HumanTextVerdict::Update,
        )],
    ];

    let same = overlay_disagreement_spans(&spans, &spans, before_src, after_src);
    assert!(
        same[0].is_empty() && same[1].is_empty(),
        "identical accounts must show nothing: {same:?}"
    );

    let empty = [Vec::new(), Vec::new()];
    let differing = overlay_disagreement_spans(&spans, &empty, before_src, after_src);
    assert!(
        !differing[0].is_empty(),
        "text one side calls changed and the other does not must show"
    );
}

/// The painted ranges use the shared overlay palette, not hardcoded ANSI colours - so a range
/// looks the same here as the same range does in the `codediff` TUI.
#[test]
fn painted_ranges_use_the_shared_overlay_palette() {
    let palette = OverlayTheme::default().palette();

    assert_eq!(
        verdict_style(HumanTextVerdict::Move).bg,
        Some(palette.move_bg)
    );
    assert_eq!(
        verdict_style(HumanTextVerdict::Update).bg,
        Some(palette.update_bg)
    );
    assert_eq!(
        verdict_style(HumanTextVerdict::Delete).bg,
        Some(palette.delete_bg)
    );
    assert_eq!(
        verdict_style(HumanTextVerdict::Insert).bg,
        Some(palette.insert_bg)
    );
    assert_eq!(
        verdict_style(HumanTextVerdict::Move).fg,
        Some(palette.overlay_fg)
    );
}

/// A multi-row span's middle row is fully covered up to its last real character, but never
/// past it - a human painting never means to paint a line's trailing whitespace, and least of
/// all its newline.
#[test]
fn span_covers_stops_at_the_last_real_character_of_a_middle_row() {
    let span = HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 2,
        end_column: 1,
    };
    let row_len = 3; // row 1 is "foo"

    assert!(
        span_covers(span, 1, 2, row_len),
        "the last real character must still be covered"
    );
    assert!(
        !span_covers(span, 1, 3, row_len),
        "the position past the last character is the newline - must not be covered"
    );
}

/// A blank row caught in the middle of a multi-row span has no character of its own, so it is
/// still reported as covered at column 0 - `render_paint_side`'s one-space fallback is what
/// makes that visible, and it needs `span_covers` to say the row belongs to the span at all.
#[test]
fn span_covers_still_covers_a_blank_middle_row() {
    let span = HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 2,
        end_column: 1,
    };

    assert!(span_covers(span, 1, 0, 0));
}

/// The span's own end row is unaffected either way - `end_column` already says exactly where
/// the human stopped painting.
#[test]
fn span_covers_uses_the_exact_end_column_on_the_last_row() {
    let span = HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    };

    assert!(span_covers(span, 0, 2, 5));
    assert!(!span_covers(span, 0, 3, 5));
}

/// Unset means Dracula, which is what makes these render tests deterministic rather than
/// dependent on whatever `.codediff.toml` the machine running them happens to hold.
#[test]
fn the_palette_falls_back_to_the_default_theme_when_none_was_installed() {
    assert_eq!(
        overlay_palette().move_bg,
        OverlayTheme::Dracula.palette().move_bg
    );
}

/// A live selection is drawn in the same colour the TUI paints a cursor's counterpart: both
/// mean "the region you are pointing at".
#[test]
fn a_selection_uses_the_cross_panel_highlight_colour() {
    assert_eq!(
        paint_class_style(PaintClass::Selected).bg,
        Some(OverlayTheme::default().palette().cross_highlight_bg)
    );
}

/// The shape this exists for: several ranges banked on each side commit as ONE match, so three
/// occurrences before against two after is a single correspondence rather than five.
#[test]
fn x_banks_ranges_so_m_can_commit_an_n_to_m_match() {
    let (before_src, after_src) = ("foo\nfoo\nfoo\n", "bar\nbar\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();

    // Two banked before-ranges plus a live third; two live/banked after-ranges.
    state.pending[0] = vec![
        HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 3,
        },
        HumanTextSpan {
            start_row: 1,
            start_column: 0,
            end_row: 1,
            end_column: 3,
        },
    ];
    state.anchor[0] = Some((2, 0));
    state.cursor[0] = (2, 2);
    state.pending[1] = vec![HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    }];
    state.anchor[1] = Some((1, 0));
    state.cursor[1] = (1, 2);

    action_paint_match(&mut app, &mut state, before_src, after_src);

    let entries = solution_entries(&app.mapping, &app.text_solution);
    assert_eq!(entries.len(), 1, "one entry, not five");
    assert_eq!(entries[0].before.len(), 3);
    assert_eq!(entries[0].after.len(), 2);
    assert_eq!(
        entries[0].verdict(before_src, after_src).unwrap(),
        HumanTextVerdict::Update,
        "foo against bar is an edit"
    );
    assert_eq!(state.pending, [vec![], vec![]], "the bank is consumed");
    assert!(
        app.status.as_deref().unwrap_or("").contains("3:2"),
        "the shape should be reported: {:?}",
        app.status
    );
}

/// The live selection commits along with the bank, so forgetting the final `x` before `m`
/// doesn't silently drop a range - a loss this view has no undo for.
#[test]
fn the_live_selection_commits_together_with_banked_ranges() {
    let source = "foo\nfoo\n";
    let mut state = TextPaintState::default();
    state.pending[0] = vec![HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    }];
    state.anchor[0] = Some((1, 0));
    state.cursor[0] = (1, 2);

    assert_eq!(state.committable(0, source).len(), 2);
}

/// A group whose spans disagree within one side is refused at the keystroke, while the
/// selection is still on screen to fix - not silently stored to fail at save time.
#[test]
fn m_refuses_a_group_whose_spans_differ_within_a_side() {
    let (before_src, after_src) = ("foo\nqux\n", "bar\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.pending[0] = vec![HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    }];
    state.anchor[0] = Some((1, 0));
    state.cursor[0] = (1, 2);
    state.anchor[1] = Some((0, 0));
    state.cursor[1] = (0, 2);

    action_paint_match(&mut app, &mut state, before_src, after_src);

    assert!(
        app.mapping.text_mappings.is_empty(),
        "nothing should have been stored"
    );
    assert!(!app.dirty);
    assert!(
        app.status
            .as_deref()
            .unwrap_or("")
            .contains("identical text"),
        "the reason has to reach the human: {:?}",
        app.status
    );
    assert_eq!(
        state.pending[0].len(),
        1,
        "the selection must survive so it can be corrected"
    );
}

#[test]
fn d_paints_every_banked_range_as_one_entry() {
    let (before_src, after_src) = ("foo\nbar\n", "\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.pending[0] = vec![HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    }];
    state.anchor[0] = Some((1, 0));
    state.cursor[0] = (1, 2);

    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    let entries = solution_entries(&app.mapping, &app.text_solution);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].before.len(),
        2,
        "one decision covering both ranges, not two decisions"
    );
}

/// Drives the text view through `handle_modal_key` rather than calling the actions directly,
/// because the bug this pins was a missing *key binding*, not a broken action: `x` banked
/// nothing because its match arm was never wired into the text view at all, while every test
/// that called `action_paint_match` with a pre-filled bank kept passing.
#[test]
fn x_in_the_text_view_banks_the_live_selection() {
    let source = "foo\nfoo\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 2);
    app.modal = Some(Modal::TextView { state });

    handle_modal_key(
        &mut app,
        KeyCode::Char('x'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("the text view should still be open, got {:?}", app.modal);
    };
    assert_eq!(state.pending[0].len(), 1, "x must bank the selection");
    assert!(
        state.anchor[0].is_none(),
        "and clear it, so the next v starts a fresh range"
    );
    assert!(
        app.status.as_deref().unwrap_or("").contains("Banked"),
        "got {:?}",
        app.status
    );
}

/// `c` is the other half of the same pair, and was missing alongside `x`.
#[test]
fn c_in_the_text_view_clears_both_sides_banks() {
    let source = "foo\nfoo\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    let mut state = TextPaintState::default();
    state.pending[0] = vec![HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    }];
    state.pending[1] = state.pending[0].clone();
    app.modal = Some(Modal::TextView { state });

    handle_modal_key(
        &mut app,
        KeyCode::Char('c'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("the text view should still be open, got {:?}", app.modal);
    };
    assert!(state.pending[0].is_empty() && state.pending[1].is_empty());
}

/// `diff -u` reports positions only in its hunk headers; the gutter turns counting into
/// reading. A deleted line has no after-side number and an inserted one has no before-side
/// number - the blank half is the point, not an omission.
#[test]
fn the_unix_diff_view_numbers_both_sides() {
    let backend = ratatui::backend::TestBackend::new(110, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 110, 16);
    let output = "--- a\n+++ b\n@@ -10,3 +20,3 @@\n context\n-gone\n+new\n";

    terminal
        .draw(|f| render_unix_diff_modal(f, area, output, 0))
        .unwrap();

    // Asserted per row on collapsed whitespace, not on exact column padding: the gutter's
    // field width is a layout choice, and pinning it would make this test fail for a cosmetic
    // change while saying nothing about the numbering it exists to check.
    let rows: Vec<String> = rendered_text(&terminal)
        .split('\u{2502}')
        .map(|row| row.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let has = |wanted: &str| rows.iter().any(|row| row.contains(wanted));

    // The context line carries both counters, starting at the hunk header's positions.
    assert!(has("10 20 context"), "got: {rows:#?}");
    // A deletion advances only the before side, an insertion only the after side - so each
    // shows one number and leaves the other half of the gutter blank.
    assert!(has("11 -gone"), "got: {rows:#?}");
    assert!(has("21 +new"), "got: {rows:#?}");
}

/// `:` opens the jump prompt, digits accumulate, Enter moves the cursor - the whole point
/// being a file too big to reach with `j`.
#[test]
fn colon_jumps_to_a_line_in_the_text_view() {
    let source = "a\nb\nc\nd\ne\nf\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::TextView {
        state: TextPaintState::default(),
    });

    let before = Code::from_string(source, &Language::Rust);
    let after = Code::from_string(source, &Language::Rust);
    for key in [KeyCode::Char(':'), KeyCode::Char('4'), KeyCode::Enter] {
        handle_modal_key(
            &mut app,
            key,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &before,
            &after,
        );
    }

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("the text view should still be open, got {:?}", app.modal);
    };
    assert_eq!(state.cursor[0], (3, 0), "line 4 is row 3");
    assert!(state.line_prompt.is_none(), "the prompt closes on Enter");
}

/// Every key in the painting views preserves the cached `FrameState`, because painting writes
/// `text_mappings` and the caches are built from `entries`. Falling through to `false` here
/// re-walked both ASTs on every cursor keystroke, which is what made the view unusable on a
/// large file.
#[test]
fn text_view_keys_do_not_invalidate_the_frame_state() {
    let text_view = Modal::TextView {
        state: TextPaintState::default(),
    };
    for key in [
        KeyCode::Char('j'),
        KeyCode::Char('v'),
        KeyCode::Char('m'),
        KeyCode::Char('x'),
        KeyCode::Esc,
    ] {
        assert!(
            is_state_preserving_key(Some(&text_view), key),
            "{key:?} should preserve the frame state"
        );
    }
}

/// The painting test is clamped at 100.0, not 0.0. Nothing in the corpus agrees exactly yet,
/// so a fresh 0.0 would fail on the first run and have to be loosened - which is the "clamp
/// moved for a reason that was not a measurement" habit the per-file layout exists to prevent.
#[test]
fn a_fresh_painting_test_passes_and_says_it_means_nothing_yet() {
    let block = painting_test_block("rust-add-if");

    assert!(
        block.contains(r#"assert_matches_human_painting_within_limit("rust-add-if", 100.0)"#),
        "got: {block}"
    );
    assert!(block.contains("Not measured yet"), "got: {block}");
    assert!(
        block.contains("fn painting()"),
        "the test name has to match the module's convention: {block}"
    );
    // Appended to a file that already has the licence header and the mapping test, so it
    // carries neither of its own - it starts at the blank line before its own `#[test]`.
    assert!(block.starts_with("\n#[test]"), "got: {block}");
}

/// A bare `App` for the text-painting action tests: they only touch `mapping`, `dirty` and
/// `status`, so the AST panels' node ids are irrelevant and a dummy pair keeps the setup to
/// one line.
fn test_app() -> App {
    App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        0,
        0,
        HumanMapping::default(),
    )
}

fn paint_state_at(
    side: usize,
    cursor: (usize, usize),
    anchor: Option<(usize, usize)>,
) -> TextPaintState {
    let mut state = TextPaintState {
        side,
        ..Default::default()
    };
    state.cursor[side] = cursor;
    state.anchor[side] = anchor;
    state
}

/// A selection includes the character *under* the cursor. Anything else surprises a reader
/// every time: they see a highlight covering `foo` and get a span covering `fo`.
#[test]
fn a_selection_includes_the_character_under_the_cursor() {
    let source = "let foo = 1;\n";
    let state = paint_state_at(0, (0, 6), Some((0, 4)));

    let spans = state.selection(0, source);
    assert_eq!(spans.len(), 1, "a same-row selection is a single span");
    let span = spans[0];

    assert_eq!(
        codediff::test::helper::human_mapping::span_text(source, span),
        Some("foo")
    );
}

/// Selecting backwards is the same selection - the anchor may be after the cursor.
#[test]
fn a_backwards_selection_normalizes_to_the_same_span() {
    let source = "let foo = 1;\n";
    let forward = paint_state_at(0, (0, 6), Some((0, 4)));
    let backward = paint_state_at(0, (0, 4), Some((0, 6)));

    // Both cover `foo`, but each includes the character under its own cursor, so the backward
    // one runs from 4 through the cursor at 4 and out to the anchor at 6 inclusive.
    assert_eq!(forward.selection(0, source), backward.selection(0, source));
}

/// Byte columns, not character columns - a multi-byte character before the selection must not
/// shift it. The same trap `span_text`'s own test pins from the other side.
#[test]
fn a_selection_past_a_multibyte_character_lands_on_the_right_text() {
    let source = "let é = foo;\n";
    // "let é = " is 9 bytes ('é' costs two), so `foo` starts at byte 9 and its last character
    // starts at byte 11.
    let state = paint_state_at(0, (0, 11), Some((0, 9)));

    let span = state.selection(0, source)[0];

    assert_eq!(
        codediff::test::helper::human_mapping::span_text(source, span),
        Some("foo")
    );
}

/// A selection spanning several rows is vertical: one span per row, all sharing the same
/// column range, rather than a single span sweeping full lines in between - the bug this
/// feature replaces would highlight (and let `d`/`i` swallow) every untouched character
/// between the two columns on the middle rows.
#[test]
fn a_multi_row_selection_is_a_stack_of_per_row_spans_not_a_line_sweep() {
    let source = "aaaXaaa\nbbbYbbb\ncccZccc\n";
    // Column 3 on row 0 through column 4 on row 2 - a one-column-wide block.
    let state = paint_state_at(0, (2, 4), Some((0, 3)));

    let spans = state.selection(0, source);

    assert_eq!(
        spans,
        vec![
            HumanTextSpan {
                start_row: 0,
                start_column: 3,
                end_row: 0,
                end_column: 5
            },
            HumanTextSpan {
                start_row: 1,
                start_column: 3,
                end_row: 1,
                end_column: 5
            },
            HumanTextSpan {
                start_row: 2,
                start_column: 3,
                end_row: 2,
                end_column: 5
            },
        ]
    );
}

/// A row too short to reach the selected columns contributes no span - there is nothing there
/// to select, rather than the selection falling back to covering whatever the row does have.
#[test]
fn a_multi_row_selection_skips_a_row_shorter_than_the_selected_columns() {
    let source = "aaaaaa\nbb\ncccccc\n";
    let state = paint_state_at(0, (2, 4), Some((0, 4)));

    let spans = state.selection(0, source);

    assert_eq!(
        spans.iter().map(|s| s.start_row).collect::<Vec<_>>(),
        vec![0, 2],
        "row 1 ('bb') is shorter than column 4, so it contributes nothing"
    );
}

/// `V` swaps a selection back to the pre-vertical behaviour: one span sweeping every full row
/// between anchor and cursor end to end, needed for a single contiguous multi-line block where
/// `m`'s identical-text-per-side check would otherwise fail on a per-row decomposition.
#[test]
fn toggling_off_vertical_restores_the_full_line_sweep() {
    let source = "aaaXaaa\nbbbYbbb\ncccZccc\n";
    let mut state = paint_state_at(0, (2, 4), Some((0, 3)));
    state.vertical = false;

    let spans = state.selection(0, source);

    assert_eq!(
        spans,
        vec![HumanTextSpan {
            start_row: 0,
            start_column: 3,
            end_row: 2,
            end_column: 5
        }],
        "one span end to end, not a per-row stack"
    );
}

/// Reads back the style painted at one character position of one rendered row, skipping the
/// gutter span every row starts with.
fn style_at(lines: &[Line<'static>], row: usize, column: usize) -> Style {
    let line = &lines[row];
    let mut consumed = 0usize;
    for span in line.spans.iter().skip(1) {
        let len = span.content.chars().count();
        if column < consumed + len {
            return span.style;
        }
        consumed += len;
    }
    panic!("column {column} past the end of row {row}'s content: {line:?}");
}

/// The user-visible reason this toggle exists: a vertical selection leaves the untouched tail
/// of a middle row unstyled, while the same anchor and cursor in full-line mode sweeps that
/// tail in too. Exercises the actual render path, not just the spans `selection` computes.
#[test]
fn vertical_selection_leaves_a_middle_rows_tail_unstyled_but_full_line_does_not() {
    let source = "aaaXaaaaaaaa\nbbbYbbbbbbbb\ncccZcccccccc\n";
    let mut state = paint_state_at(0, (2, 4), Some((0, 3)));
    let selected_bg = Some(OverlayTheme::default().palette().cross_highlight_bg);
    // Row 1, well past the selection's column 3-4 range but still inside the line - the tail
    // a full-line sweep would highlight and a vertical selection would not.
    let tail_column = 10;

    let vertical = render_paint_side(source, &[], &state, 0, 5, 10_000);
    assert_ne!(
        style_at(&vertical, 1, tail_column).bg,
        selected_bg,
        "vertical: row 1's tail past the selected columns must stay unstyled"
    );

    state.vertical = false;
    let full_line = render_paint_side(source, &[], &state, 0, 5, 10_000);
    assert_eq!(
        style_at(&full_line, 1, tail_column).bg,
        selected_bg,
        "full-line: row 1's tail is swept into the selection between anchor and cursor"
    );
}

/// `h`/`l` step by characters, so the cursor can never land inside one - a column that did
/// would produce a span `span_text` correctly refuses to read back.
#[test]
fn stepping_across_a_multibyte_character_lands_on_boundaries() {
    let source = "aéb\n";
    let mut state = paint_state_at(0, (0, 0), None);

    state.step_column(true, source);
    assert_eq!(state.cursor[0], (0, 1), "onto the two-byte character");
    state.step_column(true, source);
    assert_eq!(state.cursor[0], (0, 3), "past it in one step, not into it");
    state.step_column(false, source);
    assert_eq!(state.cursor[0], (0, 1), "and back the same way");
}

#[test]
fn m_pairs_both_sides_selections_and_derives_move_from_identical_text() {
    let (before_src, after_src) = ("alpha\nbeta\n", "beta\nalpha\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 4);
    state.anchor[1] = Some((1, 0));
    state.cursor[1] = (1, 4);

    action_paint_match(&mut app, &mut state, before_src, after_src);

    let entries = &solution_entries(&app.mapping, &app.text_solution);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation, HumanTextOperation::Match);
    assert_eq!(
        entries[0].verdict(before_src, after_src).unwrap(),
        HumanTextVerdict::Move,
        "identical text on both sides is a relocation"
    );
    assert!(app.dirty);
    assert_eq!(state.anchor, [None, None], "both selections are consumed");
}

#[test]
fn m_without_a_selection_on_both_sides_paints_nothing_and_says_so() {
    let (before_src, after_src) = ("alpha\n", "alpha\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 4);

    action_paint_match(&mut app, &mut state, before_src, after_src);

    assert!(app.mapping.text_mappings.is_empty(), "nothing was painted");
    assert!(!app.dirty);
    assert!(
        app.status.as_deref().unwrap_or("").contains("both sides"),
        "got {:?}",
        app.status
    );
}

#[test]
fn d_and_i_paint_one_sided_ranges_on_their_own_side() {
    let (before_src, after_src) = ("gone\nkept\n", "kept\nnew\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();

    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 3);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    state.side = 1;
    state.anchor[1] = Some((1, 0));
    state.cursor[1] = (1, 2);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Insert,
        before_src,
        after_src,
    );

    let entries = &solution_entries(&app.mapping, &app.text_solution);
    assert_eq!(entries.len(), 2);
    assert_eq!(
        codediff::test::helper::human_mapping::span_text(before_src, entries[0].before[0]),
        Some("gone")
    );
    assert!(entries[0].after.is_empty(), "a delete has no after side");
    assert_eq!(
        codediff::test::helper::human_mapping::span_text(after_src, entries[1].after[0]),
        Some("new")
    );
    assert!(entries[1].before.is_empty(), "an insert has no before side");
}

/// `u` removes the whole entry, both halves of a `Match` included: a half-removed match is a
/// malformed entry that `verdict` refuses to read, so removing one side is not a smaller edit
/// but a broken file.
#[test]
fn u_removes_a_whole_match_from_either_side() {
    let (before_src, after_src) = ("alpha\nbeta\n", "beta\nalpha\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 4);
    state.anchor[1] = Some((1, 0));
    state.cursor[1] = (1, 4);
    action_paint_match(&mut app, &mut state, before_src, after_src);

    // Stand on the *after* half and unmark; the before half must go too.
    state.side = 1;
    state.cursor[1] = (1, 2);
    action_paint_unmark(&mut app, &state, before_src, after_src);

    assert!(
        solution_entries(&app.mapping, &app.text_solution).is_empty(),
        "both halves of the match should be gone"
    );
}

/// The `None`/`Some(empty)` distinction the field's `Option` exists for, reachable only
/// deliberately.
#[test]
fn z_marks_an_unpainted_fixture_as_deliberately_empty() {
    let mut app = test_app();
    assert!(app.mapping.text_mappings.is_empty());

    action_paint_mark_empty(&mut app);

    assert_eq!(
        app.mapping.text_mappings.len(),
        1,
        "a named painting exists"
    );
    assert!(app.mapping.text_mappings[0].mapping.entries.is_empty());
    assert!(app.dirty);
}

#[test]
fn z_refuses_to_touch_a_fixture_that_already_has_painted_ranges() {
    let (before_src, after_src) = ("gone\n", "\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();
    state.anchor[0] = Some((0, 0));
    state.cursor[0] = (0, 3);
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    action_paint_mark_empty(&mut app);

    assert_eq!(
        solution_entries(&app.mapping, &app.text_solution).len(),
        1,
        "Z must not clear an existing painting"
    );
    assert!(
        app.status.as_deref().unwrap_or("").contains("already has"),
        "got {:?}",
        app.status
    );
}

#[test]
fn the_text_view_renders_painted_ranges() {
    let backend = ratatui::backend::TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 100, 24);
    let mapping = HumanMapping {
        entries: vec![],
        groups: vec![],
        text_mappings: vec![NamedTextMapping {
            name: "Minimal".to_string(),
            mapping: HumanTextMapping {
                entries: vec![HumanTextEntry {
                    operation: HumanTextOperation::Delete,
                    before: vec![HumanTextSpan {
                        start_row: 0,
                        start_column: 3,
                        end_row: 0,
                        end_column: 11,
                    }],
                    after: vec![],
                }],
            },
        }],
    };

    terminal
        .draw(|f| {
            render_text_view_modal(
                f,
                area,
                "fn old_name() {}",
                "fn new_name() {}",
                &mapping,
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                &TextPaintState::default(),
            );
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(text.contains("old_name"), "before content missing: {text}");
    assert!(text.contains("new_name"), "after content missing: {text}");
    assert!(
        text.contains("1 painted"),
        "the painted count should be in the title: {text}"
    );
}

fn rendered_text(terminal: &Terminal<ratatui::backend::TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// The picker's header row is where its whole interaction state lives: the `Unmarked` count
/// column exists, the sorted column carries a direction arrow, and a filtered column is
/// marked - with the filters spelled out in the title so a compound one is readable.
#[test]
fn render_open_diff_picker_shows_the_unmarked_column_and_the_sort_and_filter_markers() {
    // Wide enough that the block title isn't truncated: this asserts on the title's contents,
    // so a narrower backend would be testing ratatui's truncation rather than the picker.
    let backend = ratatui::backend::TestBackend::new(160, 14);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 160, 14);
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
    ];
    let unmarked =
        std::collections::HashMap::from([("alpha".to_string(), 42), ("bravo".to_string(), 0)]);
    let mut view = sort_view(DiffColumn::Unmarked);
    view.column = DiffColumn::Unmarked;
    view.filters.paint = FlagFilter::No;
    let modal = Modal::OpenDiffPicker {
        options,
        selected: 0,
        view,
        name_input: None,
    };

    terminal
        .draw(|f| {
            render_modal(
                f,
                area,
                &modal,
                "alpha",
                None,
                "",
                "",
                &HumanMapping::default(),
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                DiffPickerData {
                    unmarked: Some(&unmarked),
                    ..DiffPickerData::default()
                },
                None,
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(text.contains("Unmarked^"), "sorted column header: {text}");
    assert!(text.contains("Paint*"), "filtered column marker: {text}");
    assert!(text.contains("42"), "the unmarked count itself: {text}");
    assert!(
        text.contains("unpainted only"),
        "the active filter spelled out in the title: {text}"
    );
    assert!(text.contains("s sort, f filter"), "the key legend: {text}");
}

/// The title is longer than the popup at ordinary terminal widths, so ratatui truncates it -
/// and what has to survive that truncation is the filter list, the part that actually changes
/// as the reader works. Guards the abbreviation in `render_open_diff_picker`'s title: at 110
/// columns the key legend is expected to be cut, the filters are not.
#[test]
fn the_diff_picker_title_keeps_its_filter_list_when_the_terminal_truncates_it() {
    let backend = ratatui::backend::TestBackend::new(110, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 110, 12);
    let mut view = DiffPickerView::default();
    view.filters.paint = FlagFilter::No;
    view.filters.disagree = FlagFilter::Yes;
    let modal = Modal::OpenDiffPicker {
        options: vec![("alpha".to_string(), "handmade")],
        selected: 0,
        view,
        name_input: None,
    };

    terminal
        .draw(|f| {
            render_modal(
                f,
                area,
                &modal,
                "alpha",
                None,
                "",
                "",
                &HumanMapping::default(),
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                DiffPickerData::default(),
                None,
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("unpainted only AND disagreements only"),
        "both active filters must survive truncation at 110 columns: {text}"
    );
}

#[test]
fn text_view_modal_renders_both_sides_content() {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 80, 24);

    terminal
        .draw(|f| {
            render_text_view_modal(
                f,
                area,
                "fn old_name() {}",
                "fn new_name() {}",
                &HumanMapping::default(),
                "Minimal",
                TextOverlay::Human,
                None,
                None,
                &TextPaintState::default(),
            );
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("old_name"),
        "before content missing from render: {text}"
    );
    assert!(
        text.contains("new_name"),
        "after content missing from render: {text}"
    );
}

#[test]
fn unix_diff_modal_renders_diff_output() {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 80, 24);

    let output = run_unix_diff(
        b"fn main() {\n    old();\n}\n",
        b"fn main() {\n    new();\n}\n",
    )
    .unwrap();
    terminal
        .draw(|f| {
            render_unix_diff_modal(f, area, &output, 0);
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("old();"),
        "removed line missing from render: {text}"
    );
    assert!(
        text.contains("new();"),
        "added line missing from render: {text}"
    );
}

#[test]
fn centered_rect_at_least_uses_the_percentage_when_it_already_meets_the_minimum() {
    let area = Rect::new(0, 0, 100, 100);
    let rect = centered_rect_at_least(60, 30, 10, 10, area);
    assert_eq!(rect, centered_rect(60, 30, area));
}

#[test]
fn centered_rect_at_least_grows_past_the_percentage_to_meet_the_minimum() {
    // A 100x20 terminal's 30%-height popup would be 6 rows, short of a 10-row minimum.
    let small_area = Rect::new(0, 0, 100, 20);
    let rect = centered_rect_at_least(60, 30, 10, 10, small_area);
    assert_eq!(
        rect.height, 10,
        "should grow to the minimum, not stay at 30% (6 rows)"
    );
    assert!(rect.height <= small_area.height);
}

#[test]
fn centered_rect_at_least_never_exceeds_the_available_area() {
    // A terminal too small even for the minimum (a phone-sized SSH client is the motivating
    // case) must still produce a rect that fits, not one that's clamped to a minimum bigger
    // than the terminal itself.
    let tiny_area = Rect::new(0, 0, 20, 8);
    let rect = centered_rect_at_least(60, 30, 50, 20, tiny_area);
    assert!(rect.width <= tiny_area.width);
    assert!(rect.height <= tiny_area.height);
}

/// Regression guard for the real bug: on a small terminal (a phone-sized SSH client is the
/// motivating case), `render_text_modal`'s old fixed `centered_rect(60, 30, area)` could come
/// out short enough that the `> {input}` line - the actual input box, well past the first
/// couple of lines of instructions - was clipped out of the visible area entirely, with no
/// scroll indicator to hint why (a plain `Paragraph` has none). This is exactly the
/// `PromptPromoteName` modal's own body shape (instructions, then a blank line, then `> `).
#[test]
fn render_text_modal_shows_every_line_including_the_input_box_on_a_small_terminal() {
    let backend = ratatui::backend::TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 40, 12);

    let body = "Enter a name for src/test/data/diffs/small/<name>/\n(letters, digits, - and _; must not already exist)\n\n> rust-rustdesk-\n\n[Enter] confirm   [Esc] cancel";
    terminal
        .draw(|f| render_text_modal(f, area, "Promote sample to test case", body))
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("rust-rustdesk-"),
        "the input line must still be visible on a small terminal, not scrolled out of view: {text}"
    );
}

/// A `SampleRow` without spelling out five fields at each of the call sites below.
fn sample_row(
    name: &str,
    language: &str,
    bucket: Option<&str>,
    status: SampleTriageStatus,
    size: usize,
) -> SampleRow {
    SampleRow {
        name: name.to_string(),
        language: language.to_string(),
        bucket: bucket.map(str::to_string),
        status,
        size,
    }
}

fn sample_rows() -> Vec<SampleRow> {
    vec![
        sample_row(
            "charlie",
            "Go",
            Some("30-100"),
            SampleTriageStatus::Sampled,
            5,
        ),
        sample_row(
            "alpha",
            "Rust",
            Some("1000-3000"),
            SampleTriageStatus::Promoted,
            20,
        ),
        sample_row("bravo", "Go", None, SampleTriageStatus::Rejected, 0),
    ]
}

#[test]
fn open_sample_picker_renders_every_column_including_the_bucket() {
    let rows = sample_rows();
    let backend = ratatui::backend::TestBackend::new(200, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 200, 24);

    terminal
        .draw(|f| render_open_sample_picker(f, area, &rows, 0, &SamplePickerView::default(), None))
        .unwrap();

    let text = rendered_text(&terminal);
    for expected in [
        "Name",
        "Lang",
        "Bucket",
        "Status",
        "Size", // headers
        "30-100",
        "1000-3000", // the strata each sample was drawn for
        "Go",
        "Rust",
        "SOLVED",
        "REJECTED",
        "sampled",
    ] {
        assert!(text.contains(expected), "{expected:?} missing from: {text}");
    }
    assert!(
        text.contains('?'),
        "an unbucketed sample should render its stratum as ?: {text}"
    );
}

#[test]
fn visible_sample_rows_sorts_by_the_selected_column() {
    let rows = sample_rows();
    let names = |column: SampleColumn, descending: bool| -> Vec<String> {
        let view = SamplePickerView {
            column,
            sort: SampleSort { column, descending },
            filters: SampleFilters::default(),
        };
        visible_sample_rows(&rows, &view)
            .into_iter()
            .map(|row| row.name)
            .collect()
    };

    assert_eq!(
        names(SampleColumn::Name, false),
        vec!["alpha", "bravo", "charlie"]
    );
    assert_eq!(
        names(SampleColumn::Name, true),
        vec!["charlie", "bravo", "alpha"]
    );
    // Go before Rust, and the two Go rows tie-broken by name rather than left to chance.
    assert_eq!(
        names(SampleColumn::Lang, false),
        vec!["bravo", "charlie", "alpha"]
    );
    assert_eq!(
        names(SampleColumn::Size, false),
        vec!["bravo", "charlie", "alpha"]
    );
    assert_eq!(
        names(SampleColumn::Status, false),
        vec!["charlie", "alpha", "bravo"],
        "untriaged rows first - they are what the picker exists to surface"
    );
    // 30-100 before 1000-3000, and the unbucketed row last. Sorting these as strings would put
    // "1000-3000" first, which is the whole reason `bucket_order` parses the lower bound.
    assert_eq!(
        names(SampleColumn::Bucket, false),
        vec!["charlie", "alpha", "bravo"]
    );
}

#[test]
fn bucket_order_ranks_by_the_lower_bound_not_the_label() {
    assert!(bucket_order(Some("30-100")) < bucket_order(Some("100-300")));
    assert!(bucket_order(Some("1000-3000")) < bucket_order(Some("3000+")));
    assert!(bucket_order(Some("0-10")) < bucket_order(Some("10-30")));
    assert_eq!(
        bucket_order(None),
        usize::MAX,
        "an unrecorded stratum sorts last, not first"
    );
}

#[test]
fn the_bucket_filter_never_hides_a_sample_with_no_recorded_stratum() {
    let rows = sample_rows();
    let view = SamplePickerView {
        filters: SampleFilters {
            bucket: Some("30-100".to_string()),
            ..SampleFilters::default()
        },
        ..SamplePickerView::default()
    };
    let names: Vec<String> = visible_sample_rows(&rows, &view)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(
        names,
        vec!["bravo", "charlie"],
        "charlie matches the bucket and bravo has none - 'not recorded' is not evidence to drop it"
    );
}

#[test]
fn sample_filters_narrow_together_rather_than_either_or() {
    let rows = sample_rows();
    let view = SamplePickerView {
        filters: SampleFilters {
            language: Some("Go".to_string()),
            size: FlagFilter::Yes,
            ..SampleFilters::default()
        },
        ..SamplePickerView::default()
    };
    let names: Vec<String> = visible_sample_rows(&rows, &view)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(
        names,
        vec!["charlie"],
        "bravo is Go but has an empty diff, alpha has a diff but is Rust"
    );
}

#[test]
fn the_size_filter_finds_samples_whose_diff_is_empty() {
    let rows = sample_rows();
    let view = SamplePickerView {
        filters: SampleFilters {
            size: FlagFilter::No,
            ..SampleFilters::default()
        },
        ..SamplePickerView::default()
    };
    let names: Vec<String> = visible_sample_rows(&rows, &view)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(names, vec!["bravo"], "a 0-line diff is a broken draw");
}

#[test]
fn the_name_filter_is_a_case_insensitive_substring() {
    let rows = sample_rows();
    let view = SamplePickerView {
        filters: SampleFilters {
            name: Some("rav".to_string()),
            ..SampleFilters::default()
        },
        ..SamplePickerView::default()
    };
    let names: Vec<String> = visible_sample_rows(&rows, &view)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(names, vec!["bravo"]);
}

#[test]
fn value_filters_cycle_through_the_values_present_and_back_to_all() {
    let rows = sample_rows();
    let languages = sample_language_values(&rows);
    assert_eq!(languages, vec!["Go", "Rust"]);
    assert_eq!(next_value_filter(None, &languages).as_deref(), Some("Go"));
    assert_eq!(
        next_value_filter(Some("Go"), &languages).as_deref(),
        Some("Rust")
    );
    assert_eq!(
        next_value_filter(Some("Rust"), &languages),
        None,
        "past the last value the filter turns off rather than sticking"
    );

    // Offered smallest-stratum-first, and only for strata some row actually has.
    let buckets = sample_bucket_values(&rows);
    assert_eq!(buckets, vec!["30-100", "1000-3000"]);
}

#[test]
fn the_status_filter_cycles_through_all_three_states_and_back() {
    assert_eq!(next_status_filter(None), Some(SampleTriageStatus::Sampled));
    assert_eq!(
        next_status_filter(Some(SampleTriageStatus::Sampled)),
        Some(SampleTriageStatus::Promoted)
    );
    assert_eq!(
        next_status_filter(Some(SampleTriageStatus::Promoted)),
        Some(SampleTriageStatus::Rejected)
    );
    assert_eq!(next_status_filter(Some(SampleTriageStatus::Rejected)), None);
}

#[test]
fn the_sample_picker_title_names_the_sorted_column_and_the_filters() {
    let rows = sample_rows();
    let backend = ratatui::backend::TestBackend::new(200, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 200, 24);

    let view = SamplePickerView {
        column: SampleColumn::Bucket,
        sort: SampleSort {
            column: SampleColumn::Size,
            descending: true,
        },
        filters: SampleFilters {
            language: Some("Go".to_string()),
            ..SampleFilters::default()
        },
    };
    terminal
        .draw(|f| render_open_sample_picker(f, area, &rows, 0, &view, None))
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(text.contains("sort:Sizev"), "sorted column missing: {text}");
    assert!(text.contains("lang=Go"), "active filter missing: {text}");
}

#[test]
fn open_sample_picker_enter_opens_the_visible_entry_not_the_raw_index() {
    // Regression guard for `visible.get(selected)` rather than `rows[selected]`: with the Status
    // filter narrowing to untriaged rows, `selected` indexes the *filtered* list, so a raw index
    // would open the wrong sample.
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    app.modal = Some(Modal::OpenSamplePicker {
        rows: vec![
            sample_row(
                "rejected-one",
                "Rust",
                None,
                SampleTriageStatus::Rejected,
                0,
            ),
            sample_row("solved-one", "Rust", None, SampleTriageStatus::Promoted, 0),
            sample_row("unsolved-one", "Rust", None, SampleTriageStatus::Sampled, 0),
            sample_row("unsolved-two", "Rust", None, SampleTriageStatus::Sampled, 0),
        ],
        selected: 1,
        view: SamplePickerView {
            filters: SampleFilters {
                status: Some(SampleTriageStatus::Sampled),
                ..SampleFilters::default()
            },
            ..SamplePickerView::default()
        },
        name_input: None,
    });
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    let target = handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    match target {
        Some(OpenTarget::Sample(name)) => assert_eq!(name, "unsolved-two"),
        other => panic!("expected OpenTarget::Sample(\"unsolved-two\"), got {other:?}"),
    }
}
#[test]
fn open_sample_picker_s_sorts_by_the_cursor_column_and_keeps_the_selected_row() {
    // Unlike the old fixed four-way cycle, `s` takes the sort over to whichever column `h`/`l`
    // last moved to, and the selection follows the row it was on - which is the point of
    // re-sorting while looking at a particular sample.
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    app.modal = Some(Modal::OpenSamplePicker {
        rows: vec![
            sample_row("alpha", "Rust", None, SampleTriageStatus::Sampled, 5),
            sample_row("bravo", "Rust", None, SampleTriageStatus::Sampled, 1),
            sample_row("charlie", "Rust", None, SampleTriageStatus::Sampled, 20),
        ],
        // Sorted by Name, so index 2 is "charlie"; by Size it becomes index 1.
        selected: 2,
        view: SamplePickerView {
            column: SampleColumn::Size,
            ..SamplePickerView::default()
        },
        name_input: None,
    });
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    let target = handle_modal_key(
        &mut app,
        KeyCode::Char('s'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(target.is_none(), "s should not switch cases directly");
    match &app.modal {
        Some(Modal::OpenSamplePicker { selected, view, .. }) => {
            assert_eq!(view.sort.column, SampleColumn::Size);
            assert!(
                !view.sort.descending,
                "a fresh column sorts ascending first"
            );
            assert_eq!(
                *selected, 2,
                "charlie has the largest diff, so it is last under an ascending Size sort"
            );
        }
        other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
    }
    assert_eq!(
        app.sample_view.sort.column,
        SampleColumn::Size,
        "the new sort must persist on App too, not just this modal instance, so the next O \
         reopens with it"
    );
}
#[test]
fn open_sample_picker_f_persists_the_column_filter_on_app() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    app.modal = Some(Modal::OpenSamplePicker {
        rows: vec![sample_row(
            "alpha",
            "Rust",
            Some("30-100"),
            SampleTriageStatus::Sampled,
            5,
        )],
        selected: 0,
        view: SamplePickerView {
            column: SampleColumn::Status,
            ..SamplePickerView::default()
        },
        name_input: None,
    });
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    handle_modal_key(
        &mut app,
        KeyCode::Char('f'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert_eq!(
        app.sample_view.filters.status,
        Some(SampleTriageStatus::Sampled),
        "f's new filter must persist on App too, so the next O reopens with it"
    );
}
#[test]
fn open_commit_picker_j_k_move_selection_clamped_to_bounds() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitPicker {
        commits: vec![
            ("aaa".to_string(), "first commit".to_string()),
            ("bbb".to_string(), "second commit".to_string()),
        ],
        selected: 0,
    });

    // Up at the top must stay clamped at 0, not underflow.
    handle_modal_key(
        &mut app,
        KeyCode::Char('k'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    match &app.modal {
        Some(Modal::OpenCommitPicker { selected, .. }) => assert_eq!(*selected, 0),
        other => panic!("expected Modal::OpenCommitPicker, got {other:?}"),
    }

    // Two Downs must clamp at the last index (1), not run past it.
    for _ in 0..2 {
        handle_modal_key(
            &mut app,
            KeyCode::Char('j'),
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
    }
    match &app.modal {
        Some(Modal::OpenCommitPicker { selected, .. }) => assert_eq!(*selected, 1),
        other => panic!("expected Modal::OpenCommitPicker, got {other:?}"),
    }
}

#[test]
fn open_commit_picker_esc_cancels() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitPicker {
        commits: vec![("aaa".to_string(), "first commit".to_string())],
        selected: 0,
    });

    let target = handle_modal_key(
        &mut app,
        KeyCode::Esc,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(target.is_none());
    assert_eq!(app.status.as_deref(), Some("Cancelled"));
}

#[test]
fn open_commit_picker_enter_on_an_unresolvable_commit_reports_an_error_without_crashing() {
    // Doesn't depend on this repository's actual git history (which the CI checkout may only
    // have a shallow slice of - see `list_commit_files`'s doc comment): any hash git can't
    // resolve at all takes the same `git diff-tree` failure path, regardless of clone depth.
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitPicker {
        commits: vec![("not-a-real-commit-hash".to_string(), "bogus".to_string())],
        selected: 0,
    });

    let target = handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(
        target.is_none(),
        "an unresolvable commit must not switch cases"
    );
    match &app.modal {
        Some(Modal::OpenCommitPicker { .. }) => {}
        other => {
            panic!("expected to stay on Modal::OpenCommitPicker after the error, got {other:?}")
        }
    }
    assert!(
        app.status.is_some(),
        "the failure should be reported on the status line, not silently dropped"
    );
}

#[test]
fn open_commit_file_picker_enter_opens_the_selected_file_as_an_open_target() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitFilePicker {
        hash: "abc123".to_string(),
        summary: "did a thing".to_string(),
        files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
        selected: 1,
    });

    let target = handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    match target {
        Some(OpenTarget::GitCommitFile {
            hash,
            summary,
            path,
        }) => {
            assert_eq!(hash, "abc123");
            assert_eq!(summary, "did a thing");
            assert_eq!(path, "src/b.rs");
        }
        other => panic!("expected OpenTarget::GitCommitFile, got {other:?}"),
    }
}

#[test]
fn open_commit_file_picker_enter_from_a_dirty_git_commit_file_case_cannot_save_directly() {
    // Mirrors `CaseOrigin::Sample`'s existing `can_save = false`: a git-commit-sourced case
    // isn't a real diffs/ case yet either, so it needs `s`'s promote-name prompt (from the
    // main view), not a single-key save, before it can be switched away from.
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::GitCommitFile {
            path: "src/current.rs".to_string(),
        },
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    app.dirty = true;
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitFilePicker {
        hash: "abc123".to_string(),
        summary: "did a thing".to_string(),
        files: vec!["src/a.rs".to_string()],
        selected: 0,
    });

    let target = handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(
        target.is_none(),
        "must not switch immediately; ConfirmDiscardUnsaved decides first"
    );
    match app.modal {
        Some(Modal::ConfirmDiscardUnsaved { target, can_save }) => {
            assert!(
                !can_save,
                "a git-commit-sourced current case cannot be saved with a single key"
            );
            match target {
                OpenTarget::GitCommitFile { path, .. } => assert_eq!(path, "src/a.rs"),
                other => panic!("expected OpenTarget::GitCommitFile, got {other:?}"),
            }
        }
        other => panic!("expected Modal::ConfirmDiscardUnsaved, got {other:?}"),
    }
}

#[test]
fn open_commit_file_picker_esc_cancels() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenCommitFilePicker {
        hash: "abc123".to_string(),
        summary: "did a thing".to_string(),
        files: vec!["src/a.rs".to_string()],
        selected: 0,
    });

    let target = handle_modal_key(
        &mut app,
        KeyCode::Esc,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert!(target.is_none());
    assert_eq!(app.status.as_deref(), Some("Cancelled"));
}

#[test]
fn open_sample_picker_modal_selects_the_currently_open_case_under_the_given_view() {
    let rows = vec![
        sample_row("alpha", "Rust", None, SampleTriageStatus::Sampled, 5),
        sample_row("bravo", "Rust", None, SampleTriageStatus::Sampled, 1),
        sample_row("charlie", "Rust", None, SampleTriageStatus::Sampled, 20),
    ];

    // "bravo" is index 1 in `rows`' own order, but index 0 once sorted by Size ascending -
    // proves `selected` is computed against the sorted/filtered view, not raw `rows`.
    let view = SamplePickerView {
        sort: SampleSort {
            column: SampleColumn::Size,
            descending: false,
        },
        ..SamplePickerView::default()
    };
    let modal = open_sample_picker_modal(rows, "bravo", view.clone());

    match modal {
        Modal::OpenSamplePicker {
            selected,
            view: got,
            ..
        } => {
            assert_eq!(selected, 0);
            assert_eq!(got, view);
        }
        other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
    }
}

#[test]
fn open_sample_picker_modal_falls_back_to_the_first_entry_when_the_current_case_is_not_a_sample() {
    let rows = vec![sample_row(
        "alpha",
        "Rust",
        None,
        SampleTriageStatus::Sampled,
        5,
    )];
    let modal = open_sample_picker_modal(rows, "not-a-sample-name", SamplePickerView::default());
    match modal {
        Modal::OpenSamplePicker { selected, .. } => assert_eq!(selected, 0),
        other => panic!("expected Modal::OpenSamplePicker, got {other:?}"),
    }
}

#[test]
fn visible_diff_options_narrows_to_the_given_dataset() {
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "small"),
        ("charlie".to_string(), "handmade"),
        ("delta".to_string(), "full"),
    ];
    assert_eq!(
        visible_diff_options(&options, &dataset_view(None), DiffPickerData::default()),
        vec!["alpha", "bravo", "charlie", "delta"],
        "no filter should show every dataset"
    );
    assert_eq!(
        visible_diff_options(
            &options,
            &dataset_view(Some("handmade")),
            DiffPickerData::default()
        ),
        vec!["alpha", "charlie"]
    );
    assert_eq!(
        visible_diff_options(
            &options,
            &dataset_view(Some("full")),
            DiffPickerData::default()
        ),
        vec!["delta"]
    );
}

#[test]
fn visible_diff_options_cmpl_filter_excludes_only_cases_the_map_measured() {
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
        ("charlie".to_string(), "handmade"),
    ];
    let mut unmarked = std::collections::HashMap::new();
    unmarked.insert("alpha".to_string(), 4); // incomplete
    unmarked.insert("bravo".to_string(), 0); // complete
    // "charlie" deliberately absent - not yet scanned, or failed to load.
    let data = DiffPickerData {
        unmarked: Some(&unmarked),
        ..DiffPickerData::default()
    };

    assert_eq!(
        visible_diff_options(
            &options,
            &flag_view(DiffColumn::Cmpl, FlagFilter::Yes),
            data
        ),
        vec!["alpha", "charlie"],
        "'incomplete only' hides complete, and keeps incomplete and unscanned alike"
    );
    assert_eq!(
        visible_diff_options(&options, &flag_view(DiffColumn::Cmpl, FlagFilter::No), data),
        vec!["bravo", "charlie"],
        "'complete only' hides incomplete, and still keeps the unscanned one"
    );
    assert_eq!(
        visible_diff_options(&options, &DiffPickerView::default(), data),
        vec!["alpha", "bravo", "charlie"],
        "the filter off should show everything regardless of the map"
    );
}

/// Setting both `Cmpl` and `Unmarked` the same way narrows nothing further, so the title bar
/// must not read as two constraints - see `DiffFilters::labels`.
#[test]
fn the_title_lists_a_matching_cmpl_and_unmarked_filter_once() {
    let mut filters = DiffFilters {
        cmpl: FlagFilter::Yes,
        unmarked: FlagFilter::Yes,
        ..DiffFilters::default()
    };
    assert_eq!(filters.labels(), vec!["incomplete only"]);

    // Opposite directions really are two constraints (an unsatisfiable pair, but the reader
    // should be able to see that), so both are listed.
    filters.unmarked = FlagFilter::No;
    assert_eq!(filters.labels(), vec!["incomplete only", "none unmarked"]);
}

/// `Cmpl` and `Unmarked` read one map (`App::diff_unmarked`), so their filters must select
/// exactly the same rows - the two columns differ in how they *sort*, not in what they hide.
#[test]
fn the_cmpl_and_unmarked_filters_select_the_same_rows() {
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
        ("charlie".to_string(), "handmade"),
    ];
    let unmarked =
        std::collections::HashMap::from([("alpha".to_string(), 4), ("bravo".to_string(), 0)]);
    let data = DiffPickerData {
        unmarked: Some(&unmarked),
        ..DiffPickerData::default()
    };

    for filter in [FlagFilter::Yes, FlagFilter::No] {
        assert_eq!(
            visible_diff_options(&options, &flag_view(DiffColumn::Cmpl, filter), data),
            visible_diff_options(&options, &flag_view(DiffColumn::Unmarked, filter), data),
            "{filter:?} must mean the same thing on both columns"
        );
    }
}

#[test]
fn next_dataset_filter_cycles_through_diff_datasets_and_back_to_all() {
    // Walks every entry rather than hardcoding `DIFF_DATASETS`' length, so this doesn't need
    // editing again the next time a dataset is added (it already needed exactly that edit
    // once, when `stratified` became the fourth).
    let mut current = None;
    for &dataset in DIFF_DATASETS {
        current = next_dataset_filter(current);
        assert_eq!(current, Some(dataset));
    }
    assert_eq!(next_dataset_filter(current), None);
}

#[test]
fn open_diff_picker_modal_selects_the_currently_open_case_under_the_given_filter() {
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "small"),
        ("charlie".to_string(), "handmade"),
    ];
    let view = dataset_view(Some("handmade"));

    // "charlie" is index 2 in `options`' own order, but index 1 once filtered to just
    // "handmade" - proves `selected` is computed against the filtered view, not raw options.
    let modal = open_diff_picker_modal(options, "charlie", view, DiffPickerData::default());

    match modal {
        Modal::OpenDiffPicker { selected, view, .. } => {
            assert_eq!(selected, 1);
            assert_eq!(view.filters.dataset, Some("handmade"));
        }
        other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
    }
}

#[test]
fn open_diff_picker_modal_falls_back_to_the_first_entry_when_the_current_case_is_filtered_out() {
    let options = vec![
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "small"),
    ];
    let view = dataset_view(Some("small"));
    // "alpha" is the currently open case, but it's a "handmade" fixture and the filter above
    // is "small" - alpha isn't in the filtered view at all, so this must fall back to the
    // first visible entry instead of panicking or landing out of bounds.
    let modal = open_diff_picker_modal(options, "alpha", view, DiffPickerData::default());
    match modal {
        Modal::OpenDiffPicker { selected, .. } => assert_eq!(selected, 0),
        other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
    }
}

/// Opens the `o` picker over `options` with `view` in force and feeds it `keys` in order,
/// handing back the App - the shared body of every picker key test below, so each says only
/// what it is actually about.
fn press_in_diff_picker(
    options: Vec<(String, &'static str)>,
    view: DiffPickerView,
    keys: &[KeyCode],
) -> App {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    app.modal = Some(Modal::OpenDiffPicker {
        options,
        selected: 0,
        view,
        name_input: None,
    });

    for key in keys {
        handle_modal_key(
            &mut app,
            *key,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );
    }
    app
}

fn picker_view(app: &App) -> DiffPickerView {
    match &app.modal {
        Some(Modal::OpenDiffPicker { view, .. }) => view.clone(),
        other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
    }
}

/// `h`/`l` walk the cursor across the header and clamp at both ends rather than wrapping.
#[test]
fn open_diff_picker_h_and_l_move_the_column_cursor_and_clamp_at_the_ends() {
    let options = vec![("alpha".to_string(), "handmade")];

    let app = press_in_diff_picker(
        options.clone(),
        DiffPickerView::default(),
        &[KeyCode::Char('l'), KeyCode::Char('l')],
    );
    assert_eq!(picker_view(&app).column, DiffColumn::Cmpl);
    assert_eq!(
        app.diff_view.column,
        DiffColumn::Cmpl,
        "the cursor column persists on App too, so the next o reopens on it"
    );

    // Six presses from the far left overshoots the six-column table by one.
    let app = press_in_diff_picker(
        options.clone(),
        DiffPickerView::default(),
        &[
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
        ],
    );
    assert_eq!(
        picker_view(&app).column,
        DiffColumn::Disagree,
        "l must clamp at the last column, not wrap round to Name"
    );

    let app = press_in_diff_picker(options, DiffPickerView::default(), &[KeyCode::Char('h')]);
    assert_eq!(
        picker_view(&app).column,
        DiffColumn::Name,
        "h must clamp at the first column"
    );
}

/// `f` on `Dataset` is the old `d` key: it cycles the dataset filter, and - like every other
/// filter and sort change here - persists the result on `App` so the next `o` reopens with it.
#[test]
fn open_diff_picker_f_on_the_dataset_column_persists_the_filter_on_app() {
    let app = press_in_diff_picker(
        vec![("alpha".to_string(), "handmade")],
        column_view(DiffColumn::Dataset),
        &[KeyCode::Char('f')],
    );

    assert_eq!(
        app.diff_view.filters.dataset,
        Some(DIFF_DATASETS[0]),
        "f's new filter must persist on App too, so the next o reopens with it"
    );
    assert_eq!(picker_view(&app).filters.dataset, Some(DIFF_DATASETS[0]));
}

/// `s` takes the sort over to the cursor column ascending, and flips direction when pressed
/// again on the column that already owns it - the "last column selected sorts" rule.
#[test]
fn open_diff_picker_s_sorts_by_the_cursor_column_and_flips_on_a_second_press() {
    let view = column_view(DiffColumn::Dataset);
    let options = vec![("alpha".to_string(), "handmade")];

    let app = press_in_diff_picker(options.clone(), view.clone(), &[KeyCode::Char('s')]);
    assert_eq!(
        app.diff_view.sort,
        DiffSort {
            column: DiffColumn::Dataset,
            descending: false
        }
    );

    let app = press_in_diff_picker(options, view, &[KeyCode::Char('s'), KeyCode::Char('s')]);
    assert_eq!(
        app.diff_view.sort,
        DiffSort {
            column: DiffColumn::Dataset,
            descending: true
        },
        "a second s on the same column reverses it rather than moving on"
    );
}

/// While the `Name` filter's prompt is open it takes every keystroke - so a name containing
/// `j`, `s` or `f` is typed rather than moving the selection and re-sorting mid-word.
#[test]
fn open_diff_picker_name_filter_prompt_swallows_command_keys_until_enter() {
    let options = vec![
        ("rust-add-if".to_string(), "handmade"),
        ("java-fix".to_string(), "handmade"),
    ];

    let app = press_in_diff_picker(
        options.clone(),
        DiffPickerView::default(),
        &[KeyCode::Char('f'), KeyCode::Char('j'), KeyCode::Char('s')],
    );
    match &app.modal {
        Some(Modal::OpenDiffPicker {
            name_input, view, ..
        }) => {
            assert_eq!(name_input.as_deref(), Some("js"));
            assert_eq!(
                view.sort,
                DiffSort::default(),
                "the s typed into the prompt must not have re-sorted the table"
            );
        }
        other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
    }

    // Enter commits it, lowercased, and it narrows the list.
    let app = press_in_diff_picker(
        options.clone(),
        DiffPickerView::default(),
        &[
            KeyCode::Char('f'),
            KeyCode::Char('R'),
            KeyCode::Char('u'),
            KeyCode::Enter,
        ],
    );
    assert_eq!(app.diff_view.filters.name.as_deref(), Some("ru"));
    assert_eq!(
        visible_diff_options(&options, &app.diff_view, DiffPickerData::from_app(&app)),
        vec!["rust-add-if"]
    );

    // Esc abandons the prompt and leaves the filter exactly as it was.
    let app = press_in_diff_picker(
        options,
        DiffPickerView::default(),
        &[KeyCode::Char('f'), KeyCode::Char('r'), KeyCode::Esc],
    );
    assert_eq!(picker_view(&app).filters.name, None);
    assert!(
        matches!(
            &app.modal,
            Some(Modal::OpenDiffPicker {
                name_input: None,
                ..
            })
        ),
        "Esc must close the prompt but leave the picker open"
    );
}

/// An empty submission clears the filter rather than being stored as a needle that matches
/// everything while the header still reads as filtered.
#[test]
fn open_diff_picker_name_filter_prompt_clears_on_an_empty_submission() {
    let app = press_in_diff_picker(
        vec![("rust-add-if".to_string(), "handmade")],
        name_view("rust"),
        &[
            KeyCode::Char('f'),
            KeyCode::Backspace,
            KeyCode::Backspace,
            KeyCode::Backspace,
            KeyCode::Backspace,
            KeyCode::Enter,
        ],
    );

    assert_eq!(app.diff_view.filters.name, None);
}

/// Moving the cursor must never kick off one of the corpus-wide scans - only `s`/`f` do, and
/// those are deliberate presses that can afford the seconds it costs (see
/// `ensure_diff_column_data`).
#[test]
fn open_diff_picker_column_movement_does_not_trigger_a_corpus_scan() {
    let app = press_in_diff_picker(
        vec![("alpha".to_string(), "handmade")],
        DiffPickerView::default(),
        &[
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
        ],
    );

    assert_eq!(picker_view(&app).column, DiffColumn::Disagree);
    assert!(app.diff_unmarked.is_none());
    assert!(app.diff_text_painted.is_none());
    assert!(app.diff_disagreement.is_none());
}

#[test]
fn draw_ui_shows_only_the_focused_panel_below_the_single_panel_width_threshold() {
    let before_source = "fn before_marker() {}\n";
    let after_source = "fn after_marker() {}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    let before_unmarked = count_unmarked(&before_flat, &caches, status_before);
    let after_unmarked = count_unmarked(&after_flat, &caches, status_after);

    // Narrower than `SINGLE_PANEL_WIDTH_THRESHOLD`: only the focused (Before, by
    // `App::new`'s default) panel should render, and the After panel's content shouldn't
    // appear anywhere on screen.
    let backend = ratatui::backend::TestBackend::new(SINGLE_PANEL_WIDTH_THRESHOLD - 1, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            draw_ui(
                f,
                &mut app,
                &before_flat,
                &after_flat,
                &caches,
                before_source.as_bytes(),
                after_source.as_bytes(),
                before_unmarked,
                after_unmarked,
                "test",
            )
        })
        .unwrap();
    let text = rendered_text(&terminal);
    assert!(
        text.contains("before_marker"),
        "focused panel missing from render: {text}"
    );
    assert!(
        !text.contains("after_marker"),
        "unfocused panel should not render in single-panel mode: {text}"
    );

    // At or above the threshold, both panels render side by side.
    let backend = ratatui::backend::TestBackend::new(SINGLE_PANEL_WIDTH_THRESHOLD, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            draw_ui(
                f,
                &mut app,
                &before_flat,
                &after_flat,
                &caches,
                before_source.as_bytes(),
                after_source.as_bytes(),
                before_unmarked,
                after_unmarked,
                "test",
            )
        })
        .unwrap();
    let text = rendered_text(&terminal);
    assert!(
        text.contains("before_marker"),
        "before panel missing from wide render: {text}"
    );
    assert!(
        text.contains("after_marker"),
        "after panel missing from wide render: {text}"
    );
}

#[test]
fn help_modal_renders_keybindings() {
    // Sized generously (well past HELP_TEXT's longest line and line count) so nothing is
    // clipped by the popup's width or height -- this test is about content, not layout.
    //
    // The height has to stay ahead of HELP_TEXT: the popup takes 90% of it and spends two rows
    // on borders, so it renders `0.9 * height - 2` lines. At 80 rows that was 70 against a
    // 73-line reference, and this test started failing for the reference having grown rather
    // than for anything it exists to check - which had already cost two rounds of trimming
    // real content to fit a fixture. The modal scrolls (j/k) precisely because the reference
    // outgrew one screen long ago on any ordinary terminal.
    //
    // Derived from HELP_TEXT rather than fixed, so the next entry added to the reference does not
    // fail this test again: at 90% minus the two border rows, the popup needs `(lines + 2) / 0.9`
    // rows to show all of it.
    let lines = HELP_TEXT.lines().count();
    let height = ((lines + 2) as f32 / 0.9).ceil() as u16 + 1;
    let backend = ratatui::backend::TestBackend::new(140, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 140, height);

    terminal.draw(|f| render_help_modal(f, area, 0)).unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("Keybindings"),
        "help title missing from render: {text}"
    );
    assert!(
        text.contains("switch focus between"),
        "first entry missing from render: {text}"
    );
    assert!(
        text.contains("toggle this help"),
        "help entry missing from render: {text}"
    );
    assert!(
        text.contains("quit"),
        "last entry missing from render: {text}"
    );
}

/// Parses a tiny Rust snippet for the `fully_solved_nodes`/`flatten_visible` tests below,
/// decoupled from any real fixture on disk.
fn parse_rust(source: &str) -> tree_sitter::Tree {
    let language =
        codediff::code::language::to_treesitter(&codediff::code::Language::Rust).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    parser.parse(source, None).unwrap()
}

fn find_first<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| find_first(child, kind))
}

/// The function body's two top-level statements, `a();` and `b();`.
fn two_statements(root: Node) -> (Node, Node) {
    let block = find_first(root, "block").unwrap();
    let mut cursor = block.walk();
    let statements: Vec<Node> = block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement")
        .collect();
    assert_eq!(statements.len(), 2);
    (statements[0], statements[1])
}

/// Every `expression_statement` directly in the function body - unlike `two_statements`,
/// doesn't assume a fixed count, so it works for the 2/3-identical-`foo()`-call fixtures the
/// multi-map group tests below use.
fn block_statements(root: Node) -> Vec<Node> {
    let block = find_first(root, "block").unwrap();
    let mut cursor = block.walk();
    block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement")
        .collect()
}

fn mark_subtree_matched(node: Node, caches: &mut Caches) {
    caches.before_match.insert(node.id(), usize::MAX);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        mark_subtree_matched(child, caches);
    }
}

#[test]
fn fully_solved_nodes_hides_fully_marked_subtree_but_keeps_unmarked_ancestors() {
    let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
    let root = tree.root_node();
    let (stmt_a, stmt_b) = two_statements(root);
    let block = find_first(root, "block").unwrap();

    let mut caches = Caches::default();
    // `a();` and everything under it is matched: fully solved.
    mark_subtree_matched(stmt_a, &mut caches);
    // `b();` itself is matched, but its `call_expression` child is left Unmarked, so the
    // statement as a whole is not fully solved.
    caches.before_match.insert(stmt_b.id(), usize::MAX);

    let solved = fully_solved_nodes(root, &caches, status_before);

    assert!(
        solved.contains(&stmt_a.id()),
        "fully marked subtree should be solved"
    );
    assert!(
        !solved.contains(&stmt_b.id()),
        "partially marked subtree should not be solved"
    );
    assert!(
        !solved.contains(&block.id()),
        "block has an unsolved descendant, so isn't solved"
    );
    assert!(
        !solved.contains(&root.id()),
        "root has an unsolved descendant, so isn't solved"
    );
}

#[test]
fn count_unmarked_nodes_in_tree_counts_every_hole_not_just_the_first() {
    let tree = parse_rust("fn main() {\n    a();\n}\n");
    let root = tree.root_node();

    let total = count_unmarked_nodes_in_tree(root, &Caches::default(), status_before);
    assert!(
        total > 1,
        "nothing marked at all should count every node in the tree, got {total}"
    );

    let mut caches = Caches::default();
    mark_subtree_matched(root, &mut caches);
    assert_eq!(
        count_unmarked_nodes_in_tree(root, &caches, status_before),
        0,
        "every node (including unnamed tokens) is marked, so none should be left unmarked"
    );

    // Unmark two nodes again. The boolean predicate this replaced would have stopped at the
    // first; the count is what the picker's `Unmarked` column ranks by, so it has to see both.
    let stmt = find_first(root, "expression_statement").unwrap();
    let call = find_first(stmt, "call_expression").unwrap();
    caches.before_match.remove(&call.id());
    caches.before_match.remove(&stmt.id());
    assert_eq!(
        count_unmarked_nodes_in_tree(root, &caches, status_before),
        2,
        "two holes anywhere in the tree should count as two, not as one"
    );
}

/// The work queue must not drop, duplicate or reorder anything relative to a plain loop -
/// every worker count has to produce the identical map. Runs over enough synthetic names that
/// the shared cursor is genuinely contended, with a `scan` that returns `None` for some of
/// them so the filtering path is covered too.
#[test]
fn scan_corpus_returns_the_same_map_at_every_worker_count() {
    let names: Vec<String> = (0..500).map(|i| format!("case-{i:03}")).collect();
    // Deliberately a pure function of the name, so the expected map is knowable independently
    // of which thread happened to run which entry.
    let scan = |name: &str| {
        let n: usize = name.trim_start_matches("case-").parse().unwrap();
        (n % 3 != 0).then_some(n * 2)
    };

    let sequential = scan_corpus_with_threads(&names, 1, scan);
    assert_eq!(
        sequential.len(),
        500 - 500usize.div_ceil(3),
        "the sequential baseline itself must drop exactly the None entries"
    );

    for threads in [2, 3, 8, 64] {
        assert_eq!(
            scan_corpus_with_threads(&names, threads, scan),
            sequential,
            "{threads} workers must produce the same map as one"
        );
    }
}

/// More workers than entries must not spawn idle threads or lose work - the cursor runs out
/// immediately for most of them.
#[test]
fn scan_corpus_handles_more_workers_than_entries() {
    let names = vec!["only".to_string()];
    assert_eq!(
        scan_corpus_with_threads(&names, 8, |name| Some(name.len())),
        std::collections::HashMap::from([("only".to_string(), 4)])
    );
    assert!(
        scan_corpus_with_threads(&[], 8, |name: &str| Some(name.len())).is_empty(),
        "an empty corpus is not an error"
    );
}

/// A panicking worker must take the process down the way a sequential scan always did, rather
/// than quietly handing back a map missing that thread's share - which would read as `?` in
/// the picker and be indistinguishable from "not scanned yet".
#[test]
fn scan_corpus_propagates_a_worker_panic() {
    let names: Vec<String> = (0..64).map(|i| format!("case-{i}")).collect();

    // The default hook would dump a backtrace for a panic this test is deliberately causing.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        scan_corpus_with_threads(&names, 4, |name: &str| {
            if name == "case-40" {
                panic!("scan blew up");
            }
            Some(name.len())
        })
    });

    std::panic::set_hook(hook);

    assert!(result.is_err(), "the panic must not be swallowed");
}

#[test]
fn default_scan_threads_is_at_least_one_and_within_the_cap() {
    let threads = default_scan_threads();
    assert!(
        (1..=MAX_SCAN_THREADS).contains(&threads),
        "got {threads}, outside 1..={MAX_SCAN_THREADS}"
    );
}

#[test]
fn diff_case_unmarked_count_returns_some_for_a_real_case_on_disk() {
    // Full integrated path (`diffs_case_dir`/`code_pair_from_dir`/`human_mapping::load`, not
    // crafted temp files) against whatever's actually checked in under src/test/data/diffs/ -
    // skips rather than fails if none exist yet, same convention
    // `sample_diff_line_count_is_nonzero_for_a_real_sample_on_disk` uses for this repo's
    // optional/local-only test data.
    let Ok(options) = list_available_cases() else {
        return;
    };
    let Some((name, _)) = options.first() else {
        return;
    };
    assert!(
        diff_case_unmarked_count(name).is_some(),
        "a real, on-disk case should always resolve to Some(_), not None"
    );
}

#[test]
fn open_diff_picker_f_on_cmpl_uses_the_cached_unmarked_map_without_recomputing_it() {
    let view = column_view(DiffColumn::Cmpl);

    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    // Pre-seeded, so this exercises only the filter - not the lazy full-corpus scan (see
    // `open_diff_picker_f_computes_the_unmarked_map_lazily_when_not_yet_cached` for that).
    app.diff_unmarked = Some(std::collections::HashMap::from([
        ("alpha".to_string(), 3),
        ("bravo".to_string(), 0),
    ]));
    app.modal = Some(Modal::OpenDiffPicker {
        options: vec![
            ("alpha".to_string(), "handmade"),
            ("bravo".to_string(), "handmade"),
        ],
        selected: 0,
        view,
        name_input: None,
    });
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    handle_modal_key(
        &mut app,
        KeyCode::Char('f'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    assert_eq!(
        app.diff_view.filters.cmpl,
        FlagFilter::Yes,
        "f should persist the new filter on App too, so the next o reopens with it"
    );
    match &app.modal {
        Some(Modal::OpenDiffPicker { view, options, .. }) => {
            assert_eq!(view.filters.cmpl, FlagFilter::Yes);
            assert_eq!(
                options.len(),
                2,
                "the full options list itself is untouched"
            );
        }
        other => panic!("expected Modal::OpenDiffPicker to stay open, got {other:?}"),
    }
    assert_eq!(
        app.diff_unmarked.as_ref().unwrap().len(),
        2,
        "an already-cached map should not be recomputed"
    );
}

#[test]
fn open_diff_picker_f_computes_the_unmarked_map_lazily_when_not_yet_cached() {
    // Full integrated path against whatever's actually under src/test/data/diffs/ - skips if
    // there's nothing to scan, same convention as
    // `diff_case_unmarked_count_returns_some_for_a_real_case_on_disk` above.
    let Ok(options) = list_available_cases() else {
        return;
    };
    if options.is_empty() {
        return;
    }

    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    assert!(app.diff_unmarked.is_none());

    let view = column_view(DiffColumn::Unmarked);
    app.modal = Some(Modal::OpenDiffPicker {
        options: options.clone(),
        selected: 0,
        view,
        name_input: None,
    });
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    handle_modal_key(
        &mut app,
        KeyCode::Char('s'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    let map = app
        .diff_unmarked
        .as_ref()
        .expect("s on Unmarked should compute the map lazily when it wasn't cached yet");
    // Cases that fail to load are deliberately absent rather than present with a made-up
    // count, so an equality would be wrong - but a *loose* upper bound alone would pass even
    // if the parallel work queue silently dropped most of the corpus, which is the thing
    // worth guarding here. `scan_corpus_returns_the_same_map_at_every_worker_count` proves
    // the queue itself loses nothing; this only has to prove the real scan is wired to it and
    // reaches nearly every case.
    assert!(map.len() <= options.len());
    assert!(
        map.len() * 10 >= options.len() * 9,
        "the scan covered only {} of {} cases - the work queue is dropping fixtures",
        map.len(),
        options.len()
    );
}

#[test]
fn flatten_visible_skips_hidden_subtree_entirely_but_keeps_siblings() {
    let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
    let root = tree.root_node();
    let (stmt_a, stmt_b) = two_statements(root);

    let mut hidden = std::collections::HashSet::new();
    hidden.insert(stmt_a.id());

    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        Some(&hidden),
    ));

    assert!(
        !flat.iter().any(|(n, _)| n.id() == stmt_a.id()),
        "hidden node itself should not appear"
    );
    assert!(
        flat.iter().any(|(n, _)| n.id() == stmt_b.id()),
        "sibling of a hidden node should still appear"
    );
    // The root is always an ancestor of the still-visible sibling, so it must survive too.
    assert!(flat.iter().any(|(n, _)| n.id() == root.id()));
}

/// The synthetic-Caches tests above prove `fully_solved_nodes` is correct *given* every node
/// in a subtree (including unnamed tokens like `;`, `{`, `}`) has an entry. They don't prove
/// the real marking path actually produces that. `M` (`auto_match_pair`) is the tool the docs
/// point people at for matching a whole subtree at once, specifically so `H` has something to
/// hide -- this drives it for real, on a real (unchanged) pair, and checks the result through
/// the same `rebuild_caches` -> `fully_solved_nodes` pipeline the running app uses.
#[test]
fn fully_solved_nodes_hides_a_subtree_matched_for_real_via_m() {
    let source = "fn main() {\n    a();\n    b();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let (before_stmt_a, _) = two_statements(before_root);
    let (after_stmt_a, _) = two_statements(after_root);
    let (_, before_stmt_b) = two_statements(before_root);

    let mut mapping = HumanMapping::default();
    let mut matched = 0usize;
    let mut skipped = 0usize;
    let mut before_collapsed = std::collections::HashSet::new();
    let mut after_collapsed = std::collections::HashSet::new();
    let before_paths = precompute_paths(before_root);
    let after_paths = precompute_paths(after_root);
    let mut touched_before = std::collections::HashSet::new();
    let mut touched_after = std::collections::HashSet::new();

    auto_match_pair(
        &mut mapping.entries,
        &mut touched_before,
        &mut touched_after,
        &Caches::default(),
        before_stmt_a,
        after_stmt_a,
        source.as_bytes(),
        source.as_bytes(),
        &before_paths,
        &after_paths,
        &mut matched,
        &mut skipped,
        &mut before_collapsed,
        &mut after_collapsed,
    );

    let caches = rebuild_caches(&mapping.entries, before_root, after_root);
    assert_eq!(
        caches.unresolved, 0,
        "every entry M produced should resolve: {:?}",
        mapping.entries
    );

    let solved = fully_solved_nodes(before_root, &caches, status_before);
    assert!(
        solved.contains(&before_stmt_a.id()),
        "M should mark every child (including unnamed tokens), fully solving the subtree: {:?}",
        mapping.entries
    );
    assert!(
        !solved.contains(&before_stmt_b.id()),
        "b(); was never matched, so it must not be treated as solved"
    );
}

#[test]
fn action_match_to_end_matches_identical_trees_completely() {
    let source = "fn main() {\n    a();\n    b();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let no_hashes = rustc_hash::FxHashMap::default();

    let outcome = action_match_to_end(
        &mut app,
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
    )
    .unwrap();

    assert!(matches!(outcome, ActionOutcome::Done(_)));
    assert!(app.dirty);

    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    for (node, _) in before_flat.iter() {
        assert_ne!(
            status_before(*node, &caches),
            NodeStatus::Unmarked,
            "every node, including unnamed tokens, should be matched: {:?} unmatched",
            node.kind()
        );
    }

    // Running it again once everything is matched must be a no-op, not a duplicate sweep.
    let entries_before = app.mapping.entries.len();
    let outcome = action_match_to_end(
        &mut app,
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
    )
    .unwrap();
    assert!(matches!(outcome, ActionOutcome::Done(ref msg) if msg == "Nothing left to match"));
    assert_eq!(app.mapping.entries.len(), entries_before);
}

#[test]
fn action_match_to_end_stops_at_a_kind_mismatch_but_keeps_prior_matches() {
    let before_source = "fn main() {\n    a();\n}\n";
    let after_source = "fn main() {\n    1;\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let no_hashes = rustc_hash::FxHashMap::default();

    let outcome = action_match_to_end(
        &mut app,
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        before_source.as_bytes(),
        after_source.as_bytes(),
        &no_hashes,
        &no_hashes,
    )
    .unwrap();

    let (before_id, after_id, before_kind, after_kind) = match outcome {
        ActionOutcome::NeedsModal(modal) => match *modal {
            Modal::ConfirmKindMismatch {
                before_id,
                after_id,
                before_kind,
                after_kind,
                recursive,
            } => {
                assert!(
                    !recursive,
                    "f should raise a single-pair mismatch, not a recursive one"
                );
                (before_id, after_id, before_kind, after_kind)
            }
            other => panic!("expected ConfirmKindMismatch, got {other:?}"),
        },
        ActionOutcome::Done(msg) => {
            panic!("expected a kind mismatch modal, action completed instead: {msg}")
        }
    };
    assert_ne!(before_kind, after_kind);

    // The common prefix (fn main() { ... before the differing statement) must already be
    // matched, even though the sweep didn't run to completion.
    assert!(app.dirty);
    assert!(
        !app.mapping.entries.is_empty(),
        "should have matched at least the common prefix"
    );

    // The cursor is parked exactly on the mismatched pair, ready for a human (or a plain `m`)
    // to resolve it and then resume with `f` again.
    assert_eq!(app.before.cursor_id, before_id);
    assert_eq!(app.after.cursor_id, after_id);
}

fn collect_subtree_ids(node: Node, out: &mut Vec<usize>) {
    out.push(node.id());
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_subtree_ids(child, out);
    }
}

#[test]
fn action_match_to_end_does_not_pair_a_trailing_statement_against_the_wrong_node() {
    // Before has an extra trailing statement the After side has nothing to pair it with.
    // `f` pairs cursors positionally (exactly like repeated `m`), so once the shared `a(); b();`
    // prefix is consumed the After cursor lands on the block's closing `}` while the Before
    // cursor is still sitting on `c();` -- a kind mismatch, so the sweep must stop there rather
    // than inventing a match for `c();`.
    let before_source = "fn main() {\n    a();\n    b();\n    c();\n}\n";
    let after_source = "fn main() {\n    a();\n    b();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let block = find_first(before_root, "block").unwrap();
    let mut cursor = block.walk();
    let statement_c = block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement")
        .nth(2)
        .expect("before source has three statements");
    let mut untouchable_ids = Vec::new();
    collect_subtree_ids(statement_c, &mut untouchable_ids);

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let no_hashes = rustc_hash::FxHashMap::default();

    let outcome = action_match_to_end(
        &mut app,
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        before_source.as_bytes(),
        after_source.as_bytes(),
        &no_hashes,
        &no_hashes,
    )
    .unwrap();

    assert!(
        app.dirty,
        "the shared a(); b(); prefix should have been matched"
    );
    match outcome {
        ActionOutcome::NeedsModal(modal) => match *modal {
            Modal::ConfirmKindMismatch {
                before_kind,
                after_kind,
                ..
            } => assert_ne!(before_kind, after_kind),
            other => panic!("expected ConfirmKindMismatch, got {other:?}"),
        },
        ActionOutcome::Done(msg) => panic!(
            "expected the sweep to stop on a kind mismatch once `c();` has nothing left to pair \
             with, action completed instead: {msg}"
        ),
    }

    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    for id in untouchable_ids {
        assert!(
            !caches.before_match.contains_key(&id) && !caches.before_removed.contains_key(&id),
            "the trailing `c();` statement (or any of its children) must not have been paired \
             with anything on the After side"
        );
    }
}

/// Regression guard for a real hang: on a ~5,500-node real-world fixture, the original
/// implementation (which called `apply_match_entry`/`rebuild_caches` -- each O(current entry
/// count) -- once per node, and re-derived the cursor's flat-array position by linear scan
/// every iteration) took 26s to reach only 740 of 5477 matches, and was still accelerating: an
/// effective hang for anything but a toy fixture. Generates a large *synthetic* identical
/// before/after tree (no dependency on any file under src/test/data/, which can be renamed or
/// removed) and asserts the sweep finishes fast. If this regresses back to quadratic, this
/// test will time out or take drastically longer, not just get slower by a little.
#[test]
fn action_match_to_end_is_linear_not_quadratic_in_tree_size() {
    let mut source = String::from("fn main() {\n");
    for i in 0..3000 {
        source.push_str(&format!("    a{i}();\n"));
    }
    source.push_str("}\n");

    let before_tree = parse_rust(&source);
    let after_tree = parse_rust(&source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &std::collections::HashSet::new(),
        None,
    ));
    assert!(
        before_flat.len() > 10_000,
        "expected a large tree, got {} nodes",
        before_flat.len()
    );

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    let no_hashes = rustc_hash::FxHashMap::default();

    let start = std::time::Instant::now();
    let outcome = action_match_to_end(
        &mut app,
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
    )
    .unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(outcome, ActionOutcome::Done(_)));
    assert_eq!(app.mapping.entries.len(), before_flat.len());
    assert!(
        elapsed.as_secs() < 5,
        "took {elapsed:?} to sweep {} nodes -- the old O(n^2) implementation took 26s to do \
         barely a tenth of a ~5,500-node real fixture, so anything anywhere near that here \
         means the quadratic blowup is back",
        before_flat.len()
    );
}

/// `M`'s recursive workhorse (`auto_match_pair`) had the same O(n^2) shape `f` did, for the
/// same reason: it called `apply_match_entry` (O(current entry count) per call, via
/// `remove_direct_entries_for`'s full scan) once per node in the subtree. On the same
/// ~5,500-node real fixture matched against itself (so `same_shape` holds all the way down and
/// the whole tree gets recursed), the original implementation didn't finish within 2 minutes.
/// Mirrors `action_match_to_end_is_linear_not_quadratic_in_tree_size`'s synthetic large tree so
/// this doesn't depend on any file under src/test/data/.
#[test]
fn action_match_subtree_is_linear_not_quadratic_in_tree_size() {
    let mut source = String::from("fn main() {\n");
    for i in 0..3000 {
        source.push_str(&format!("    a{i}();\n"));
    }
    source.push_str("}\n");

    let before_tree = parse_rust(&source);
    let after_tree = parse_rust(&source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &std::collections::HashSet::new(),
        None,
    ));
    assert!(
        before_flat.len() > 10_000,
        "expected a large tree, got {} nodes",
        before_flat.len()
    );

    let mut mapping = HumanMapping::default();
    let caches = Caches::default();
    let mut before_collapsed = std::collections::HashSet::new();
    let mut after_collapsed = std::collections::HashSet::new();
    let no_hashes = rustc_hash::FxHashMap::default();

    let start = std::time::Instant::now();
    let outcome = action_match_subtree(
        &mut mapping,
        &before_flat,
        &after_flat,
        before_root.id(),
        after_root.id(),
        before_root,
        after_root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &mut before_collapsed,
        &mut after_collapsed,
    )
    .unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(outcome, ActionOutcome::Done(_)));
    assert_eq!(
        mapping.entries.len(),
        before_flat.len(),
        "M should match every node in the identical subtree"
    );
    assert!(
        elapsed.as_secs() < 5,
        "took {elapsed:?} to sweep {} nodes -- the old O(n^2) implementation didn't finish \
         within 2 minutes on a comparably sized real fixture, so anything anywhere near that \
         here means the quadratic blowup is back",
        before_flat.len()
    );
}

#[test]
fn m_preserves_a_pre_existing_match_under_a_subtree_it_bails_out_of() {
    // Shapes: `if true { a(); }` before vs `if true { a(); c(); }` after -- the `if`'s inner
    // block has 3 children before, 4 after, so `auto_match_pair` bails at that block (pushes
    // one MatchButNotIdentical for the block itself, does not recurse into its children).
    // `a();`'s `expression_statement` sits *below* that bail point, so `M` (pressed above it,
    // at the whole function) should never touch its pre-existing entry.
    let before_source = "fn main() {\n    if true {\n        a();\n    }\n    b();\n}\n";
    let after_source =
        "fn main() {\n    if true {\n        a();\n        c();\n    }\n    b();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let inner_block_before = find_first(before_root, "if_expression")
        .and_then(|n| find_first(n, "block"))
        .unwrap();
    let inner_block_after = find_first(after_root, "if_expression")
        .and_then(|n| find_first(n, "block"))
        .unwrap();
    let d_before = inner_block_before
        .child(1)
        .filter(|n| n.kind() == "expression_statement")
        .expect("a(); statement");
    let d_after = inner_block_after
        .child(1)
        .filter(|n| n.kind() == "expression_statement")
        .expect("a(); statement");

    let mut mapping = HumanMapping {
        entries: vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(d_before)),
            after_path: Some(path_for_node(d_after)),
        }],
        ..Default::default()
    };

    let function_before = before_root.child(0).unwrap();
    let function_after = after_root.child(0).unwrap();
    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let no_hashes = rustc_hash::FxHashMap::default();
    let mut before_collapsed = std::collections::HashSet::new();
    let mut after_collapsed = std::collections::HashSet::new();

    action_match_subtree(
        &mut mapping,
        &before_flat,
        &after_flat,
        function_before.id(),
        function_after.id(),
        before_root,
        after_root,
        &Caches::default(),
        before_source.as_bytes(),
        after_source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &mut before_collapsed,
        &mut after_collapsed,
    )
    .unwrap();

    let caches = rebuild_caches(&mapping.entries, before_root, after_root);
    assert_eq!(
        caches.before_match.get(&d_before.id()),
        Some(&d_after.id()),
        "M bailed at an ancestor without recursing into `a();` -- its pre-existing match should \
         survive untouched, not be silently dropped: {:?}",
        mapping.entries
    );
}

#[test]
fn m_replaces_a_pre_existing_match_on_a_node_it_actually_revisits() {
    // Identical before/after: `same_shape` holds at every level, so `M` pressed at the root
    // recurses all the way down and revisits every node, including `a();`. Pre-seed a *wrong*
    // pre-existing entry for `a();` (pointing at `b();` instead of its own counterpart) and
    // confirm `M` replaces it with exactly one correct entry -- not a leftover stale one
    // alongside the new one, which would silently corrupt the saved mapping.
    let source = "fn main() {\n    a();\n    b();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let (before_stmt_a, _) = two_statements(before_root);
    let (after_stmt_a, after_stmt_b) = two_statements(after_root);

    let mut mapping = HumanMapping {
        entries: vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(before_stmt_a)),
            after_path: Some(path_for_node(after_stmt_b)), // deliberately wrong partner
        }],
        ..Default::default()
    };

    let function_before = before_root.child(0).unwrap();
    let function_after = after_root.child(0).unwrap();
    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let no_hashes = rustc_hash::FxHashMap::default();
    let mut before_collapsed = std::collections::HashSet::new();
    let mut after_collapsed = std::collections::HashSet::new();

    action_match_subtree(
        &mut mapping,
        &before_flat,
        &after_flat,
        function_before.id(),
        function_after.id(),
        before_root,
        after_root,
        &Caches::default(),
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &mut before_collapsed,
        &mut after_collapsed,
    )
    .unwrap();

    let matching: Vec<_> = mapping
        .entries
        .iter()
        .filter(|entry| {
            entry
                .before_path
                .as_ref()
                .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
                .is_some_and(|n| n.id() == before_stmt_a.id())
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one entry for `a();` after M, found {}: {:?}",
        matching.len(),
        mapping.entries
    );

    let caches = rebuild_caches(&mapping.entries, before_root, after_root);
    assert_eq!(
        caches.before_match.get(&before_stmt_a.id()),
        Some(&after_stmt_a.id()),
        "M should have replaced the stale wrong-partner entry with the correct one: {:?}",
        mapping.entries
    );
}

fn write_csv(path: &Path, rows: &[(&str, &str, &str, &str, &str, &str)]) {
    let mut writer = csv::Writer::from_path(path).unwrap();
    writer
        .write_record([
            "language",
            "repository",
            "commit",
            "path",
            "promoted_to",
            "dataset",
        ])
        .unwrap();
    for (language, repository, commit, row_path, promoted_to, dataset) in rows {
        writer
            .write_record([language, repository, commit, row_path, promoted_to, dataset])
            .unwrap();
    }
    writer.flush().unwrap();
}

/// (path, promoted_to, dataset) per row - the three columns every test in this section
/// actually cares about; language/repository/commit are only there to match rows.
fn read_csv(path: &Path) -> Vec<(String, String, String)> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    reader
        .records()
        .map(|r| {
            let r = r.unwrap();
            (
                r[3].to_string(),
                r.get(4).unwrap_or("").to_string(),
                r.get(5).unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn update_sample_csv_sets_promoted_to_on_the_matching_row_only() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[
            ("Rust", "repo", "abc123", "src/a.rs", "", "small"),
            ("Rust", "repo", "def456", "src/b.rs", "", "full"),
        ],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
    assert!(found);

    let rows = read_csv(file.path());
    assert_eq!(
        rows,
        vec![
            (
                "src/a.rs".to_string(),
                "rust-new-case".to_string(),
                "small".to_string()
            ),
            (
                "src/b.rs".to_string(),
                "".to_string(),
                // Every other row's dataset must survive untouched, same as its other
                // columns - this is the one column a naive "just rewrite promoted_to"
                // implementation could plausibly clobber.
                "full".to_string()
            ),
        ]
    );
}

#[test]
fn update_sample_csv_returns_false_when_no_row_matches() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "other-repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found = update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap();
    assert!(!found);

    // Untouched: no row matched, so nothing should have been rewritten.
    let rows = read_csv(file.path());
    assert_eq!(
        rows,
        vec![("src/a.rs".to_string(), "".to_string(), "small".to_string())]
    );
}

#[test]
fn update_sample_csv_returns_false_when_file_does_not_exist() {
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found =
        update_sample_csv_at(Path::new("/nonexistent/sample.csv"), &source, "name").unwrap();
    assert!(!found);
}

#[test]
fn sample_triage_statuses_at_reads_the_status_column_for_every_row() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[
            (
                "Rust",
                "repo",
                "abc123",
                "src/a.rs",
                "rust-already-promoted",
                "small",
            ),
            ("Rust", "repo", "def456", "src/b.rs", "", "small"),
        ],
    );
    // Reject the second row so the map has one of each of the three statuses (the third being
    // whatever `write_csv` alone leaves as its backward-compat default, exercised by the
    // no-`status`-column case below).
    let rejected_source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "def456".to_string(),
        path: "src/b.rs".to_string(),
        dataset: "small".to_string(),
    };
    reject_sample_csv_at(file.path(), &rejected_source, "not interesting").unwrap();

    let statuses = sample_metadata_at(file.path()).unwrap();
    assert_eq!(
        statuses
            .get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "abc123".to_string(),
                "src/a.rs".to_string(),
            ))
            .map(|meta| meta.status),
        Some(SampleTriageStatus::Promoted)
    );
    assert_eq!(
        statuses
            .get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "def456".to_string(),
                "src/b.rs".to_string(),
            ))
            .map(|meta| meta.status),
        Some(SampleTriageStatus::Rejected)
    );
}

#[test]
fn sample_metadata_at_defaults_an_unmatched_row_to_sampled() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );

    let statuses = sample_metadata_at(file.path()).unwrap();
    assert_eq!(
        statuses
            .get(&(
                "Rust".to_string(),
                "repo".to_string(),
                "abc123".to_string(),
                "src/a.rs".to_string(),
            ))
            .map(|meta| meta.status),
        Some(SampleTriageStatus::Sampled)
    );
}

#[test]
fn sample_metadata_at_is_empty_when_file_does_not_exist() {
    let statuses = sample_metadata_at(Path::new("/nonexistent/sample.csv")).unwrap();
    assert!(statuses.is_empty());
}

/// (path, promoted_to, dataset, status, comment) per row - like `read_csv`, but for tests that
/// also care about the two newest columns.
fn read_csv_with_status(path: &Path) -> Vec<(String, String, String, String, String)> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    reader
        .records()
        .map(|r| {
            let r = r.unwrap();
            (
                r[3].to_string(),
                r.get(4).unwrap_or("").to_string(),
                r.get(5).unwrap_or("").to_string(),
                r.get(6).unwrap_or("").to_string(),
                r.get(7).unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn update_sample_csv_sets_status_to_promoted() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    assert!(update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap());

    let rows = read_csv_with_status(file.path());
    assert_eq!(
        rows,
        vec![(
            "src/a.rs".to_string(),
            "rust-new-case".to_string(),
            "small".to_string(),
            "PROMOTED".to_string(),
            "".to_string(),
        )]
    );
}

#[test]
fn reject_sample_csv_at_sets_reason_and_status_without_touching_promoted_to() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[
            ("Rust", "repo", "abc123", "src/a.rs", "", "small"),
            ("Rust", "repo", "def456", "src/b.rs", "", "full"),
        ],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found =
        reject_sample_csv_at(file.path(), &source, "duplicate of an existing case").unwrap();
    assert!(found);

    let rows = read_csv_with_status(file.path());
    assert_eq!(
        rows,
        vec![
            (
                "src/a.rs".to_string(),
                // Rejection must never populate promoted_to.
                "".to_string(),
                "small".to_string(),
                "REJECTED".to_string(),
                "duplicate of an existing case".to_string(),
            ),
            (
                "src/b.rs".to_string(),
                "".to_string(),
                "full".to_string(),
                "SAMPLED".to_string(),
                "".to_string(),
            ),
        ]
    );
}

#[test]
fn reject_sample_csv_at_returns_false_when_no_row_matches() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "other-repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found = reject_sample_csv_at(file.path(), &source, "reason").unwrap();
    assert!(!found);

    // Untouched: no row matched, so the file is never rewritten -- still the original
    // (pre-`status`/`comment`) 6-column shape, not a backfilled 8-column one.
    let rows = read_csv_with_status(file.path());
    assert_eq!(
        rows,
        vec![(
            "src/a.rs".to_string(),
            "".to_string(),
            "small".to_string(),
            "".to_string(),
            "".to_string(),
        )]
    );
}

#[test]
fn set_sample_comment_at_sets_comment_without_touching_status_or_promoted_to() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[
            (
                "Rust",
                "repo",
                "abc123",
                "src/a.rs",
                "rust-already-promoted",
                "small",
            ),
            ("Rust", "repo", "def456", "src/b.rs", "", "full"),
        ],
    );

    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    let found = set_sample_comment_at(file.path(), &source, "worth a second look").unwrap();
    assert!(found);

    let rows = read_csv_with_status(file.path());
    assert_eq!(
        rows,
        vec![
            (
                "src/a.rs".to_string(),
                // A comment must never touch promoted_to or status - unlike reject, this can
                // be set on an already-PROMOTED row without disturbing either.
                "rust-already-promoted".to_string(),
                "small".to_string(),
                "PROMOTED".to_string(),
                "worth a second look".to_string(),
            ),
            (
                "src/b.rs".to_string(),
                "".to_string(),
                "full".to_string(),
                "SAMPLED".to_string(),
                "".to_string(),
            ),
        ]
    );
}

#[test]
fn set_sample_comment_at_with_an_empty_comment_clears_a_previous_one() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    assert!(set_sample_comment_at(file.path(), &source, "first note").unwrap());
    assert!(set_sample_comment_at(file.path(), &source, "").unwrap());

    let rows = read_csv_with_status(file.path());
    assert_eq!(rows[0].4, "");
}

#[test]
fn set_sample_comment_at_returns_false_when_no_row_matches() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "other-repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    assert!(!set_sample_comment_at(file.path(), &source, "note").unwrap());
}

#[test]
fn sample_comment_at_returns_the_trimmed_comment_when_present() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    set_sample_comment_at(file.path(), &source, "  needs a closer look  ").unwrap();

    assert_eq!(
        sample_comment_at(file.path(), &source).unwrap(),
        Some("needs a closer look".to_string())
    );
}

#[test]
fn sample_comment_at_is_none_when_comment_is_empty_or_row_is_missing() {
    let file = NamedTempFile::new().unwrap();
    write_csv(
        file.path(),
        &[("Rust", "repo", "abc123", "src/a.rs", "", "small")],
    );
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "small".to_string(),
    };
    // Present row, never-set comment.
    assert_eq!(sample_comment_at(file.path(), &source).unwrap(), None);

    // No matching row at all.
    let missing_source = SampleSource {
        repository: "other-repo".to_string(),
        ..source
    };
    assert_eq!(
        sample_comment_at(file.path(), &missing_source).unwrap(),
        None
    );
}

#[test]
fn algo_reason_reports_the_pass_that_produced_each_side_of_a_match() {
    let source = "fn f() { a(); }\n";
    let before = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
    let after = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
    let diff = diff_code(&before, &after);
    let diff_ast = diff.ast.expect("diff has AST");

    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    // Identical before/after source: the whole tree matches via a single hash comparison at
    // the root, so both roots should report `IdenticalHash` - and nothing further down should
    // even have its own entry (see `add_delete_mappings`'s sibling passes: a hash-matched
    // subtree's descendants are never visited individually).
    let before_reason = algo_reason_before(before_ast.root_node(), &diff_ast);
    let after_reason = algo_reason_after(after_ast.root_node(), &diff_ast);
    assert_eq!(before_reason, Some(ASTMappingReason::IdenticalHash));
    assert_eq!(after_reason, Some(ASTMappingReason::IdenticalHash));
    assert_eq!(reason_label(before_reason.unwrap()), "IdHash");
}

#[test]
fn algo_reason_is_none_when_the_diff_has_no_entry_for_the_node() {
    // A fresh, unpopulated ASTDiff has no entries at all, so every lookup should miss cleanly
    // rather than panicking - this is the state before `p` has ever been pressed.
    let source = "fn f() {}\n";
    let code = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
    let root = code.ast.as_ref().unwrap().root_node();
    let empty_diff = ASTDiff::default();

    assert_eq!(algo_reason_before(root, &empty_diff), None);
    assert_eq!(algo_reason_after(root, &empty_diff), None);
}

#[test]
fn reason_label_matches_benchmark_optimal_solutions_abbreviations() {
    // Kept in sync by hand with `src/bin/benchmark_optimal_solutions.rs`'s `REASONS` table -
    // this test exists so a label drift between the two tools fails loudly instead of quietly
    // making the same abbreviation mean two different things.
    assert_eq!(reason_label(ASTMappingReason::IdenticalHash), "IdHash");
    assert_eq!(
        reason_label(ASTMappingReason::IdenticalHashOfAncestor),
        "IdHashAnc"
    );
    assert_eq!(
        reason_label(ASTMappingReason::FullyMappingSubtrees),
        "FullMap"
    );
    assert_eq!(
        reason_label(ASTMappingReason::StructurallyIdenticalSubtrees),
        "StructId"
    );
    assert_eq!(
        reason_label(ASTMappingReason::StructurallyIdenticalAncestor),
        "StructAnc"
    );
    assert_eq!(reason_label(ASTMappingReason::OptimalIDU), "OptIDU");
    assert_eq!(reason_label(ASTMappingReason::APTED("final_pass")), "APTED");
    assert_eq!(reason_label(ASTMappingReason::FlatSequenceDiff), "FlatSeq");
    assert_eq!(reason_label(ASTMappingReason::MovedSubtree), "Moved");
    assert_eq!(reason_label(ASTMappingReason::LeadingSibling), "LeadSib");
    assert_eq!(
        reason_label(ASTMappingReason::GreedyAnchorBlock),
        "GreedyAnchor"
    );
    assert_eq!(
        reason_label(ASTMappingReason::BottomUpPropagation),
        "BottomUpProp"
    );
}

#[test]
fn reason_detail_shows_apted_provenance_but_reason_label_does_not() {
    let reason = ASTMappingReason::APTED("bottom_up_expansion");
    assert_eq!(reason_label(reason), "APTED");
    assert_eq!(reason_detail(reason), "APTED:bottom_up_expansion");
    // Every other variant has no payload to show, so `reason_detail` just falls back to the
    // same short label as `reason_label`.
    assert_eq!(
        reason_detail(ASTMappingReason::BottomUpPropagation),
        "BottomUpProp"
    );
}

// -----------------------------------------------------------------------------------------
// Multi-map groups (Phase 2: x/c selection, m/M commit, u removal)
// -----------------------------------------------------------------------------------------

#[test]
fn multi_map_group_operation_is_identical_only_when_every_member_shares_one_hash() {
    let mut before_hash = rustc_hash::FxHashMap::default();
    let mut after_hash = rustc_hash::FxHashMap::default();
    before_hash.insert(1, 42);
    before_hash.insert(2, 42);
    after_hash.insert(10, 42);
    after_hash.insert(11, 42);

    let before_ids: std::collections::BTreeSet<usize> = [1, 2].into_iter().collect();
    let after_ids: std::collections::BTreeSet<usize> = [10, 11].into_iter().collect();

    assert_eq!(
        multi_map_group_operation(&before_ids, &after_ids, &before_hash, &after_hash),
        HumanOperation::Identical
    );

    after_hash.insert(11, 99);
    assert_eq!(
        multi_map_group_operation(&before_ids, &after_ids, &before_hash, &after_hash),
        HumanOperation::MatchButNotIdentical
    );
}

#[test]
fn commit_multi_map_group_replaces_any_prior_entry_touching_its_nodes() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);
    assert_eq!(before_foos.len(), 3);
    assert_eq!(after_foos.len(), 2);

    // A pre-existing plain entry pairing the first before-foo with the first after-foo, which
    // the new group commit (covering all 3/2) should displace.
    let mut mapping = HumanMapping {
        entries: vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(before_foos[0])),
            after_path: Some(path_for_node(after_foos[0])),
        }],
        ..Default::default()
    };

    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();

    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        HumanOperation::Identical,
        true,
    )
    .unwrap();

    assert!(
        mapping.entries.is_empty(),
        "the pre-existing plain entry touching a group member should be removed: {:?}",
        mapping.entries
    );
    assert_eq!(mapping.groups.len(), 1);
    assert_eq!(mapping.groups[0].before_paths.len(), 3);
    assert_eq!(mapping.groups[0].after_paths.len(), 2);
    assert!(mapping.groups[0].with_children);
}

#[test]
fn commit_multi_map_group_replaces_a_prior_group_sharing_a_node() {
    let source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);

    let first_pair_before: std::collections::BTreeSet<usize> =
        [before_foos[0].id(), before_foos[1].id()]
            .into_iter()
            .collect();
    let first_pair_after: std::collections::BTreeSet<usize> =
        [after_foos[0].id(), after_foos[1].id()]
            .into_iter()
            .collect();
    let mut mapping = HumanMapping::default();
    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &first_pair_before,
        &first_pair_after,
        HumanOperation::Identical,
        false,
    )
    .unwrap();
    assert_eq!(mapping.groups.len(), 1);

    // A second group sharing before_foos[1] with the first should replace it, not coexist.
    let second_before: std::collections::BTreeSet<usize> =
        [before_foos[1].id(), before_foos[2].id()]
            .into_iter()
            .collect();
    let second_after: std::collections::BTreeSet<usize> =
        [after_foos[2].id()].into_iter().collect();
    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &second_before,
        &second_after,
        HumanOperation::Identical,
        false,
    )
    .unwrap();

    assert_eq!(
        mapping.groups.len(),
        1,
        "the first group should have been dropped, not left alongside the second: {:?}",
        mapping.groups
    );
    assert_eq!(mapping.groups[0].before_paths.len(), 2);
}

#[test]
fn commit_multi_map_group_orders_paths_by_source_position_not_by_arena_id() {
    // A `BTreeSet<usize>` orders by node id, not source position - parse-unstable (same
    // lesson as this project's benchmark-determinism-fix). The committed group's paths must
    // come out in source order regardless, so re-selecting the same nodes in a later session
    // can't shuffle a `human_mapping.json` that otherwise didn't change.
    let source = "fn main() {\n    a();\n    b();\n    c();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let stmts = block_statements(root);
    assert_eq!(stmts.len(), 3);

    let mut mapping = HumanMapping::default();
    let ids: std::collections::BTreeSet<usize> = stmts.iter().map(|n| n.id()).collect();
    commit_multi_map_group(
        &mut mapping,
        root,
        root,
        &ids,
        &ids,
        HumanOperation::Identical,
        false,
    )
    .unwrap();

    let expected: Vec<Vec<String>> = stmts.iter().map(|n| path_for_node(*n)).collect();
    assert_eq!(
        mapping.groups[0].before_paths, expected,
        "paths should be ordered by source position (a, b, c)"
    );
    assert_eq!(mapping.groups[0].after_paths, expected);
}

#[test]
fn commit_multi_map_group_with_children_clears_a_pre_existing_descendant_entry() {
    let before_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);
    assert_eq!(before_foos.len(), 2);
    assert_eq!(after_foos.len(), 1);

    // A stale entry on a descendant of `before_foos[0]` (its inner `call_expression`),
    // pointing at some unrelated after node - exactly what a `with_children` commit must
    // sweep, the same way `d`/`i`'s own `clear_before_descendants`/`clear_after_descendants`
    // calls already do for a plain delete/insert-with-children mark.
    let descendant = find_first(before_foos[0], "call_expression").unwrap();
    let mut mapping = HumanMapping {
        entries: vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(descendant)),
            after_path: Some(path_for_node(after_foos[0])),
        }],
        ..Default::default()
    };

    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();

    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        HumanOperation::Identical,
        true,
    )
    .unwrap();

    assert!(
        mapping.entries.is_empty(),
        "the stale descendant entry should have been cleared by the with_children commit: {:?}",
        mapping.entries
    );
    assert_eq!(mapping.groups.len(), 1);
}

#[test]
fn action_commit_multi_map_group_errors_when_a_member_is_under_a_deleted_with_children_ancestor() {
    let before_source = "fn main() {\n    if true {\n        foo();\n        foo();\n    }\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let if_expr = find_first(before_root, "if_expression").unwrap();
    let inner_block = find_first(if_expr, "block").unwrap();
    let mut cursor = inner_block.walk();
    let before_foos: Vec<Node> = inner_block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement")
        .collect();
    assert_eq!(before_foos.len(), 2);

    let after_foos = block_statements(after_root);
    assert_eq!(after_foos.len(), 2);

    let mut mapping = HumanMapping::default();
    let mut caches = Caches::default();
    caches.before_removed.insert(if_expr.id(), true);

    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();
    let no_hashes = rustc_hash::FxHashMap::default();

    let result = action_commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        &no_hashes,
        &no_hashes,
        &caches,
        false,
    );
    match result {
        Err(err) => assert!(
            err.to_string()
                .contains("covered by an ancestor's delete-with-children mark"),
            "{err}"
        ),
        Ok(_) => panic!(
            "expected an error: a selected node sits under an ancestor already marked deleted-with-children"
        ),
    }
    assert!(mapping.groups.is_empty());
}

#[test]
fn action_commit_multi_map_group_errors_when_one_side_is_empty() {
    let source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let mut mapping = HumanMapping::default();
    let before_ids: std::collections::BTreeSet<usize> = block_statements(before_root)
        .iter()
        .map(|n| n.id())
        .collect();
    let after_ids = std::collections::BTreeSet::new();
    let no_hashes = rustc_hash::FxHashMap::default();

    let result = action_commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        &no_hashes,
        &no_hashes,
        &Caches::default(),
        false,
    );
    match result {
        Err(err) => assert!(
            err.to_string()
                .contains("at least one selected node on both sides"),
            "{err}"
        ),
        Ok(_) => panic!("expected an error when one side of the selection is empty"),
    }
    assert!(mapping.groups.is_empty());
}

#[test]
fn action_commit_multi_map_group_raises_a_modal_for_mixed_kinds() {
    let before_source = "fn main() {\n    foo();\n    let x = 1;\n}\n";
    let after_source = "fn main() {\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let block = find_first(before_root, "block").unwrap();
    let mut cursor = block.walk();
    let before_nodes: Vec<Node> = block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement" || n.kind() == "let_declaration")
        .collect();
    assert_eq!(before_nodes.len(), 2);
    assert_ne!(before_nodes[0].kind(), before_nodes[1].kind());

    let after_nodes = block_statements(after_root);
    assert_eq!(after_nodes.len(), 1);

    let mut mapping = HumanMapping::default();
    let before_ids: std::collections::BTreeSet<usize> =
        before_nodes.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_nodes.iter().map(|n| n.id()).collect();
    let no_hashes = rustc_hash::FxHashMap::default();

    let outcome = action_commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        &no_hashes,
        &no_hashes,
        &Caches::default(),
        false,
    )
    .unwrap();

    match outcome {
        ActionOutcome::NeedsModal(modal) => match *modal {
            Modal::ConfirmMultiMapGroup { kinds, .. } => {
                assert!(kinds.len() > 1, "{:?}", kinds);
            }
            other => panic!("expected ConfirmMultiMapGroup, got {other:?}"),
        },
        ActionOutcome::Done(msg) => {
            panic!("expected a mixed-kinds confirmation, action completed instead: {msg}")
        }
    }
    assert!(
        mapping.groups.is_empty(),
        "nothing should commit until the modal is confirmed"
    );
}

#[test]
fn action_commit_multi_map_group_commits_directly_when_kinds_match() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);
    let mut mapping = HumanMapping::default();
    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();
    let no_hashes = rustc_hash::FxHashMap::default();

    let outcome = action_commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        &no_hashes,
        &no_hashes,
        &Caches::default(),
        true,
    )
    .unwrap();

    assert!(matches!(outcome, ActionOutcome::Done(_)));
    assert_eq!(mapping.groups.len(), 1);
    // No content hashes were supplied (`no_hashes`), so every lookup misses and the group
    // falls back to `MatchButNotIdentical` - see `multi_map_group_operation`.
    assert_eq!(
        mapping.groups[0].operation,
        HumanOperation::MatchButNotIdentical
    );
    assert!(mapping.groups[0].with_children);
}

#[test]
fn action_unmark_on_a_group_member_removes_the_whole_group() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);

    let mut mapping = HumanMapping::default();
    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();
    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        HumanOperation::Identical,
        false,
    )
    .unwrap();
    assert_eq!(mapping.groups.len(), 1);

    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);

    // Any one member - here, the leftover before-foo that landed on Delete rather than a
    // matched pair - should be enough to drop the whole group.
    let leftover = before_foos
        .iter()
        .find(|n| {
            caches.before_group.contains_key(&n.id()) && !caches.before_match.contains_key(&n.id())
        })
        .expect("exactly one before-foo should be the group's leftover");

    let msg = action_unmark(
        &mut mapping,
        Focus::Before,
        &before_flat,
        &after_flat,
        leftover.id(),
        after_foos[0].id(),
        before_root,
        after_root,
        &caches,
    )
    .unwrap();

    assert!(msg.contains("Removed multi-map group"), "{msg}");
    assert!(mapping.groups.is_empty());
}

#[test]
fn handle_key_x_toggles_multi_select_and_c_clears_both_sides() {
    let source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_foos = block_statements(before_root);

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    app.before.cursor_id = before_foos[0].id();
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let caches = Caches::default();
    let no_hashes = rustc_hash::FxHashMap::default();

    handle_key(
        &mut app,
        KeyCode::Char('x'),
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    assert_eq!(app.before_multi_select.len(), 1);
    assert!(app.before_multi_select.contains(&before_foos[0].id()));

    // Pressing x again on the same node toggles it back out.
    handle_key(
        &mut app,
        KeyCode::Char('x'),
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    assert!(app.before_multi_select.is_empty());

    app.before_multi_select.insert(before_foos[0].id());
    app.after_multi_select.insert(before_foos[1].id());
    handle_key(
        &mut app,
        KeyCode::Char('c'),
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );
    assert!(app.before_multi_select.is_empty());
    assert!(app.after_multi_select.is_empty());
}

#[test]
fn handle_key_m_with_a_pending_selection_commits_a_multi_map_group() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);

    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    app.before_multi_select = before_foos.iter().map(|n| n.id()).collect();
    app.after_multi_select = after_foos.iter().map(|n| n.id()).collect();

    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let no_hashes = rustc_hash::FxHashMap::default();

    handle_key(
        &mut app,
        KeyCode::Char('m'),
        &before_flat,
        &after_flat,
        before_root,
        after_root,
        &caches,
        before_source.as_bytes(),
        after_source.as_bytes(),
        &no_hashes,
        &no_hashes,
        &Code::from_string(before_source, &Language::Rust),
        &Code::from_string(after_source, &Language::Rust),
    );

    assert_eq!(app.mapping.groups.len(), 1, "{:?}", app.mapping.groups);
    assert!(app.mapping.entries.is_empty(), "{:?}", app.mapping.entries);
    assert!(app.dirty);
    assert!(
        app.before_multi_select.is_empty() && app.after_multi_select.is_empty(),
        "the selection should be cleared once committed"
    );
    // `m`, not `M`: the committed group should not require subtree closure.
    assert!(!app.mapping.groups[0].with_children);
}

#[test]
fn render_panel_marks_a_group_matched_node_and_a_pending_selection_distinctly() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_foos = block_statements(before_root);
    let after_foos = block_statements(after_root);

    let mut mapping = HumanMapping::default();
    let before_ids: std::collections::BTreeSet<usize> =
        before_foos.iter().map(|n| n.id()).collect();
    let after_ids: std::collections::BTreeSet<usize> = after_foos.iter().map(|n| n.id()).collect();
    commit_multi_map_group(
        &mut mapping,
        before_root,
        after_root,
        &before_ids,
        &after_ids,
        HumanOperation::Identical,
        false,
    )
    .unwrap();

    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
    let flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(before_root.id());

    // Mark one before-foo (not part of any group) as a pending multi-map selection, to prove
    // it renders distinctly from the already-committed group members.
    let mut pending = std::collections::BTreeSet::new();
    let plain_node = find_first(before_root, "function_item").unwrap();
    pending.insert(plain_node.id());

    let backend = ratatui::backend::TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 60, 20);

    terminal
        .draw(|f| {
            render_panel(
                f,
                area,
                "Before",
                &flat,
                &mut panel,
                &caches,
                Side::Before,
                before_source.as_bytes(),
                true,
                None,
                false,
                0,
                &pending,
            );
        })
        .unwrap();

    // `render_panel` draws inside a bordered `Block`, so row 0 and column 0 of the buffer are
    // the border itself - list content starts at row 1, column 1.
    let content = terminal.backend().buffer().content();
    let plain_row_idx = flat
        .iter()
        .position(|(n, _)| n.id() == plain_node.id())
        .unwrap()
        + 1;
    let group_row_idx = flat
        .iter()
        .position(|(n, _)| n.id() == before_foos[0].id())
        .unwrap()
        + 1;

    let plain_cell = &content[plain_row_idx * 60 + 1];
    assert_eq!(
        plain_cell.fg,
        Color::Magenta,
        "a pending multi-map selection should render in a distinct color"
    );

    let group_row_text: String = (0..60)
        .map(|col| content[group_row_idx * 60 + col].symbol())
        .collect();
    assert!(
        group_row_text.contains('g'),
        "a group-derived match should carry the 'g' marker: {group_row_text:?}"
    );
}

/// A tab reaching a ratatui cell is what left the `t` view's characters on screen after the modal
/// closed (see `display_safe_char`). `go-lazygit-switch-to-strings` is Go, so it is tab-indented,
/// which makes it the fixture that actually reproduced it.
#[test]
fn text_view_renders_no_literal_tabs_for_a_tab_indented_fixture() {
    let dir = diffs_root()
        .join("small")
        .join("go-lazygit-switch-to-strings");
    let source = std::fs::read_to_string(dir.join("before.go.test")).unwrap();
    assert!(
        source.contains('\t'),
        "this test is pointless unless the fixture is tab-indented"
    );

    let state = TextPaintState::default();
    let lines = render_paint_side(&source, &[], &state, 0, 100, 10_000);

    for line in &lines {
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            !rendered.contains('\t'),
            "a raw tab reached the buffer: {rendered:?}"
        );
    }
}

/// The tab replacement has to be one character wide, not an expansion to the next tab stop:
/// `render_paint_side` maps a paint cursor's column straight onto the line's byte offsets, so a
/// widened tab would paint the wrong bytes.
#[test]
fn text_view_keeps_one_screen_column_per_source_character() {
    let source = "\tif x {\n\t\treturn \"y\"\n\t}\n";
    let state = TextPaintState::default();
    let lines = render_paint_side(source, &[], &state, 0, 100, 10_000);

    for (row, expected) in source.split('\n').enumerate() {
        // The first span is the line-number gutter, which has no source counterpart.
        let rendered: String = lines[row]
            .spans
            .iter()
            .skip(1)
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(
            rendered.chars().count(),
            expected.chars().count(),
            "row {row} changed width: {rendered:?} vs {expected:?}"
        );
    }
}

#[test]
fn display_safe_str_replaces_every_tab_and_leaves_everything_else() {
    assert_eq!(display_safe_str("\ta\tb"), " a b");
    assert_eq!(display_safe_str("no tabs here"), "no tabs here");
}

/// Promoting or rejecting one sample rewrites the whole sample.csv, so any column the reader drops
/// is erased for every other row at the same time. `size_bucket` was exactly that column between
/// the stratified draw landing and this test.
#[test]
fn sample_csv_round_trip_preserves_the_size_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.csv");
    std::fs::write(
        &path,
        "language,repository,commit,path,promoted_to,dataset,status,comment,size_bucket\n\
         Go,r,abc,a.go,,stratified,SAMPLED,,100-300\n\
         Rust,r2,def,b.rs,,small,PROMOTED,a note,\n",
    )
    .unwrap();

    let rows = read_sample_csv_rows(&path).unwrap();
    assert_eq!(rows[0].size_bucket, "100-300");
    assert_eq!(rows[1].size_bucket, "", "a legacy row simply has no bucket");

    write_sample_csv_rows(&path, &rows).unwrap();
    let again = read_sample_csv_rows(&path).unwrap();
    assert_eq!(again[0].size_bucket, "100-300");
    assert_eq!(again[1].size_bucket, "");
    assert_eq!(again[1].comment, "a note", "the columns must not shift");
}

/// The same guarantee against the real corpus file, so a future column added to sample.csv by
/// `sample_test_diffs` but not to this tool's reader/writer fails here rather than silently
/// deleting itself the next time someone presses `s` in the `O` picker.
#[test]
fn round_tripping_the_real_sample_csv_loses_nothing() {
    let rows = read_sample_csv_rows(&sample_csv_path()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    write_sample_csv_rows(&path, &rows).unwrap();
    let again = read_sample_csv_rows(&path).unwrap();

    assert_eq!(again.len(), rows.len());
    let buckets = |rows: &[SampleCsvRow]| -> Vec<(String, String)> {
        rows.iter()
            .map(|r| (r.commit.clone(), r.size_bucket.clone()))
            .collect()
    };
    assert_eq!(buckets(&again), buckets(&rows));
}

#[test]
fn codediff_text_entries_keeps_the_pairing_that_the_span_view_drops() {
    let before = Code::from_string("fn main() {\n    foo();\n}\n", &Language::Rust);
    let after = Code::from_string("fn main() {\n    bar();\n}\n", &Language::Rust);

    let entries = codediff_text_entries(&before, &after).expect("this pair pairs up cleanly");
    assert!(!entries.is_empty(), "an edited pair should produce entries");

    for entry in &entries {
        match entry.operation {
            HumanTextOperation::Match => {
                assert!(
                    !entry.before.is_empty() && !entry.after.is_empty(),
                    "a match is a decision about a pair, so it needs both sides: {entry:?}"
                );
            }
            HumanTextOperation::Delete => {
                assert!(!entry.before.is_empty() && entry.after.is_empty());
            }
            HumanTextOperation::Insert => {
                assert!(entry.before.is_empty() && !entry.after.is_empty());
            }
        }
    }
}

/// The point of the seed: what it writes must *be* codediff's rendering, so the starting point
/// shows zero disagreement and every remaining edit is a correction the human chose to make.
#[test]
fn seeding_a_painting_reproduces_codediffs_own_spans_on_both_sides() {
    let before_src = "fn main() {\n    foo();\n}\n";
    let after_src = "fn main() {\n    bar();\n}\n";
    let before = Code::from_string(before_src, &Language::Rust);
    let after = Code::from_string(after_src, &Language::Rust);
    let tree = parse_rust(before_src);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );

    action_paint_seed_from_codediff(&mut app, &before, &after);
    assert!(app.dirty, "seeding is an unsaved change to the mapping");

    let algo = codediff_text_spans(&before, &after);
    for side in [0usize, 1usize] {
        let mut painted: Vec<_> = painted_spans(
            &app.mapping,
            &app.text_solution,
            side,
            before_src,
            after_src,
        );
        let mut expected = algo[side].clone();
        painted.sort_by_key(|(span, _)| (span.start_row, span.start_column));
        expected.sort_by_key(|(span, _)| (span.start_row, span.start_column));
        assert_eq!(
            painted, expected,
            "side {side} should read exactly as codediff renders it"
        );
    }
}

#[test]
fn seeding_refuses_to_overwrite_a_painting_that_already_has_ranges() {
    let before_src = "fn main() {\n    foo();\n}\n";
    let after_src = "fn main() {\n    bar();\n}\n";
    let before = Code::from_string(before_src, &Language::Rust);
    let after = Code::from_string(after_src, &Language::Rust);
    let tree = parse_rust(before_src);
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );

    action_paint_seed_from_codediff(&mut app, &before, &after);
    // `HumanTextEntry` comes from the library and carries no `PartialEq`, so compare the rendered
    // shape rather than deriving one on a public type purely for this assertion.
    let seeded = format!("{:?}", solution_entries(&app.mapping, &app.text_solution));
    app.dirty = false;

    action_paint_seed_from_codediff(&mut app, &before, &after);

    assert_eq!(
        format!("{:?}", solution_entries(&app.mapping, &app.text_solution)),
        seeded,
        "a second P must leave hand-corrected work exactly as it was"
    );
    assert!(!app.dirty, "a refused seed is not a change");
    assert!(
        app.status
            .as_deref()
            .is_some_and(|s| s.contains("already has painted ranges")),
        "the refusal should say why: {:?}",
        app.status
    );
}

/// The overlap guard, against a fixture that actually trips it. codediff's own rendering produces
/// overlapping ranges on 22 of the 513 corpus fixtures, so this is a real shape, not a contrived
/// one - and a painting cannot represent it, because the renderer resolves an overlap by highest
/// verdict while the scorer resolves it by list order.
#[test]
fn seeding_refuses_a_pair_whose_codediff_ranges_overlap() {
    let pairs = codediff::test::helper::handmade_test_code_pairs().unwrap();
    let (before, after) = pairs
        .get("xml-odoo-odoo-add-two-attributes")
        .expect("fixture should exist");

    let overlapping = codediff_text_spans(before, after)
        .iter()
        .any(|side| spans_overlap(side));
    assert!(
        overlapping,
        "this test is pointless unless the fixture still has overlapping codediff ranges"
    );

    let error = codediff_text_entries(before, after)
        .expect_err("an overlapping pair must not produce a painting");
    assert!(
        error.contains("overlap"),
        "the reason should say why: {error}"
    );
}

#[test]
fn spans_overlap_detects_a_shared_byte_and_allows_touching_ranges() {
    let span = |sc, ec| {
        (
            HumanTextSpan {
                start_row: 0,
                start_column: sc,
                end_row: 0,
                end_column: ec,
            },
            HumanTextVerdict::Delete,
        )
    };
    assert!(spans_overlap(&[span(0, 8), span(4, 12)]), "they share 4..8");
    assert!(
        !spans_overlap(&[span(0, 4), span(4, 8)]),
        "abutting ranges share no byte - end is exclusive"
    );
    assert!(!spans_overlap(&[span(0, 4)]));
    assert!(!spans_overlap(&[]));
}

/// The text of every screen row a call produced, gutter included - what the reader actually sees.
fn painted_screen_rows(lines: &[ratatui::text::Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

#[test]
fn a_long_line_wraps_across_screen_rows_with_a_blank_continuation_gutter() {
    let source = format!("short\nx{}\ntail\n", "ABCDEFGHIJ".repeat(9));
    let state = TextPaintState::default();

    // Width 40, gutter "  N " = 4 columns, so 36 columns of content per screen row.
    let rows = painted_screen_rows(&render_paint_side(&source, &[], &state, 0, 12, 40));

    assert_eq!(rows[0], "  1 short");
    assert_eq!(rows[1].chars().count(), 40, "a wrapped row fills the width");
    assert!(rows[1].starts_with("  2 x"));
    assert!(
        rows[2].starts_with("    ") && !rows[2].starts_with("  3"),
        "a continuation row carries a blank gutter, not a repeated line number: {:?}",
        rows[2]
    );
    assert!(
        rows.iter().any(|r| r.starts_with("  3 tail")),
        "the next source row still gets its own number: {rows:?}"
    );

    // Nothing is lost or duplicated: the wrapped rows reassemble into the original line.
    let rejoined: String = rows[1..4]
        .iter()
        .map(|r| r[4..].to_string())
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(rejoined, format!("x{}", "ABCDEFGHIJ".repeat(9)));
}

#[test]
fn wrapping_never_emits_more_screen_rows_than_the_viewport_holds() {
    let source = format!(
        "{}\n{}\n{}\n",
        "a".repeat(200),
        "b".repeat(200),
        "c".repeat(200)
    );
    let state = TextPaintState::default();

    let rows = render_paint_side(&source, &[], &state, 0, 5, 40);
    assert_eq!(rows.len(), 5, "a 5-row viewport must render exactly 5 rows");
}

/// `scroll_into_view` keeps the cursor inside a *source*-row window, which stops meaning the same
/// thing once rows wrap: one very long line can fill the viewport by itself. The renderer walks
/// its start row forward so the cursor's row is always on screen.
#[test]
fn the_cursor_row_stays_visible_when_the_rows_above_it_wrap() {
    let source = format!("{}\n{}\nCURSORROW\n", "a".repeat(400), "b".repeat(400));
    let mut state = TextPaintState::default();
    state.cursor[0] = (2, 0);
    state.scroll[0] = 0;

    // Rows 0 and 1 wrap to 12 screen rows each at this width, so a naive render from row 0 would
    // never reach row 2 inside a 10-row viewport.
    let rows = painted_screen_rows(&render_paint_side(&source, &[], &state, 0, 10, 40));
    assert!(
        rows.iter().any(|r| r.contains("CURSORROW")),
        "the cursor's row should have been scrolled to: {rows:?}"
    );
}

#[test]
fn a_width_with_no_room_beside_the_gutter_does_not_wrap_or_hang() {
    let source = format!("{}\n", "z".repeat(80));
    let state = TextPaintState::default();

    let rows = painted_screen_rows(&render_paint_side(&source, &[], &state, 0, 5, 0));
    // Two source rows: the 80 z's, and the empty one left by the trailing newline.
    assert_eq!(
        rows.len(),
        2,
        "no room to wrap into means no extra rows, not a loop"
    );
    assert!(
        rows[0].ends_with(&"z".repeat(80)),
        "the row is rendered whole rather than wrapped: {:?}",
        rows[0]
    );
}

#[test]
fn a_painted_span_keeps_its_style_across_a_wrap_boundary() {
    let source = format!("{}\n", "q".repeat(80));
    // Focus the other side: `PaintClass::Cursor` outranks `Painted`, so a cursor resting on this
    // row would style its first character as the cursor and mask what this test checks.
    let state = TextPaintState {
        side: 1,
        ..TextPaintState::default()
    };
    let spans = vec![(
        HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 80,
        },
        HumanTextVerdict::Delete,
    )];

    let lines = render_paint_side(&source, &spans, &state, 0, 5, 40);
    assert!(
        lines.len() > 1,
        "80 columns should not fit in one 36-wide row"
    );
    for (i, line) in lines.iter().enumerate() {
        // Span 0 is the gutter; everything after it is the painted content of that screen row.
        for span in line.spans.iter().skip(1) {
            assert_eq!(
                span.style,
                paint_class_style(PaintClass::Painted(HumanTextVerdict::Delete)),
                "row {i} lost its paint across the wrap: {:?}",
                span.content
            );
        }
    }
}

/// A wrapped row must respect terminal *cells*, not characters: a CJK ideograph is one character
/// and two cells, so a character-counted wrap overflows the panel by one column per ideograph.
#[test]
fn wrapping_measures_terminal_cells_not_characters() {
    let source = format!("{}\n", "漢".repeat(20));
    let state = TextPaintState::default();

    // Width 20, gutter "  N " = 4 columns, so 16 cells of content: eight ideographs per row.
    let rows: Vec<String> = render_paint_side(&source, &[], &state, 0, 12, 20)
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    let cells = |s: &str| -> usize {
        use unicode_width::UnicodeWidthChar;
        s.chars().map(|c| c.width().unwrap_or(0)).sum()
    };
    for (i, row) in rows.iter().enumerate() {
        assert!(
            cells(row) <= 20,
            "row {i} is {} cells wide, past the 20-column panel: {row:?}",
            cells(row)
        );
    }
    assert!(
        rows.len() >= 3,
        "20 ideographs need three rows of eight: {rows:?}"
    );
    let rejoined: String = rows.iter().map(|r| r[4..].to_string()).collect();
    assert_eq!(
        rejoined,
        "漢".repeat(20),
        "wrapping must not lose or split a character"
    );
}

/// Two painted ranges claiming the same byte are not representable: the renderer resolves an
/// overlap by highest verdict and the scorer by list order, so such a painting looks like one
/// thing and grades as another. Refused at the keystroke, while the selection is still on screen.
#[test]
fn painting_over_an_already_painted_range_is_refused() {
    let before_src = "aaaabbbbcccc\n";
    let after_src = "aaaabbbbdddd\n";
    let tree = parse_rust("fn main() {}\n");
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let mut state = TextPaintState {
        side: 0,
        ..TextPaintState::default()
    };

    // Paint cols 0..8 on the Before side.
    state.cursor[0] = (0, 0);
    state.anchor[0] = Some((0, 7));
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );
    assert_eq!(
        solution_entries(&app.mapping, &app.text_solution).len(),
        1,
        "the first paint should land: {:?}",
        app.status
    );

    // Now paint cols 4..12, which shares bytes 4..8 with it.
    state.cursor[0] = (0, 4);
    state.anchor[0] = Some((0, 11));
    action_paint_one_sided(
        &mut app,
        &mut state,
        HumanTextOperation::Delete,
        before_src,
        after_src,
    );

    assert_eq!(
        solution_entries(&app.mapping, &app.text_solution).len(),
        1,
        "the overlapping paint must be refused, not appended"
    );
    let status = app.status.clone().unwrap_or_default();
    assert!(
        status.contains("already has a painted"),
        "the refusal should say what it clashes with: {status:?}"
    );
}

/// Two ranges meeting at a line boundary share only the newline, which `label_bytes` never
/// labels. They disagree about nothing, and refusing them would make ordinary line-by-line
/// painting impossible.
#[test]
fn painting_two_ranges_that_meet_at_a_newline_is_allowed() {
    let before_src = "first line\nsecond line\n";
    let after_src = "first line\nchanged line\n";
    let tree = parse_rust("fn main() {}\n");
    let root = tree.root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let mut state = TextPaintState {
        side: 0,
        ..TextPaintState::default()
    };

    for (row, last_col) in [(0usize, 9usize), (1usize, 10usize)] {
        state.cursor[0] = (row, 0);
        state.anchor[0] = Some((row, last_col));
        action_paint_one_sided(
            &mut app,
            &mut state,
            HumanTextOperation::Delete,
            before_src,
            after_src,
        );
    }

    assert_eq!(
        solution_entries(&app.mapping, &app.text_solution).len(),
        2,
        "both rows should paint: {:?}",
        app.status
    );
}

/// `!` clears all three grounds truth at once. Clearing the tree mapping while leaving paintings
/// behind would leave a fixture asserting things about a mapping that no longer exists.
#[test]
fn resetting_a_case_clears_the_mapping_the_groups_and_every_painting() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let mut mapping = HumanMapping::default();
    mapping.entries.push(HumanMappingEntry {
        operation: HumanOperation::Delete,
        before_path: Some(vec!["source_file:1".to_string()]),
        after_path: None,
    });
    solution_entries_mut(&mut mapping, "Minimal").push(HumanTextEntry {
        operation: HumanTextOperation::Delete,
        before: vec![HumanTextSpan {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 2,
        }],
        after: Vec::new(),
    });
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        mapping,
    );
    app.dirty = false;

    let status = action_reset_case(&mut app);

    assert!(
        app.mapping.entries.is_empty(),
        "tree mapping should be gone"
    );
    assert!(app.mapping.groups.is_empty(), "groups should be gone");
    assert!(
        app.mapping.text_mappings.is_empty(),
        "every painting should be gone"
    );
    assert!(app.dirty, "a reset is an unsaved change");
    assert!(
        app.tree_text_spans.is_none(),
        "spans derived from the discarded mapping must not survive it"
    );
    assert!(status.contains("Reset"), "status should say so: {status}");
}

/// The confirmation only goes through on the explicit key. Enter confirms everywhere else in this
/// tool, so it is exactly the key most likely to be pressed by reflex on a modal that cannot be
/// undone.
#[test]
fn resetting_a_case_needs_the_explicit_key_and_enter_will_not_do() {
    let source = "fn main() {}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(root, &Default::default(), None));

    for (key, should_clear) in [
        (KeyCode::Char('y'), true),
        (KeyCode::Enter, false),
        (KeyCode::Esc, false),
        (KeyCode::Char('n'), false),
    ] {
        let mut mapping = HumanMapping::default();
        mapping.entries.push(HumanMappingEntry {
            operation: HumanOperation::Delete,
            before_path: Some(vec!["source_file:1".to_string()]),
            after_path: None,
        });
        let mut app = App::new(
            "test".to_string(),
            CaseOrigin::Diffs,
            root.id(),
            root.id(),
            mapping,
        );
        app.modal = Some(Modal::ConfirmResetCase {
            entries: 1,
            groups: 0,
            paintings: 0,
        });
        let caches = rebuild_caches(&app.mapping.entries, root, root);

        handle_modal_key(
            &mut app,
            key,
            &flat,
            &flat,
            root,
            root,
            &caches,
            source.as_bytes(),
            source.as_bytes(),
            &Code::from_string(source, &Language::Rust),
            &Code::from_string(source, &Language::Rust),
        );

        assert_eq!(
            app.mapping.entries.is_empty(),
            should_clear,
            "{key:?} should {} the mapping",
            if should_clear { "clear" } else { "leave" }
        );
    }
}
