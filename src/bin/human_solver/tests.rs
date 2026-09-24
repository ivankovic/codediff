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
//! Every test for the `human_solver` binary. One file because the fixtures (`test_app`,
//! `press_on_case`, the picker view builders) are shared across modules.

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
    let contents = stub_test_contents("rust-add-if", None, false);
    assert!(!contents.contains("//\n") && !contents.contains("    // "));
    assert!(contents.contains(
        "fn mapping() -> Result<()> {\n    test::helper::human_mapping::assert_matches_human_mapping(\"rust-add-if\")\n}\n"
    ));
}

#[test]
fn stub_test_contents_has_no_comment_block_when_comment_is_empty_or_whitespace() {
    assert_eq!(
        stub_test_contents("rust-add-if", Some(""), false),
        stub_test_contents("rust-add-if", None, false)
    );
    assert_eq!(
        stub_test_contents("rust-add-if", Some("   \n  "), false),
        stub_test_contents("rust-add-if", None, false)
    );
}

#[test]
fn stub_test_contents_includes_a_wrapped_comment_block_right_before_the_assert() {
    let contents = stub_test_contents("rust-add-if", Some("A short note."), false);
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
    // Strip each line's "    // " prefix so it is not counted as a word.
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
    // Enter/Esc can move the cursor, save, reject or close: all can change `FrameState`.
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
    // These mutate collapse state or the mapping; `s`/`R`/`o`/`O`/`C` stay on the full-rebuild
    // path deliberately (see `is_navigation_or_display_key`).
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

    let mut subtree_ids = Vec::new();
    collect_subtree_ids(stmt_a, &mut subtree_ids);
    mark_subtree_matched(stmt_a, &mut caches);

    let after_marking = count_unmarked(&flat, &caches, status_before);
    assert_eq!(before_unmarked - after_marking, subtree_ids.len());
}

#[test]
fn render_panel_only_scans_the_visible_window_not_the_whole_flat_list() {
    // Only a few rows fit the terminal: `render_panel` must not touch rows outside that window,
    // and the header count must be the caller's `total_unmarked`.
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
    let backend = ratatui::backend::TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 40, 20);

    // Deliberately wrong: a full scan would show the real count instead.
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
                &[],
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
    // No "dataset" key: samples materialized before provenance tracking look like this.
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
    // Unreachable through `load_git_commit_file`, but must degrade to "no prefix", not panic.
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

    let found =
        advance_to_next_search_match(&mut panel, &flat, source.as_bytes(), "alpha").unwrap();
    assert_eq!(found.utf8_text(source.as_bytes()).unwrap(), "alpha");
}

#[test]
fn advance_to_next_search_match_only_matches_leaf_nodes_not_a_containers_concatenated_text() {
    // The `block`'s text contains both names, but only leaves may match.
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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

    assert!(raw_before_after(dir.path()).is_none());
}

#[test]
fn sample_diff_line_count_counts_only_changed_lines_not_context_or_headers() {
    let dir = tempfile::tempdir().unwrap();
    // `samples_root()` is fixed to this crate's samples, so this drives the two calls
    // `sample_diff_line_count` makes against a temp directory instead.
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
    // Skips when no sample is checked in, as `list_dir_names` treats a missing directory.
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

fn column_view(column: DiffColumn) -> DiffPickerView {
    DiffPickerView {
        column,
        ..DiffPickerView::default()
    }
}

fn sort_view(column: DiffColumn) -> DiffPickerView {
    DiffPickerView {
        sort: DiffSort::default().toggled(column),
        ..DiffPickerView::default()
    }
}

fn dataset_view(dataset: Option<&'static str>) -> DiffPickerView {
    DiffPickerView {
        filters: DiffFilters {
            dataset,
            ..DiffFilters::default()
        },
        ..DiffPickerView::default()
    }
}

fn name_view(needle: &str) -> DiffPickerView {
    DiffPickerView {
        filters: DiffFilters {
            name: Some(needle.to_string()),
            ..DiffFilters::default()
        },
        ..DiffPickerView::default()
    }
}

fn flag_view(column: DiffColumn, filter: FlagFilter) -> DiffPickerView {
    let mut view = DiffPickerView::default();
    *view
        .filters
        .flag_mut(column)
        .expect("column has a flag filter") = filter;
    view
}

/// A case the scan never reached survives either direction of a filter (see `FlagFilter::keeps`).
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

/// Filters on two columns combine as an AND.
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

/// `s` sorts by a column and flips direction on a second press. The name breaks ties, so an
/// unscanned column still sorts alphabetically.
#[test]
fn visible_diff_options_sorts_by_the_selected_column_with_a_name_tiebreak() {
    let options = vec![
        ("charlie".to_string(), "handmade"),
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
        ("delta".to_string(), "handmade"),
    ];
    // No `delta`: an unscanned case sorts last ascending, first descending.
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

/// Unknown sizes sort last ascending; `f` narrows to non-empty diffs or to empty (broken) ones.
#[test]
fn visible_diff_options_sorts_and_filters_by_diff_size() {
    let options = vec![
        ("charlie".to_string(), "handmade"),
        ("alpha".to_string(), "handmade"),
        ("bravo".to_string(), "handmade"),
        ("delta".to_string(), "handmade"),
    ];
    let sizes = std::collections::HashMap::from([
        ("charlie".to_string(), 40),
        ("alpha".to_string(), 3),
        ("bravo".to_string(), 0),
    ]);
    let data = DiffPickerData {
        sizes: Some(&sizes),
        ..DiffPickerData::default()
    };

    let mut view = sort_view(DiffColumn::Size);
    assert_eq!(
        visible_diff_options(&options, &view, data),
        vec!["bravo", "alpha", "charlie", "delta"],
        "smallest diff first, unknown last"
    );
    view.sort = view.sort.toggled(DiffColumn::Size);
    assert_eq!(
        visible_diff_options(&options, &view, data),
        vec!["delta", "charlie", "alpha", "bravo"],
        "largest first when flipped, unknown to the front"
    );

    let mut filtered = DiffPickerView::default();
    filtered.filters.size = FlagFilter::Yes;
    assert_eq!(
        visible_diff_options(&options, &filtered, data),
        vec!["alpha", "charlie", "delta"],
        "'has changed lines' drops the empty diff and keeps the unknown"
    );
    filtered.filters.size = FlagFilter::No;
    assert_eq!(
        visible_diff_options(&options, &filtered, data),
        vec!["bravo", "delta"],
        "'empty diffs only' keeps the empty diff and the unknown"
    );
    assert_eq!(
        filtered.filters.labels(),
        vec!["empty diffs only"],
        "the title bar names the filter"
    );
}

/// The `o` and `O` pickers measure `Size` the same way.
#[test]
fn changed_line_count_reads_a_case_directory_like_a_sample() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("before.rs.test"),
        "fn a() {}\nfn b() {}\nfn c() {}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("after.rs.test"),
        "fn a() {}\nfn B() {}\nfn c() {}\nfn d() {}\n",
    )
    .unwrap();
    assert_eq!(
        changed_line_count(dir.path()),
        Some(3),
        "one line replaced (2) and one added (1)"
    );
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        changed_line_count(empty.path()),
        None,
        "no pair to read is unknown, not empty"
    );
}

/// The `Name` filter needs no corpus scan.
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

/// The painting being edited is at the top, above unused suggestions.
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

/// Presses one key on the main view of a case with `origin`. Goes through `handle_key` so an
/// unwired key arm fails the test, which calling the action directly cannot catch.
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

/// A handmade fixture has no sample.csv row, but `e` still opens a prompt (for description.md).
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

/// A `C` git-commit case has nowhere to save a comment, and says so.
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

/// A promoted fixture's note lives only in its `description.md`: promotion moves it (see
/// `update_sample_csv_at`), because two copies drift. A rejected or untriaged row keeps its
/// comment, since it has no fixture directory and a rejection reason is the only record.
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

/// Opens the solution picker over a fixture holding `paintings` and presses `keys` through
/// `handle_modal_key`, so an unwired key fails the test.
fn press_in_solution_picker(paintings: &[&str], keys: &[KeyCode]) -> App {
    press_in_solution_picker_editing(paintings, None, keys)
}

/// As above, but editing `editing`, so the deleted painting need not be the current one.
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
            Some(root),
            Some(root),
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

/// There is no undo for a painting, so `D` needs two presses.
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

/// Moving the cursor between the two presses must not retarget the second `D`.
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

/// Deleting a painting you are not editing must not move you.
#[test]
fn deleting_another_painting_leaves_you_editing_your_own() {
    // Three, not two: `starting_solution` returns the first survivor, which with two would be
    // the edited one anyway and hide an unguarded reset.
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

/// `D` on an unused suggestion must not look like it worked.
#[test]
fn d_on_an_unused_suggestion_deletes_nothing() {
    // "Full" exists; j lands on an unused suggestion.
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

/// Renaming copies the ranges, so a fixture can hold more than one painting.
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

    action_save_solution_as(&mut app, "Full", true, before_src, after_src);

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

/// Branching `Minimal` to `Full` widens every wholly-changed line to its indentation: invariant 4
/// requires that of `Full` and invariant 6 forbids it in `Minimal`.
#[test]
fn branching_minimal_to_full_widens_a_wholly_changed_line_to_its_indentation() {
    let source = indented_block();
    let (mut app, _) =
        press_in_text_view(source, source, sweep_over_the_block(), KeyCode::Char('d'));
    let minimal = painted_entries(&app).remove(0).1;
    assert_eq!(minimal[0].start_column, 4, "Minimal starts past the indent");

    action_save_solution_as(&mut app, "Full", true, source, source);

    let full = painted_entries(&app).remove(0).1;
    assert_eq!(
        full,
        vec![
            HumanTextSpan {
                start_row: 1,
                start_column: 0,
                end_row: 1,
                end_column: 14,
            },
            HumanTextSpan {
                start_row: 3,
                start_column: 0,
                end_row: 3,
                end_column: 18,
            },
            HumanTextSpan {
                start_row: 4,
                start_column: 0,
                end_row: 4,
                end_column: 15,
            },
        ],
        "each row now starts at column 0"
    );
    assert_eq!(
        solution_entries(&app.mapping, "Minimal")[0].before[0].start_column,
        4,
        "and the painting it was branched from is untouched"
    );
    assert!(
        app.status
            .as_deref()
            .unwrap_or("")
            .contains("3 lines widened"),
        "got {:?}",
        app.status
    );
}

/// One fixture with both paintings satisfies invariant 6 on `Minimal` and 4 on `Full`.
#[test]
fn the_two_branched_paintings_satisfy_their_own_preset_rules() {
    let source = indented_block();
    let (mut app, _) =
        press_in_text_view(source, source, sweep_over_the_block(), KeyCode::Char('d'));
    action_save_solution_as(&mut app, "Full", true, source, source);

    let code = Code::from_string(source, &Language::Rust);
    let violations = human_mapping::invariants::ground_truth_invariant_violations_for(
        &app.mapping,
        &code,
        &code,
    )
    .expect("the invariant checker should read this mapping");
    assert!(
        violations.is_empty(),
        "neither preset should contradict itself: {violations:?}"
    );
}

/// A partly changed line's indentation stays unpainted under both presets.
#[test]
fn branching_leaves_a_partly_changed_line_alone() {
    let source = indented_block();
    // `a = 1` out of `    let a = 1;`.
    let state = TextPaintState {
        anchor: [Some((1, 8)), None],
        cursor: [(1, 12), (0, 0)],
        ..Default::default()
    };
    let (mut app, _) = press_in_text_view(source, source, state, KeyCode::Char('d'));

    action_save_solution_as(&mut app, "Full", true, source, source);

    assert_eq!(
        painted_entries(&app).remove(0).1[0].start_column,
        8,
        "`let` and the indentation before it both survive, so nothing widens"
    );
}

/// A `Match` resolves to Move/Update, which invariant 4 excludes: a surviving line's indentation
/// may be untouched.
#[test]
fn branching_leaves_a_matched_line_alone() {
    let source = indented_block();
    let mut app = test_app();
    solution_entries_mut(&mut app.mapping, "Minimal").push(HumanTextEntry {
        operation: HumanTextOperation::Match,
        before: vec![HumanTextSpan {
            start_row: 1,
            start_column: 4,
            end_row: 1,
            end_column: 14,
        }],
        after: vec![HumanTextSpan {
            start_row: 1,
            start_column: 4,
            end_row: 1,
            end_column: 14,
        }],
    });

    action_save_solution_as(&mut app, "Full", true, source, source);

    let spans = painted_entries(&app).remove(0).1;
    assert_eq!(spans[0].start_column, 4, "a move keeps its own columns");
}

/// The rule belongs to the preset, so a free-form name widens nothing.
#[test]
fn branching_to_a_free_form_name_widens_nothing() {
    let source = indented_block();
    let (mut app, _) =
        press_in_text_view(source, source, sweep_over_the_block(), KeyCode::Char('d'));

    action_save_solution_as(&mut app, "Only one solution", true, source, source);

    assert_eq!(
        painted_entries(&app).remove(0).1[0].start_column,
        4,
        "nothing was asked for, so nothing changed"
    );
    assert!(
        !app.status.as_deref().unwrap_or("").contains("widened"),
        "and the status must not claim otherwise: {:?}",
        app.status
    );
}

/// Enter copies the ranges and `e` starts empty; both must be reachable.
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

    action_save_solution_as(&mut app, "Full", false, before_src, after_src);

    assert_eq!(app.mapping.text_mappings.len(), 2);
    assert_eq!(solution_entries(&app.mapping, "Minimal").len(), 1);
    assert!(solution_entries(&app.mapping, "Full").is_empty());
}

/// An existing name is never written: merging duplicates ranges, replacing loses work.
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

    action_save_solution_as(&mut app, "Full", true, "gone\n", "\n");

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

/// codediff's side comes from `TextDiff`, the projection the TUI draws, not a second reading of
/// its node mapping.
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
    assert!(
        before_spans.iter().all(|(span, _)| span.start_row > 0),
        "row 0 is unchanged and should carry no span: {before_spans:?}"
    );
}

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

/// The shared overlay palette, so a range looks as it does in the `codediff` TUI.
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

/// A middle row is covered up to its last real character, never its trailing whitespace or
/// newline.
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

/// A blank middle row counts as covered at column 0, so `render_paint_side`'s one-space fallback
/// draws it.
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

/// Unset means Dracula, which keeps render tests independent of the machine's `.codediff.toml`.
#[test]
fn the_palette_falls_back_to_the_default_theme_when_none_was_installed() {
    assert_eq!(
        overlay_palette().move_bg,
        OverlayTheme::Dracula.palette().move_bg
    );
}

#[test]
fn a_selection_uses_the_cross_panel_highlight_colour() {
    assert_eq!(
        paint_class_style(PaintClass::Selected).bg,
        Some(OverlayTheme::default().palette().cross_highlight_bg)
    );
}

/// Several banked ranges per side commit as ONE match, not one per range.
#[test]
fn x_banks_ranges_so_m_can_commit_an_n_to_m_match() {
    let (before_src, after_src) = ("foo\nfoo\nfoo\n", "bar\nbar\n");
    let mut app = test_app();
    let mut state = TextPaintState::default();

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

/// The live selection commits with the bank, so a forgotten final `x` loses nothing.
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

/// Refused at the keystroke, while the selection is still on screen, not at save time.
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

/// Through `handle_modal_key`, not the actions: an unwired key arm is what this pins.
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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

/// A 40-line pair differing in three places (insert, reword, delete), taller than the 20-row
/// viewport so a jump has somewhere to scroll.
fn navigable_pair() -> (String, String) {
    let mut before: Vec<String> = (0..40)
        .map(|row| format!("let line_{row} = {row};"))
        .collect();
    let mut after = before.clone();
    before.remove(30);
    after[20] = "let line_20 = 999;".to_string();
    after.insert(10, "let inserted = 0;".to_string());
    (
        format!("{}\n", before.join("\n")),
        format!("{}\n", after.join("\n")),
    )
}

/// One keystroke through `handle_modal_key` into a text view over a differing pair.
fn press_in_text_view(
    before_src: &str,
    after_src: &str,
    state: TextPaintState,
    code: KeyCode,
) -> (App, TextPaintState) {
    press_in_text_view_painting("Minimal", before_src, after_src, state, code)
}

/// [`press_in_text_view_painting`] with ranges painted first.
fn press_in_text_view_seeded(
    solution: &str,
    source: &str,
    state: TextPaintState,
    code: KeyCode,
    seed: impl Fn(&mut App),
) -> (App, TextPaintState) {
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
    app.text_solution = solution.to_string();
    seed(&mut app);
    app.modal = Some(Modal::TextView { state });

    handle_modal_key(
        &mut app,
        code,
        &flat,
        &flat,
        Some(root),
        Some(root),
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &Code::from_string(source, &Language::Rust),
        &Code::from_string(source, &Language::Rust),
    );

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("the text view should still be open, got {:?}", app.modal);
    };
    let state = state.clone();
    (app, state)
}

/// [`press_in_text_view`] editing a named painting, for rules keyed on the name.
fn press_in_text_view_painting(
    solution: &str,
    before_src: &str,
    after_src: &str,
    state: TextPaintState,
    code: KeyCode,
) -> (App, TextPaintState) {
    let tree = parse_rust(before_src);
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
    app.text_solution = solution.to_string();
    app.modal = Some(Modal::TextView { state });

    handle_modal_key(
        &mut app,
        code,
        &flat,
        &flat,
        Some(root),
        Some(root),
        &caches,
        before_src.as_bytes(),
        after_src.as_bytes(),
        &Code::from_string(before_src, &Language::Rust),
        &Code::from_string(after_src, &Language::Rust),
    );

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("the text view should still be open, got {:?}", app.modal);
    };
    let state = state.clone();
    (app, state)
}

/// One hunk per differing region; an unchanged pair has none, so "No differences" is reportable.
#[test]
fn text_diff_hunks_finds_one_entry_per_differing_region() {
    assert_eq!(text_diff_hunks("a\nb\nc\n", "a\nb\nc\n"), Some(Vec::new()));
    // A pure insertion: the before row is where the new line goes in front of.
    assert_eq!(text_diff_hunks("a\nc\n", "a\nb\nc\n"), Some(vec![(1, 1)]));
    assert_eq!(text_diff_hunks("a\nb\nc\n", "a\nc\n"), Some(vec![(1, 1)]));
    // A reworded line is unmatched on both sides, so it is a hunk like any other - the property
    // `plain_text_line_diff`'s `RangeMatch` lists would have made harder to read off.
    assert_eq!(
        text_diff_hunks("a\nOLD\nc\n", "a\nNEW\nc\n"),
        Some(vec![(1, 1)])
    );
    // Two regions, each keeping its own side's row: the second hunk's after row is one ahead,
    // since the first inserted a line.
    assert_eq!(
        text_diff_hunks("a\nb\nc\nd\n", "a\nX\nb\nc\nY\nd\n"),
        Some(vec![(1, 1), (3, 4)])
    );
}

/// `n` moves and scrolls *both* sides; they scroll independently, so moving one leaves the other
/// showing an unrelated part of the file.
#[test]
fn n_in_the_text_view_jumps_both_sides_to_the_next_hunk() {
    let (before_src, after_src) = navigable_pair();
    let (app, state) = press_in_text_view(
        &before_src,
        &after_src,
        TextPaintState::default(),
        KeyCode::Char('n'),
    );

    assert_eq!(state.cursor[0].0, 10, "the first hunk is the inserted line");
    assert_eq!(state.cursor[1].0, 10);
    assert!(
        app.status.as_deref().unwrap_or("").starts_with("Diff 1/3"),
        "got {:?}",
        app.status
    );

    let (_, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('n'));
    assert_eq!(state.cursor[0].0, 20, "the reworded line, before side");
    assert_eq!(state.cursor[1].0, 21, "one lower after the insertion");
    let (_, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('n'));
    assert_eq!(state.cursor[0].0, 30, "the deleted line");
    assert_eq!(state.cursor[1].0, 31);
    assert!(
        state.scroll[0] > 0,
        "the before panel scrolled to its cursor"
    );
    assert!(state.scroll[1] > 0, "and so did the unfocused after panel");
}

/// `p` wraps at the top like the tree view's `n`/`N`, so holding it keeps cycling.
#[test]
fn p_in_the_text_view_walks_back_and_wraps_to_the_last_hunk() {
    let (before_src, after_src) = navigable_pair();
    let mut state = TextPaintState::default();
    state.cursor[0] = (20, 0);

    let (app, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('p'));
    assert_eq!(state.cursor[0].0, 10, "back to the hunk above");
    assert!(
        app.status.as_deref().unwrap_or("").starts_with("Diff 1/3"),
        "got {:?}",
        app.status
    );

    let (app, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('p'));
    assert_eq!(state.cursor[0].0, 30, "wrapped around to the last hunk");
    assert!(
        app.status.as_deref().unwrap_or("").starts_with("Diff 3/3"),
        "got {:?}",
        app.status
    );
}

/// `n` from the After panel steps by after-side rows: the side being read.
#[test]
fn the_focused_side_decides_which_hunk_is_next() {
    let (before_src, after_src) = navigable_pair();
    let mut state = TextPaintState {
        side: 1,
        ..Default::default()
    };
    // Past the second hunk on the after side (row 21), but not past it on the before side (20).
    state.cursor[1] = (21, 0);

    let (app, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('n'));
    assert_eq!(
        state.cursor[1].0, 31,
        "the third hunk, not the second again"
    );
    assert!(
        app.status.as_deref().unwrap_or("").starts_with("Diff 3/3"),
        "got {:?}",
        app.status
    );
}

/// An identical pair has no hunks at all; `n` must say so rather than look broken.
#[test]
fn n_on_an_unchanged_pair_reports_no_differences() {
    let source = "let a = 1;\nlet b = 2;\n";
    let (app, state) = press_in_text_view(
        source,
        source,
        TextPaintState::default(),
        KeyCode::Char('n'),
    );
    assert_eq!(state.cursor[0], (0, 0), "and moves nothing");
    assert_eq!(
        app.status.as_deref(),
        Some("No differences between the two sides")
    );
}

/// `a` lines the unfocused panel up with the focused one: same line number, same screen row.
#[test]
fn a_in_the_text_view_aligns_the_other_side_on_the_same_line() {
    let (before_src, after_src) = navigable_pair();
    let mut state = TextPaintState::default();
    state.cursor[0] = (25, 0);
    state.scroll[0] = 15;

    state.cursor[1] = (0, 4);

    let (app, state) = press_in_text_view(&before_src, &after_src, state, KeyCode::Char('a'));
    assert_eq!(state.cursor[1].0, 25, "same line number on the other side");
    assert_eq!(
        state.cursor[1].1, 4,
        "and its own column, not the focused side's - a is about the line"
    );
    assert_eq!(
        state.scroll[1], 15,
        "and the same top row, so the two panels show the same numbers"
    );
    assert_eq!(state.side, 0, "focus does not move");
    assert!(
        app.status.as_deref().unwrap_or("").contains("line 26"),
        "1-based in the status, as the gutter shows it; got {:?}",
        app.status
    );
}

/// The other side can be shorter than the line asked for. Landing past its end would put the
/// cursor on a row no span can be read from, so it clamps - and says it clamped.
#[test]
fn a_clamps_to_the_other_sides_last_line() {
    let before_src = "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;\n";
    let after_src = "let a = 1;\n";
    let mut state = TextPaintState::default();
    state.cursor[0] = (3, 0);

    let (app, state) = press_in_text_view(before_src, after_src, state, KeyCode::Char('a'));
    assert_eq!(
        state.cursor[1].0,
        TextPaintState::row_count(after_src) - 1,
        "clamped to the last row that side actually has"
    );
    assert!(
        app.status.as_deref().unwrap_or("").contains("ends there"),
        "got {:?}",
        app.status
    );
}

/// An indented block with a blank middle line: the shape a multi-line delete is painted over.
fn indented_block() -> &'static str {
    "fn main() {\n    let a = 1;\n\n        let b = 2;\n    println!();\n}\n"
}

/// Selects rows 1 to 4 of [`indented_block`] as one full-line (`V`) sweep on the Before side.
fn sweep_over_the_block() -> TextPaintState {
    let source = indented_block();
    let last = TextPaintState::row_text(source, 4).len();
    TextPaintState {
        vertical: false,
        anchor: [Some((1, 0)), None],
        cursor: [(4, last), (0, 0)],
        ..Default::default()
    }
}

/// The painting under edit, as `(operation, before spans)`.
fn painted_entries(app: &App) -> Vec<(HumanTextOperation, Vec<HumanTextSpan>)> {
    solution_entries(&app.mapping, &app.text_solution)
        .iter()
        .map(|entry| (entry.operation, entry.before.clone()))
        .collect()
}

/// Invariant 6 at the keystroke: `Minimal` never claims indentation, and a full-line sweep cannot
/// say so itself, so `d` splits it into one range per row, starting at the first code character.
#[test]
fn a_minimal_full_line_sweep_is_painted_without_any_indentation() {
    let source = indented_block();
    let (app, _) = press_in_text_view(source, source, sweep_over_the_block(), KeyCode::Char('d'));

    let entries = painted_entries(&app);
    assert_eq!(
        entries.len(),
        1,
        "one decision, however many ranges it holds"
    );
    let (operation, spans) = &entries[0];
    assert_eq!(*operation, HumanTextOperation::Delete);
    assert_eq!(
        spans,
        &vec![
            HumanTextSpan {
                start_row: 1,
                start_column: 4,
                end_row: 1,
                end_column: 14,
            },
            // Row 2 is blank and drops out: it has no code character to start at.
            HumanTextSpan {
                start_row: 3,
                start_column: 8,
                end_row: 3,
                end_column: 18,
            },
            HumanTextSpan {
                start_row: 4,
                start_column: 4,
                end_row: 4,
                end_column: 15,
            },
        ],
        "one range per row, each past its own indentation"
    );

    assert_eq!(
        app.status.as_deref(),
        Some("Painted 1 deletion(s) - indentation left unpainted (Minimal)"),
        "the count is what the painter selected, not how many rows it became"
    );
}

/// The split `d` records draws no invariant-6 violation; the same painting left whole does.
#[test]
fn the_split_painting_passes_invariant_6_and_the_unsplit_one_does_not() {
    let source = indented_block();
    let before = Code::from_string(source, &Language::Rust);
    let after = Code::from_string(source, &Language::Rust);
    let leading = |app: &App| -> Vec<String> {
        human_mapping::invariants::ground_truth_invariant_violations_for(
            &app.mapping,
            &before,
            &after,
        )
        .expect("the invariant checker should read this mapping")
        .into_iter()
        .filter(|violation| violation.message.contains("leading whitespace"))
        .map(|violation| violation.message)
        .collect()
    };

    let (app, _) = press_in_text_view(source, source, sweep_over_the_block(), KeyCode::Char('d'));
    assert!(leading(&app).is_empty(), "got {:?}", leading(&app));

    // Negative control, so the assertion above is not vacuous: the same sweep painted unsplit,
    // then renamed to `Minimal`.
    let (mut unsplit, _) = press_in_text_view_painting(
        "Full",
        source,
        source,
        sweep_over_the_block(),
        KeyCode::Char('d'),
    );
    unsplit.mapping.text_mappings[0].name = "Minimal".to_string();
    unsplit.text_solution = "Minimal".to_string();
    let violations = leading(&unsplit);
    assert_eq!(
        violations.len(),
        3,
        "one per indented row of the sweep, got {violations:?}"
    );
}

/// Invariant 4 wants a wholly-deleted line painted whole in `Full`, so the sweep is kept as drawn.
#[test]
fn a_full_painting_keeps_the_sweep_exactly_as_drawn() {
    let source = indented_block();
    let (app, _) = press_in_text_view_painting(
        "Full",
        source,
        source,
        sweep_over_the_block(),
        KeyCode::Char('d'),
    );

    let entries = painted_entries(&app);
    assert_eq!(
        entries[0].1,
        vec![HumanTextSpan {
            start_row: 1,
            start_column: 0,
            end_row: 4,
            end_column: TextPaintState::row_text(source, 4).len(),
        }],
        "one multi-row range, indentation and all"
    );
    assert_eq!(
        app.status.as_deref(),
        Some("Painted 1 deletion(s)"),
        "and no note about a split that did not happen"
    );
}

/// A vertical selection names its own columns, so even `Minimal` does not reshape it.
#[test]
fn a_vertical_selection_is_never_reshaped() {
    let source = indented_block();
    // Columns 2..6 of rows 1 and 3: inside the indentation, which the split would have removed.
    let state = TextPaintState {
        anchor: [Some((1, 2)), None],
        cursor: [(3, 6), (0, 0)],
        ..Default::default()
    };
    let (app, _) = press_in_text_view(source, source, state, KeyCode::Char('d'));

    let spans = painted_entries(&app).remove(0).1;
    assert_eq!(
        spans,
        vec![
            HumanTextSpan {
                start_row: 1,
                start_column: 2,
                end_row: 1,
                end_column: 7,
            },
            // Row 2 is empty; a vertical selection skips rows with no character at its columns.
            HumanTextSpan {
                start_row: 3,
                start_column: 2,
                end_row: 3,
                end_column: 7,
            },
        ],
        "painted through the indentation, as drawn"
    );
    assert_eq!(app.status.as_deref(), Some("Painted 2 deletion(s)"));
}

/// The split runs *before* the overlap check: a sweep that collides only in indentation no longer
/// collides once split, and `Minimal` never claims those bytes. Pinned against reordering.
#[test]
fn a_minimal_sweep_commits_over_a_range_that_only_overlapped_the_indentation() {
    let source = indented_block();
    let indentation = HumanTextSpan {
        start_row: 1,
        start_column: 0,
        end_row: 1,
        end_column: 4,
    };
    let seed = |app: &mut App| {
        solution_entries_mut(&mut app.mapping, &app.text_solution.clone()).push(HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![indentation],
            after: Vec::new(),
        });
    };

    let (app, _) = press_in_text_view_seeded(
        "Minimal",
        source,
        sweep_over_the_block(),
        KeyCode::Char('d'),
        seed,
    );
    assert_eq!(
        painted_entries(&app).len(),
        2,
        "the seeded range and the swept one, got status {:?}",
        app.status
    );
    assert_eq!(
        app.status.as_deref(),
        Some("Painted 1 deletion(s) - indentation left unpainted (Minimal)")
    );

    // Without the split the same sweep still collides, so the assertion above is about the split,
    // not a removed overlap check.
    let (app, _) = press_in_text_view_seeded(
        "Full",
        source,
        sweep_over_the_block(),
        KeyCode::Char('d'),
        seed,
    );
    assert_eq!(painted_entries(&app).len(), 1, "only the seeded range");
    assert!(
        app.status
            .as_deref()
            .unwrap_or("")
            .starts_with("Not painted:"),
        "got {:?}",
        app.status
    );
}

/// A sweep over only blank lines trims to nothing. The painter did select something, so it is not
/// told to press `v`.
#[test]
fn a_minimal_sweep_over_blank_lines_only_paints_nothing_and_says_so() {
    let source = "fn main() {\n\n   \n}\n";
    let state = TextPaintState {
        vertical: false,
        anchor: [Some((1, 0)), None],
        cursor: [(2, 3), (0, 0)],
        ..Default::default()
    };
    let (app, _) = press_in_text_view(source, source, state, KeyCode::Char('d'));

    assert!(
        painted_entries(&app).is_empty(),
        "nothing paintable was left after the indentation came out"
    );
    assert_eq!(
        app.status.as_deref(),
        Some("Only blank lines selected - Minimal claims no indentation, so nothing to paint")
    );
}

/// The split's arithmetic: partial first and last rows, a blank middle row, and a row whose
/// indentation swallows the whole span.
#[test]
fn skip_leading_whitespace_splits_a_sweep_row_by_row() {
    let source = indented_block();
    // The whole of rows 1..=4, as a full-line sweep writes it.
    let swept = HumanTextSpan {
        start_row: 1,
        start_column: 0,
        end_row: 4,
        end_column: 15,
    };
    let rows: Vec<(usize, usize, usize)> = skip_leading_whitespace(swept, source)
        .into_iter()
        .map(|span| (span.start_row, span.start_column, span.end_column))
        .collect();
    assert_eq!(rows, vec![(1, 4, 14), (3, 8, 18), (4, 4, 15)]);

    // A sweep starting past the indentation keeps its own, later start on that row.
    let inside = HumanTextSpan {
        start_row: 1,
        start_column: 8,
        end_row: 1,
        end_column: 14,
    };
    assert_eq!(skip_leading_whitespace(inside, source), vec![inside]);

    // A span that ends inside its row's indentation has nothing left once that comes out.
    let indentation_only = HumanTextSpan {
        start_row: 3,
        start_column: 0,
        end_row: 3,
        end_column: 5,
    };
    assert!(skip_leading_whitespace(indentation_only, source).is_empty());
}

/// `^` is the first code character. Painted ranges start there: starting at `0` sweeps the
/// indentation in, which reads differently from the same code painted elsewhere.
#[test]
fn caret_moves_to_the_first_non_whitespace_character() {
    let source = "    let a = 1;\n\tlet b = 2;\n   \nlet c = 3;\n";
    let mut state = TextPaintState::default();
    state.cursor[0] = (0, 12);

    let (_, state) = press_in_text_view(source, source, state, KeyCode::Char('^'));
    assert_eq!(state.cursor[0].1, 4, "past four spaces of indentation");

    // A tab is one byte and one indent character, not one column of many.
    let mut state = state;
    state.cursor[0] = (1, 9);
    let (_, state) = press_in_text_view(source, source, state, KeyCode::Char('^'));
    assert_eq!(state.cursor[0].1, 1);

    // A whitespace-only row has no code character; `^` ends up where `$` would, which is where
    // code would begin, rather than back at `0`'s column.
    let mut state = state;
    state.cursor[0] = (2, 0);
    let (_, state) = press_in_text_view(source, source, state, KeyCode::Char('^'));
    assert_eq!(state.cursor[0].1, 3);

    // An unindented row: `^` and `0` agree, as they do in vi.
    let mut state = state;
    state.cursor[0] = (3, 6);
    let (_, state) = press_in_text_view(source, source, state, KeyCode::Char('^'));
    assert_eq!(state.cursor[0].1, 0);
}

/// The column `^` lands on is a byte offset like every other column here, so an indent followed by
/// a multi-byte character must not report the character's *index*.
#[test]
fn the_first_code_column_is_a_byte_offset() {
    assert_eq!(
        TextPaintState::first_code_column("  \u{e9}t\u{e9} = 1;", 0),
        2
    );
    assert_eq!(TextPaintState::first_code_column("\u{e9} = 1;", 0), 0);
    // Past the end of the file: no row, no code, column 0.
    assert_eq!(TextPaintState::first_code_column("a\n", 9), 0);
}

/// A fixture with no grammar opens text-only with `None` roots, and the navigation keys must still
/// work; the tests above all parse a tree.
#[test]
fn the_navigation_keys_work_in_text_only_mode() {
    let (before_src, after_src) = navigable_pair();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        usize::MAX,
        usize::MAX,
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(Vec::new());
    let caches = Caches::default();
    app.modal = Some(Modal::TextView {
        state: TextPaintState::default(),
    });

    for code in [KeyCode::Char('n'), KeyCode::Char('a'), KeyCode::Char('p')] {
        handle_modal_key(
            &mut app,
            code,
            &flat,
            &flat,
            None,
            None,
            &caches,
            before_src.as_bytes(),
            after_src.as_bytes(),
            &Code::from_string(&before_src, &Language::Rust),
            &Code::from_string(&after_src, &Language::Rust),
        );
        assert!(
            matches!(app.modal, Some(Modal::TextView { .. })),
            "the text view should still be open after {code:?}, got {:?}",
            app.modal
        );
    }

    let Some(Modal::TextView { state }) = &app.modal else {
        unreachable!("asserted above");
    };
    assert_eq!(
        state.cursor[0].0, 30,
        "n then p walked to the first hunk and wrapped back to the last"
    );
}

/// `o` cycles the overlay and `p` navigates; if `p` also cycled, the help would show two keys for
/// one job.
#[test]
fn o_cycles_the_overlay_and_p_no_longer_does() {
    let (before_src, after_src) = navigable_pair();
    let (app, _) = press_in_text_view(
        &before_src,
        &after_src,
        TextPaintState::default(),
        KeyCode::Char('o'),
    );
    assert_eq!(app.text_overlay, TextOverlay::CodeDiff);

    // Each press starts a fresh app, so `Human` is the default; the status line (a jump, not an
    // overlay switch) is what rules out `p` cycling.
    let (app, _) = press_in_text_view(
        &before_src,
        &after_src,
        TextPaintState::default(),
        KeyCode::Char('p'),
    );
    assert_eq!(app.text_overlay, TextOverlay::Human);
    assert!(
        app.status.as_deref().unwrap_or("").starts_with("Diff 3/3"),
        "p navigates now, wrapping to the last hunk from the top; got {:?}",
        app.status
    );
}

/// `diff -u` gives positions only in hunk headers, so the gutter numbers each row. A deleted line
/// has no after number and an inserted one no before number; the blank half is intended.
#[test]
fn the_unix_diff_view_numbers_both_sides() {
    let backend = ratatui::backend::TestBackend::new(110, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 110, 16);
    let output = "--- a\n+++ b\n@@ -10,3 +20,3 @@\n context\n-gone\n+new\n";

    terminal
        .draw(|f| render_unix_diff_modal(f, area, output, 0))
        .unwrap();

    // Compared on collapsed whitespace: the gutter's field width is cosmetic.
    let rows: Vec<String> = rendered_text(&terminal)
        .split('\u{2502}')
        .map(|row| row.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let has = |wanted: &str| rows.iter().any(|row| row.contains(wanted));

    assert!(has("10 20 context"), "got: {rows:#?}");
    assert!(has("11 -gone"), "got: {rows:#?}");
    assert!(has("21 +new"), "got: {rows:#?}");
}

/// `:` opens the jump prompt, digits accumulate, Enter moves the cursor.
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
            Some(root),
            Some(root),
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

/// Every painting-view key preserves the cached `FrameState`: painting writes `text_mappings`, and
/// the caches are built from `entries`. A rebuild per cursor key makes large files unusable.
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

/// Clamped at 100.0, not 0.0: nothing agrees exactly yet, so 0.0 would fail at once and be
/// loosened for a reason that is not a measurement.
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
    // Appended to a file that already has the licence header and the mapping test.
    assert!(block.starts_with("\n#[test]"), "got: {block}");
}

/// Generated strict: an invariant holds or it does not, so there is no placeholder to replace.
#[test]
fn a_fresh_invariants_test_is_generated_strict_with_no_number_to_fill_in() {
    let block = invariants_test_block("rust-add-if");

    assert!(
        block.contains(r#"assert_ground_truth_invariants("rust-add-if")"#),
        "got: {block}"
    );
    assert!(
        !block.contains("within_limit"),
        "the generated form must be the strict one: {block}"
    );
    assert!(
        block.contains("fn invariants()"),
        "the test name has to match the module's convention: {block}"
    );
    assert!(block.starts_with("\n#[test]"), "got: {block}");
}

/// A bare `App` for the text-painting tests, which only touch `mapping`, `dirty` and `status`.
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

/// A selection includes the character under the cursor, as the highlight shows.
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

    assert_eq!(forward.selection(0, source), backward.selection(0, source));
}

/// Byte columns, not character columns: a multi-byte character before the selection must not
/// shift it.
#[test]
fn a_selection_past_a_multibyte_character_lands_on_the_right_text() {
    let source = "let é = foo;\n";
    // 'é' is two bytes, so `foo` starts at byte 9.
    let state = paint_state_at(0, (0, 11), Some((0, 9)));

    let span = state.selection(0, source)[0];

    assert_eq!(
        codediff::test::helper::human_mapping::span_text(source, span),
        Some("foo")
    );
}

/// A multi-row selection is vertical: one span per row over the same columns, so `d`/`i` do not
/// swallow the untouched characters between them on middle rows.
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

/// A row too short to reach the selected columns contributes no span.
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

/// `V` makes one span sweeping full rows, for a contiguous block where `m`'s identical-text check
/// would fail on a per-row split.
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

/// Through the render path: a vertical selection leaves a middle row's tail unstyled, full-line
/// mode styles it.
#[test]
fn vertical_selection_leaves_a_middle_rows_tail_unstyled_but_full_line_does_not() {
    let source = "aaaXaaaaaaaa\nbbbYbbbbbbbb\ncccZcccccccc\n";
    let mut state = paint_state_at(0, (2, 4), Some((0, 3)));
    let selected_bg = Some(OverlayTheme::default().palette().cross_highlight_bg);
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

/// `h`/`l` step by characters, never landing inside one (`span_text` refuses such a span).
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

/// `u` removes the whole entry, both halves of a `Match`: a half match is a malformed entry
/// `verdict` refuses to read.
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

/// The header row carries the interaction state: the `Unmarked` column, the sort arrow and the
/// filter marker, with the filters spelled out in the title.
#[test]
fn render_open_diff_picker_shows_the_unmarked_column_and_the_sort_and_filter_markers() {
    // Wide enough that the title is not truncated; this asserts on its contents.
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

/// At 110 columns ratatui truncates the title: the key legend may be cut, the filter list (the
/// part that changes) must survive.
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
    // A terminal smaller than the minimum still gets a rect that fits.
    let tiny_area = Rect::new(0, 0, 20, 8);
    let rect = centered_rect_at_least(60, 30, 50, 20, tiny_area);
    assert!(rect.width <= tiny_area.width);
    assert!(rect.height <= tiny_area.height);
}

/// On a small terminal a percentage-sized `render_text_modal` can clip the `> {input}` line with
/// no sign that anything is cut off. The body is `PromptPromoteName`'s shape.
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
    // 30-100 before 1000-3000, unbucketed last: as strings, "1000-3000" would sort first.
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
    // With a filter on, `selected` indexes the filtered list; a raw index opens the wrong sample.
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
        Some(root),
        Some(root),
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
    // `s` sorts by the cursor column and the selection follows its row.
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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

    handle_modal_key(
        &mut app,
        KeyCode::Char('k'),
        &flat,
        &flat,
        Some(root),
        Some(root),
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

    for _ in 0..2 {
        handle_modal_key(
            &mut app,
            KeyCode::Char('j'),
            &flat,
            &flat,
            Some(root),
            Some(root),
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
        Some(root),
        Some(root),
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
    // Independent of this repository's history (CI may be shallow): any unresolvable hash takes
    // the same `git diff-tree` failure path.
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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
    // Like `CaseOrigin::Sample`, a git-commit case is not a diffs/ case yet, so it cannot be
    // saved with one key before switching away.
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
        Some(root),
        Some(root),
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
        Some(root),
        Some(root),
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

    // "bravo" is index 1 in `rows` but 0 sorted by Size: `selected` indexes the sorted view.
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

/// `Cmpl` and `Unmarked` set the same way narrow nothing further, so the title shows one
/// constraint (see `DiffFilters::labels`).
#[test]
fn the_title_lists_a_matching_cmpl_and_unmarked_filter_once() {
    let mut filters = DiffFilters {
        cmpl: FlagFilter::Yes,
        unmarked: FlagFilter::Yes,
        ..DiffFilters::default()
    };
    assert_eq!(filters.labels(), vec!["incomplete only"]);

    // Opposite directions are two constraints (unsatisfiable, and visibly so).
    filters.unmarked = FlagFilter::No;
    assert_eq!(filters.labels(), vec!["incomplete only", "none unmarked"]);
}

/// `Cmpl` and `Unmarked` read one map, so their filters select the same rows; they differ only in
/// how they sort.
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
    // Walks every entry, so a new dataset needs no edit here.
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

    // "charlie" is index 2 in `options` but 1 filtered: `selected` indexes the filtered view.
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
    // The open case "alpha" is filtered out, so the selection falls back to the first visible one.
    let modal = open_diff_picker_modal(options, "alpha", view, DiffPickerData::default());
    match modal {
        Modal::OpenDiffPicker { selected, .. } => assert_eq!(selected, 0),
        other => panic!("expected Modal::OpenDiffPicker, got {other:?}"),
    }
}

/// Opens the `o` picker over `options` with `view` and feeds it `keys`.
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
            Some(root),
            Some(root),
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

/// `h`/`l` clamp at both ends of the header rather than wrapping.
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

    let app = press_in_diff_picker(
        options.clone(),
        DiffPickerView::default(),
        &[KeyCode::Char('l'); DiffColumn::ALL.len()],
    );
    assert_eq!(
        picker_view(&app).column,
        DiffColumn::Size,
        "l must clamp at the last column, not wrap round to Name"
    );

    let app = press_in_diff_picker(options, DiffPickerView::default(), &[KeyCode::Char('h')]);
    assert_eq!(
        picker_view(&app).column,
        DiffColumn::Name,
        "h must clamp at the first column"
    );
}

/// The `Invariant` column shows `?` until its scan runs: an unscanned case must not sort or filter
/// as a clean one.
#[test]
fn diff_picker_invariant_column_separates_unscanned_from_clean() {
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        0,
        0,
        HumanMapping::default(),
    );

    assert_eq!(
        DiffPickerData::from_app(&app).invariants_of("alpha"),
        None,
        "before the scan runs, every case reads as unknown"
    );

    app.diff_invariants = Some(
        [("alpha".to_string(), 0usize), ("beta".to_string(), 2usize)]
            .into_iter()
            .collect(),
    );
    let data = DiffPickerData::from_app(&app);
    assert_eq!(data.invariants_of("alpha"), Some(0), "scanned and clean");
    assert_eq!(data.invariants_of("beta"), Some(2), "scanned and broken");
    assert_eq!(
        data.invariants_of("gamma"),
        None,
        "a case the scan left out - no mapping to check - stays unknown, not clean"
    );

    // Fails open on the unknown row both ways, or narrowing would hide exactly the unchecked cases.
    assert!(FlagFilter::Yes.keeps(data.invariants_of("beta").map(|count| count > 0)));
    assert!(!FlagFilter::Yes.keeps(data.invariants_of("alpha").map(|count| count > 0)));
    assert!(FlagFilter::Yes.keeps(data.invariants_of("gamma").map(|count| count > 0)));
    assert!(FlagFilter::No.keeps(data.invariants_of("gamma").map(|count| count > 0)));
}

/// `f` on `Dataset` cycles the filter and persists it on `App` for the next `o`.
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

/// `s` sorts ascending by the cursor column and flips on a second press.
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

/// The open `Name` prompt takes every key, so `j`, `s` and `f` are typed, not acted on.
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

/// An empty submission clears the filter, rather than storing a match-all needle that still shows
/// as filtered.
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

/// Moving the cursor never starts a corpus-wide scan; only `s`/`f` do (see
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
            KeyCode::Char('l'),
        ],
    );

    assert_eq!(picker_view(&app).column, DiffColumn::Invariant);
    assert!(app.diff_unmarked.is_none());
    assert!(app.diff_text_painted.is_none());
    assert!(app.diff_disagreement.is_none());
    assert!(app.diff_invariants.is_none());
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

    // Below `SINGLE_PANEL_WIDTH_THRESHOLD`: only the focused (Before) panel renders.
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
                false,
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
                false,
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
    // Content, not layout: the popup renders `0.9 * height - 2` lines, so the height is derived
    // from HELP_TEXT's length and grows with it.
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
    // `b();` is matched but its `call_expression` child is not.
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

    // Two holes count as two: the picker's `Unmarked` column ranks by this count.
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

/// Every worker count produces the identical map, with a contended cursor and some `None` scans.
#[test]
fn scan_corpus_returns_the_same_map_at_every_worker_count() {
    let names: Vec<String> = (0..500).map(|i| format!("case-{i:03}")).collect();
    // A pure function of the name, so the expected map does not depend on thread scheduling.
    let scan = |name: &str| {
        let n: usize = name.trim_start_matches("case-").parse().unwrap();
        (!n.is_multiple_of(3)).then_some(n * 2)
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

/// A panicking worker takes the process down, rather than returning a map missing its share,
/// which would read as `?` ("not scanned") in the picker.
#[test]
fn scan_corpus_propagates_a_worker_panic() {
    let names: Vec<String> = (0..64).map(|i| format!("case-{i}")).collect();

    // Silences the default hook's backtrace for this deliberate panic.
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
    // Against the checked-in corpus; skips when there is none.
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
    // Pre-seeded, so only the filter runs, not the lazy corpus scan.
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
        Some(root),
        Some(root),
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
    // Against the checked-in corpus; skips when there is none.
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
        Some(root),
        Some(root),
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
    // Unloadable cases are absent, so equality is wrong, but a loose bound would miss a queue that
    // drops most of the corpus. Losing nothing is pinned by
    // `scan_corpus_returns_the_same_map_at_every_worker_count`; this pins that the scan uses it.
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
    assert!(flat.iter().any(|(n, _)| n.id() == root.id()));
}

/// The synthetic-`Caches` tests assume every node, unnamed tokens included, gets an entry. This
/// drives `M` for real through `rebuild_caches` -> `fully_solved_nodes` to show it does.
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

    // Running it again once everything is matched is a no-op.
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

    // The common prefix is matched even though the sweep stopped.
    assert!(app.dirty);
    assert!(
        !app.mapping.entries.is_empty(),
        "should have matched at least the common prefix"
    );

    // The cursor is parked on the mismatched pair, so `m` then `f` resumes.
    assert_eq!(app.before.cursor_id, before_id);
    assert_eq!(app.after.cursor_id, after_id);
}

/// `f` on an insert-only change: the After panel takes the focus and the sweep stops, no modal.
#[test]
fn action_match_to_end_focuses_after_when_the_diff_only_adds() {
    let before_source = "fn main() {\n    a();\n}\n";
    let after_source = "fn main() {\n    a();\n    b();\n}\n";
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
    // Starts on the side the assertion must not be able to get for free.
    app.focus = Focus::Before;
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

    match outcome {
        ActionOutcome::Done(msg) => assert!(
            msg.contains("After panel"),
            "the status line should say where the focus went, got: {msg}"
        ),
        ActionOutcome::NeedsModal(modal) => {
            panic!("a diff that only adds should not need a modal, got {modal:?}")
        }
    }
    assert_eq!(app.focus, Focus::After);
    assert!(
        !app.mapping.entries.is_empty(),
        "the common prefix should still have been matched"
    );
}

/// Only-adds and only-removes each name a side; a mixed or empty diff names none and keeps the
/// modal.
#[test]
fn one_sided_diff_names_a_side_only_when_the_diff_has_one() {
    let adds = "--- before\n+++ after\n@@ -1 +1,2 @@\n a();\n+b();\n";
    let removes = "--- before\n+++ after\n@@ -1,2 +1 @@\n a();\n-b();\n";
    let mixed = "--- before\n+++ after\n@@ -1 +1 @@\n-a();\n+b();\n";
    assert_eq!(one_sided_diff(adds), Some(Side::After));
    assert_eq!(one_sided_diff(removes), Some(Side::Before));
    assert_eq!(one_sided_diff(mixed), None);
    assert_eq!(one_sided_diff(""), None);
    // A header alone is not a change.
    assert_eq!(one_sided_diff("--- before\n+++ after\n"), None);
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
    // Before has an extra trailing `c();`. `f` pairs positionally, so the After cursor reaches the
    // closing `}` while Before is on `c();`: a kind mismatch where the sweep must stop, never
    // pairing `c();` with `}`. The diff only removes lines, so the Before panel takes the focus.
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
    // Starts on the far side, so the focus assertion below cannot pass by default.
    app.focus = Focus::After;
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
        ActionOutcome::Done(msg) => assert!(
            msg.contains("Before panel"),
            "the sweep should stop and say where the focus went, got: {msg}"
        ),
        ActionOutcome::NeedsModal(modal) => panic!(
            "a diff that only removes should hand the Before panel the focus rather than ask, \
             got {modal:?}"
        ),
    }
    assert_eq!(app.focus, Focus::Before);

    let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);
    for id in untouchable_ids {
        assert!(
            !caches.before_match.contains_key(&id) && !caches.before_removed.contains_key(&id),
            "the trailing `c();` statement (or any of its children) must not have been paired \
             with anything on the After side"
        );
    }
}

/// `f` must stay linear in tree size: a quadratic sweep is an effective hang on real files. Uses
/// a large synthetic identical pair, so no fixture on disk is needed.
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

/// `M`'s recursion (`auto_match_pair`) must stay linear too; one dedup scan per node is quadratic.
/// Same synthetic tree as `action_match_to_end_is_linear_not_quadratic_in_tree_size`.
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
    // The `if`'s block has 3 children before and 4 after, so `auto_match_pair` stops there. `a();`
    // is below that point, so `M` at the function must leave its existing entry alone.
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
    // Identical pair: `M` at the root revisits every node, `a();` included, so its wrong
    // pre-seeded entry must be replaced by exactly one correct entry, with no stale leftover.
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

/// (path, promoted_to, dataset) per row; the other columns only identify rows.
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
                // Every other row's dataset survives: the column a naive rewrite would clobber.
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

    // No row matched, so nothing is rewritten.
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
    // Reject the second row, so the map has all three statuses (the third is `write_csv`'s
    // default).
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

/// `read_csv` plus the `status` and `comment` columns.
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

    // No row matched, so the file keeps its original 6-column shape.
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
                // A comment never touches promoted_to or status, even on a PROMOTED row.
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

    // Identical source matches by one hash comparison at the root, so both roots report
    // `IdenticalHash` and descendants have no entry of their own.
    let before_reason = algo_reason(Side::Before, before_ast.root_node(), &diff_ast);
    let after_reason = algo_reason(Side::After, after_ast.root_node(), &diff_ast);
    assert_eq!(before_reason, Some(ASTMappingReason::IdenticalHash));
    assert_eq!(after_reason, Some(ASTMappingReason::IdenticalHash));
    assert_eq!(reason_label(before_reason.unwrap()), "IdHash");
}

#[test]
fn algo_reason_is_none_when_the_diff_has_no_entry_for_the_node() {
    // An empty ASTDiff (before `p`) misses cleanly rather than panicking.
    let source = "fn f() {}\n";
    let code = codediff::code::Code::from_string(source, &codediff::code::Language::Rust);
    let root = code.ast.as_ref().unwrap().root_node();
    let empty_diff = ASTDiff::default();

    assert_eq!(algo_reason(Side::Before, root, &empty_diff), None);
    assert_eq!(algo_reason(Side::After, root, &empty_diff), None);
}

#[test]
fn reason_label_matches_benchmark_optimal_solutions_abbreviations() {
    // Must match `benchmark_optimal_solutions`' `REASONS` table, so one abbreviation means one
    // thing in both tools.
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
    assert_eq!(
        reason_detail(ASTMappingReason::BottomUpPropagation),
        "BottomUpProp"
    );
}

// -----------------------------------------------------------------------------------------
// Multi-map groups (x/c selection, m/M commit, u removal)
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

    // A plain entry on the first before/after foo, which the group commit displaces.
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
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
    // A `BTreeSet<usize>` orders by node id, which is not parse-stable. Paths must come out in
    // source order, so re-selecting in a later session does not reshuffle the saved file.
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
        GroupPairing::AnyOneToOne,
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

    // A stale entry on a descendant of `before_foos[0]`, which a `with_children` commit sweeps as
    // `d`/`i`'s `clear_before_descendants`/`clear_after_descendants` do.
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
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
        GroupPairing::AnyOneToOne,
    )
    .unwrap();

    assert!(matches!(outcome, ActionOutcome::Done(_)));
    assert_eq!(mapping.groups.len(), 1);
    // No content hashes, so the group falls back to `MatchButNotIdentical`.
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
        GroupPairing::AnyOneToOne,
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

    // Any member, here a leftover on Delete, drops the whole group.
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
    // `m`, not `M`: no subtree closure required.
    assert!(!app.mapping.groups[0].with_children);
}

/// Three statements before, two after, every one pending on both sides.
fn app_with_every_statement_pending() -> (
    App,
    tree_sitter::Tree,
    tree_sitter::Tree,
    &'static str,
    &'static str,
) {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
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
    app.before_multi_select = block_statements(before_root)
        .iter()
        .map(|n| n.id())
        .collect();
    app.after_multi_select = block_statements(after_root)
        .iter()
        .map(|n| n.id())
        .collect();
    (app, before_tree, after_tree, before_source, after_source)
}

fn press(
    app: &mut App,
    code: KeyCode,
    before_tree: &tree_sitter::Tree,
    after_tree: &tree_sitter::Tree,
    before_source: &str,
    after_source: &str,
) {
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let no_hashes = rustc_hash::FxHashMap::default();
    handle_key(
        app,
        code,
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
}

#[test]
fn handle_key_capital_x_flips_the_pending_selections_pairing_and_says_so() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    assert_eq!(app.multi_select_pairing, GroupPairing::AnyOneToOne);

    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    assert_eq!(app.multi_select_pairing, GroupPairing::AllToAll);
    let status = app.status.clone().unwrap_or_default();
    assert!(
        status.contains("3 before, 2 after") && status.contains("ALL-TO-ALL"),
        "{status}"
    );
    assert!(
        !app.dirty,
        "flipping the pairing changes nothing recorded yet"
    );

    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    assert_eq!(app.multi_select_pairing, GroupPairing::AnyOneToOne);
    assert!(
        app.status
            .clone()
            .unwrap_or_default()
            .contains("any one-to-one pairing"),
        "{:?}",
        app.status
    );
}

#[test]
fn handle_key_m_with_an_all_to_all_selection_commits_it_and_resets_the_pairing() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    // On an all-to-all selection `M` walks the subtrees, so `m` commits the roots alone.
    press(
        &mut app,
        KeyCode::Char('m'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );

    assert_eq!(app.mapping.groups.len(), 1, "{:?}", app.mapping.groups);
    let group = &app.mapping.groups[0];
    assert_eq!(group.pairing, GroupPairing::AllToAll);
    assert!(!group.with_children);
    // No content hashes, so nothing is provably identical.
    assert_eq!(group.operation, HumanOperation::MatchButNotIdentical);
    assert!(
        app.status
            .clone()
            .unwrap_or_default()
            .starts_with("Committed all-to-all group: 3 before, 2 after"),
        "{:?}",
        app.status
    );
    assert!(app.dirty);
    assert!(app.before_multi_select.is_empty() && app.after_multi_select.is_empty());
    assert_eq!(
        app.multi_select_pairing,
        GroupPairing::AnyOneToOne,
        "the next selection starts plain; all-to-all is always a deliberate X"
    );
}

#[test]
fn handle_key_c_drops_the_pairing_with_the_selection() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    press(
        &mut app,
        KeyCode::Char('c'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    assert!(app.before_multi_select.is_empty());
    assert_eq!(app.multi_select_pairing, GroupPairing::AnyOneToOne);
}

#[test]
fn confirming_a_mixed_kind_all_to_all_group_records_the_pairing_it_was_raised_with() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    app.modal = Some(Modal::ConfirmMultiMapGroup {
        before_ids: app.before_multi_select.iter().copied().collect(),
        after_ids: app.after_multi_select.iter().copied().collect(),
        operation: HumanOperation::MatchButNotIdentical,
        with_children: false,
        pairing: GroupPairing::AllToAll,
        kinds: vec![
            "expression_statement".to_string(),
            "let_declaration".to_string(),
        ],
    });

    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    handle_modal_key(
        &mut app,
        KeyCode::Char('y'),
        &before_flat,
        &after_flat,
        Some(before_root),
        Some(after_root),
        &caches,
        before_source.as_bytes(),
        after_source.as_bytes(),
        &Code::from_string(before_source, &Language::Rust),
        &Code::from_string(after_source, &Language::Rust),
    );

    assert!(app.modal.is_none());
    assert_eq!(app.mapping.groups.len(), 1, "{:?}", app.mapping.groups);
    assert_eq!(app.mapping.groups[0].pairing, GroupPairing::AllToAll);
    assert!(!app.mapping.groups[0].with_children);
    assert_eq!(app.multi_select_pairing, GroupPairing::AnyOneToOne);
}

#[test]
fn action_unmark_names_the_kind_of_group_it_removes() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    press(
        &mut app,
        KeyCode::Char('m'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    assert_eq!(app.mapping.groups.len(), 1);

    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let before_flat = FlatIndex::new(flatten_visible(before_root, &app.before.collapsed, None));
    let after_flat = FlatIndex::new(flatten_visible(after_root, &app.after.collapsed, None));
    let member = block_statements(before_root)[2];
    let msg = action_unmark(
        &mut app.mapping,
        Focus::Before,
        &before_flat,
        &after_flat,
        member.id(),
        after_root.id(),
        before_root,
        after_root,
        &caches,
    )
    .unwrap();
    assert!(
        msg.starts_with("Removed all-to-all group (3 before, 2 after"),
        "{msg}"
    );
    assert!(app.mapping.groups.is_empty());
}

/// Every `foo();` statement pending on both sides, flipped to all-to-all.
fn app_with_all_to_all_selection(before_root: Node, after_root: Node) -> App {
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        before_root.id(),
        after_root.id(),
        HumanMapping::default(),
    );
    app.before_multi_select = block_statements(before_root)
        .iter()
        .map(|n| n.id())
        .collect();
    app.after_multi_select = block_statements(after_root)
        .iter()
        .map(|n| n.id())
        .collect();
    app.multi_select_pairing = GroupPairing::AllToAll;
    app
}

#[test]
fn all_to_all_subtree_groups_returns_one_member_set_per_position() {
    let source = "fn main() {\n    foo();\n    foo();\n}\n";
    let tree = parse_rust(source);
    let root = tree.root_node();
    let statements = block_statements(root);
    assert_eq!(statements.len(), 2);

    let groups = all_to_all_subtree_groups(
        vec![statements[0]],
        vec![statements[1]],
        source.as_bytes(),
        source.as_bytes(),
    )
    .unwrap();

    // `foo();` is seven nodes (statement, call, identifier, arguments, two parens, semicolon):
    // one 1:1 group each.
    assert_eq!(groups.len(), 7, "{groups:?}");
    let kinds: Vec<&str> = groups.iter().map(|(before, _)| before[0].kind()).collect();
    assert_eq!(
        kinds,
        vec![
            "expression_statement",
            "call_expression",
            "identifier",
            "arguments",
            "(",
            ")",
            ";"
        ]
    );
    for (before, after) in &groups {
        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_eq!(before[0].kind(), after[0].kind());
        assert_ne!(before[0].id(), after[0].id(), "distinct nodes, same shape");
    }
}

#[test]
fn all_to_all_subtree_groups_keeps_every_member_of_an_n_to_m_selection_together() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);

    let groups = all_to_all_subtree_groups(
        block_statements(before_tree.root_node()),
        block_statements(after_tree.root_node()),
        before_source.as_bytes(),
        after_source.as_bytes(),
    )
    .unwrap();

    assert_eq!(groups.len(), 7);
    for (before, after) in &groups {
        assert_eq!(
            (before.len(), after.len()),
            (3, 2),
            "every position keeps the selection's own shape"
        );
    }
}

#[test]
fn all_to_all_subtree_groups_reports_a_kind_divergence_and_returns_nothing() {
    let before_source = "fn main() {\n    foo();\n}\n";
    let after_source = "fn main() {\n    x = 1;\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);

    let error = all_to_all_subtree_groups(
        block_statements(before_tree.root_node()),
        block_statements(after_tree.root_node()),
        before_source.as_bytes(),
        after_source.as_bytes(),
    )
    .unwrap_err()
    .to_string();

    // Both sides' statements are `expression_statement`, so the divergence is one level down.
    assert!(
        error.contains("kinds differ")
            && error.contains("call_expression")
            && error.contains("assignment_expression"),
        "{error}"
    );
    assert!(error.contains("nothing committed"), "{error}");
}

#[test]
fn all_to_all_subtree_groups_reports_a_child_count_divergence() {
    let before_source = "fn main() {\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo(1);\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);

    let error = all_to_all_subtree_groups(
        block_statements(before_tree.root_node()),
        block_statements(after_tree.root_node()),
        before_source.as_bytes(),
        after_source.as_bytes(),
    )
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("arguments") && error.contains("child(ren)"),
        "{error}"
    );
}

#[test]
fn handle_key_capital_m_on_an_all_to_all_selection_commits_every_position() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let mut app = app_with_all_to_all_selection(before_tree.root_node(), after_tree.root_node());

    press(
        &mut app,
        KeyCode::Char('M'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );

    assert_eq!(app.mapping.groups.len(), 7, "{:?}", app.mapping.groups);
    for group in &app.mapping.groups {
        assert_eq!(group.pairing, GroupPairing::AllToAll);
        assert_eq!((group.before_paths.len(), group.after_paths.len()), (3, 2));
        // A descendant's own group is the claim about it, so closure asserts nothing more.
        assert!(!group.with_children);
    }
    assert!(app.mapping.entries.is_empty(), "{:?}", app.mapping.entries);
    assert!(app.dirty);
    assert!(app.before_multi_select.is_empty() && app.after_multi_select.is_empty());
    assert_eq!(app.multi_select_pairing, GroupPairing::AnyOneToOne);
    assert!(
        app.status
            .clone()
            .unwrap_or_default()
            .starts_with("Committed 7 all-to-all groups:"),
        "{:?}",
        app.status
    );
}

#[test]
fn handle_key_lowercase_m_on_an_all_to_all_selection_still_commits_only_the_roots() {
    let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let mut app = app_with_all_to_all_selection(before_tree.root_node(), after_tree.root_node());

    press(
        &mut app,
        KeyCode::Char('m'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );

    assert_eq!(app.mapping.groups.len(), 1, "{:?}", app.mapping.groups);
    assert_eq!(app.mapping.groups[0].pairing, GroupPairing::AllToAll);
}

#[test]
fn handle_key_capital_m_commits_nothing_when_the_subtrees_diverge() {
    let before_source = "fn main() {\n    foo();\n}\n";
    let after_source = "fn main() {\n    foo(1);\n}\n";
    let before_tree = parse_rust(before_source);
    let after_tree = parse_rust(after_source);
    let mut app = app_with_all_to_all_selection(before_tree.root_node(), after_tree.root_node());

    press(
        &mut app,
        KeyCode::Char('M'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );

    assert!(
        app.mapping.groups.is_empty(),
        "not even the roots: {:?}",
        app.mapping.groups
    );
    assert!(!app.dirty, "a refused commit leaves nothing to save");
    assert!(app.modal.is_none(), "the walk reports, it does not ask");
    let status = app.status.clone().unwrap_or_default();
    assert!(status.starts_with("Error: Subtrees diverge:"), "{status}");
    assert!(
        !app.before_multi_select.is_empty(),
        "the selection survives, so it can be fixed and retried"
    );
    assert_eq!(
        app.multi_select_pairing,
        GroupPairing::AllToAll,
        "including its pairing"
    );
}

#[test]
fn render_panel_marks_an_all_to_all_member_with_a_capital_g() {
    let (mut app, before_tree, after_tree, before_source, after_source) =
        app_with_every_statement_pending();
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();
    press(
        &mut app,
        KeyCode::Char('X'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );
    press(
        &mut app,
        KeyCode::Char('m'),
        &before_tree,
        &after_tree,
        before_source,
        after_source,
    );

    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(before_root.id());
    // Tall enough for every row of the fully expanded tree, so the third statement is on screen.
    let backend = ratatui::backend::TestBackend::new(60, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_panel(
                f,
                Rect::new(0, 0, 60, 40),
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
                &std::collections::BTreeSet::new(),
                &app.mapping.groups,
            );
        })
        .unwrap();

    let content = terminal.backend().buffer().content();
    // Every member is matched, including the third an any-one-to-one group would leave over.
    for statement in block_statements(before_root) {
        assert_eq!(status_before(statement, &caches), NodeStatus::Matched);
        let row = flat
            .iter()
            .position(|(n, _)| n.id() == statement.id())
            .unwrap()
            + 1;
        let text: String = (0..60)
            .map(|col| content[row * 60 + col].symbol())
            .collect();
        // Column 0 is the panel border; the row reads "<glyph><marker> <label>".
        let marker = text.trim_start_matches('│').trim_start().chars().nth(1);
        assert_eq!(marker, Some('G'), "{text:?}");
    }
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
        GroupPairing::AnyOneToOne,
    )
    .unwrap();

    let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
    let flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(before_root.id());

    // A before-foo outside any group, pending, to show it renders distinctly.
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
                &mapping.groups,
            );
        })
        .unwrap();

    // Row 0 and column 0 are the `Block` border.
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

/// A tab in a ratatui cell leaves characters on screen after the modal closes (see
/// `display_safe_char`). This Go fixture is tab-indented.
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

/// A tab becomes one character, not a tab stop: paint cursor columns are byte offsets.
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

/// A `\r` returns the terminal cursor to column 0, overwriting the row. `split('\n')` keeps a CRLF
/// file's `\r`, so the renderer must catch it.
#[test]
fn the_text_view_never_puts_a_carriage_return_in_the_buffer() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/test/data/diffs/small/typescript-microsoft-typescript-add-target-comment");
    let source = std::fs::read_to_string(dir.join("before.ts.test")).unwrap();
    assert!(
        source.contains('\r'),
        "this test is pointless unless the fixture is CRLF"
    );

    let state = TextPaintState::default();
    let lines = render_paint_side(&source, &[], &state, 0, 100, 10_000);
    assert!(!lines.is_empty());

    for line in &lines {
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            !rendered.chars().any(|c| c.is_ascii_control()),
            "a raw control character reached the buffer: {rendered:?}"
        );
    }
}

/// A CRLF row's `\r` is part of the terminator, not a column: otherwise `$`, selections and
/// multi-row spans reach one phantom cell past the last visible character.
#[test]
fn a_crlf_row_ends_at_its_last_visible_character() {
    let source = "let x = 1;\r\nlet y = 2;\r\n";

    assert_eq!(TextPaintState::row_text(source, 0), "let x = 1;");
    assert_eq!(
        TextPaintState::row_text(source, 0).len(),
        source.split('\n').next().unwrap().len() - 1,
        "the row must be exactly one byte shorter than what split('\\n') hands back"
    );

    let span = HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 1,
        end_column: 0,
    };
    let row_len = TextPaintState::row_text(source, 0).len();
    assert!(span_covers(span, 0, row_len - 1, row_len));
    assert!(
        !span_covers(span, 0, row_len, row_len),
        "the terminator is not a paintable column"
    );
}

/// One byte for one byte: a C1 code point is two UTF-8 bytes, and spans are in byte columns.
#[test]
fn display_safe_leaves_multi_byte_control_code_points_alone() {
    assert_eq!(display_safe_char('\u{9c}'), '\u{9c}');
    assert_eq!(display_safe_char('\r'), ' ');
}

/// Promoting or rejecting rewrites the whole sample.csv, so a column the reader drops is erased
/// from every row.
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

/// The same against the real corpus file, so a new sample.csv column this tool does not know
/// fails here instead of being deleted on the next promotion.
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

/// The seed must *be* codediff's rendering, so it starts at zero disagreement.
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
    // `HumanTextEntry` has no `PartialEq`; compare the rendered shape.
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

/// The overlap guard, against a fixture that trips it: codediff's rendering overlaps on some
/// corpus fixtures, and a painting cannot represent that (the renderer resolves an overlap by
/// highest verdict, the scorer by list order).
#[test]
fn seeding_refuses_a_pair_whose_codediff_ranges_overlap() {
    let pair = codediff::test::helper::handmade_test_code_pair("xml-odoo-odoo-add-two-attributes")
        .expect("fixture should exist");
    let (before, after) = &*pair;

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

/// The text of every screen row a call produced, gutter included.
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

    // The wrapped rows reassemble into the original line.
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

/// `scroll_into_view` bounds the cursor in source rows; one long wrapped line can fill the
/// viewport, so the renderer walks its start row forward to keep the cursor row on screen.
#[test]
fn the_cursor_row_stays_visible_when_the_rows_above_it_wrap() {
    let source = format!("{}\n{}\nCURSORROW\n", "a".repeat(400), "b".repeat(400));
    let mut state = TextPaintState::default();
    state.cursor[0] = (2, 0);
    state.scroll[0] = 0;

    // Rows 0 and 1 wrap to 12 screen rows each: a render from row 0 never reaches row 2.
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
    // Focus the other side: `PaintClass::Cursor` outranks `Painted` and would mask the check.
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

/// Wrapping counts terminal cells: a CJK ideograph is one character and two cells.
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

/// Two ranges claiming one byte are not representable (the renderer and the scorer resolve an
/// overlap differently), so they are refused at the keystroke.
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

    // Cols 4..12 share bytes 4..8 with it.
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

/// Ranges meeting at a line boundary share only the newline, which `label_bytes` never labels;
/// refusing them would make line-by-line painting impossible.
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

/// `!` clears all three ground truths: paintings left behind would assert about a mapping that no
/// longer exists.
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

/// Only the explicit key confirms: Enter confirms everywhere else, so it is the reflex press on a
/// modal that cannot be undone.
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
            Some(root),
            Some(root),
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

// ---------------------------------------------------------------------------------------------
// Text-only mode: a fixture tree-sitter has no grammar for
// ---------------------------------------------------------------------------------------------

/// A Bazel `BUILD`-shaped pair, in a language with no tree-sitter grammar. It must open.
fn unparseable_pair() -> (Code, Code) {
    let before = "cc_library(\n    name = \"a\",\n    srcs = [\"a.cc\"],\n)\n";
    let after = "cc_library(\n    name = \"a\",\n    srcs = [\"b.cc\"],\n)\n";
    (
        Code::from_string(before, &Language::Unknown),
        Code::from_string(after, &Language::Unknown),
    )
}

#[test]
fn compute_frame_state_has_no_roots_for_a_pair_with_no_grammar() -> Result<()> {
    let (before, after) = unparseable_pair();
    assert!(
        is_text_only(&before, &after),
        "the premise of these tests is a pair tree-sitter cannot parse"
    );

    let app = test_app();
    let state = compute_frame_state(&before, &after, &app)?;

    assert!(state.roots().is_none(), "there is no tree to hand out");
    assert!(state.before_flat.is_empty() && state.after_flat.is_empty());
    assert_eq!((state.before_unmarked, state.after_unmarked), (0, 0));
    // The text is present either way: a painting is made of it.
    assert_eq!(state.before_src, before.contents.as_bytes());
    assert_eq!(state.after_src, after.contents.as_bytes());
    Ok(())
}

/// Empty panels without a reason would read as a bug.
#[test]
fn draw_ui_names_the_missing_grammar_instead_of_drawing_an_empty_tree() {
    let (before, after) = unparseable_pair();
    let mut app = test_app();
    let flat = FlatIndex::new(Vec::new());
    let caches = Caches::default();

    let backend = ratatui::backend::TestBackend::new(SINGLE_PANEL_WIDTH_THRESHOLD, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            draw_ui(
                f,
                &mut app,
                &flat,
                &flat,
                &caches,
                before.contents.as_bytes(),
                after.contents.as_bytes(),
                0,
                0,
                "bazel-not-actually-supported-by-treesitter",
                true,
            )
        })
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(
        text.contains("<Language not supported by TreeSitter>"),
        "the panels should say the language is unsupported: {text}"
    );
}

/// `t` opens the paint view with no tree: the key text-only mode exists for.
#[test]
fn the_paint_view_opens_without_a_tree() {
    let (before, after) = unparseable_pair();
    let mut app = test_app();

    handle_tree_independent_key(
        &mut app,
        KeyCode::Char('t'),
        before.contents.as_bytes(),
        after.contents.as_bytes(),
        &before,
        &after,
        false,
    );

    assert!(
        matches!(app.modal, Some(Modal::TextView { .. })),
        "t should open the painting view, got {:?}",
        app.modal.is_some()
    );
}

/// A tree key is answered, not swallowed: a dead keypress reads as a hang.
#[test]
fn a_tree_key_explains_itself_in_text_only_mode() {
    let (before, after) = unparseable_pair();
    let mut app = test_app();

    handle_tree_independent_key(
        &mut app,
        KeyCode::Char('m'),
        before.contents.as_bytes(),
        after.contents.as_bytes(),
        &before,
        &after,
        false,
    );

    let status = app.status.clone().unwrap_or_default();
    assert!(
        status.contains("No tree-sitter grammar"),
        "an unhandled tree key should say why it did nothing, got {status:?}"
    );
    assert!(app.mapping.entries.is_empty(), "and must not map anything");
}

/// With a tree, `handle_key` claims `m` itself, so the explanation must not overwrite what `m`
/// reports.
#[test]
fn the_same_key_is_not_explained_away_when_there_is_a_tree() {
    let (before, after) = unparseable_pair();
    let mut app = test_app();
    let before_status = app.status.clone();

    handle_tree_independent_key(
        &mut app,
        KeyCode::Char('m'),
        before.contents.as_bytes(),
        after.contents.as_bytes(),
        &before,
        &after,
        true,
    );

    assert_eq!(app.status, before_status);
}

/// For a pair with no grammar codediff answers with its plain-text fallback, so `p` and `P` have
/// something to work with.
#[test]
fn codediff_text_spans_falls_back_to_the_plain_text_diff() {
    let (before, after) = unparseable_pair();
    let [before_spans, after_spans] = codediff_text_spans(&before, &after);

    assert!(
        !before_spans.is_empty() && !after_spans.is_empty(),
        "an empty overlay leaves a human painting with nothing to compare against"
    );
}

/// No tree, so `assert_matches_human_mapping` could only fail.
#[test]
fn a_text_only_stub_has_no_mapping_test() {
    let contents = stub_test_contents("bazel-not-actually-supported-by-treesitter", None, true);

    assert!(
        !contents.contains("assert_matches_human_mapping"),
        "a fixture with no tree must not carry a mapping assertion: {contents}"
    );
    assert!(
        !contents.contains("fn mapping()"),
        "and no mapping test at all: {contents}"
    );
    assert!(
        contents.contains(
            "//! This fixture's language has no tree-sitter grammar, so there is no tree to map\n"
        ),
        "the file should say why it is shaped differently, on its own line with no stray \
         indentation: {contents}"
    );
    assert!(
        contents.lines().all(|line| line.len() <= 100),
        "no line should run past this codebase's comment width: {contents}"
    );
    // The later writers need an anchor here even without `use crate::test;`.
    assert!(contents.contains("use anyhow::Result;\n"));
}

/// The file a text-only fixture ends up with, written in `action_save`'s order: a painting test,
/// the invariants test, no mapping test. The imports must land in the import block, anchored on
/// `use anyhow::Result;`.
#[test]
fn a_text_only_fixture_file_carries_a_painting_and_invariants_but_no_mapping() {
    let name = "bazel-not-actually-supported-by-treesitter";
    let mut file = stub_test_contents(name, None, true);
    file = insert_use_line(
        &file,
        "use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;\n",
    );
    file.push_str(&painting_test_block(name));
    file = insert_use_line(
        &file,
        "use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;\n",
    );
    file.push_str(&invariants_test_block(name));

    assert!(file.contains("\nfn painting() -> Result<()> {\n"), "{file}");
    assert!(
        file.contains("\nfn invariants() -> Result<()> {\n"),
        "{file}"
    );
    assert!(!file.contains("fn mapping()"), "{file}");
    let first_test = file.find("#[test]").unwrap();
    for import in [
        "assert_matches_human_painting_within_limit;",
        "assert_ground_truth_invariants;",
    ] {
        assert!(file.find(import).unwrap() < first_test, "{import}: {file}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// `A` in the text view: put this side's tree panel on the leaf under the text cursor
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Presses `A` in the text view at `(row, column)` on `side` through `handle_modal_key` (an unwired
/// key is the failure worth pinning), and returns the kind and text of the leaf the tree panel
/// landed on. `Code` and the asserted tree share one parse: node ids are only valid within it.
fn press_reveal_node(
    source: &str,
    side: usize,
    row: usize,
    column: usize,
) -> (App, String, String, usize) {
    let code = Code::from_string(source, &Language::Rust);
    let root = code.ast.as_ref().expect("parsed").root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    let mut state = TextPaintState {
        side,
        ..Default::default()
    };
    state.cursor[side] = (row, column);
    app.modal = Some(Modal::TextView { state });

    handle_modal_key(
        &mut app,
        KeyCode::Char('A'),
        &flat,
        &flat,
        Some(root),
        Some(root),
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &code,
        &code,
    );

    let cursor_id = if side == 0 {
        app.before.cursor_id
    } else {
        app.after.cursor_id
    };
    let landed =
        find_node_by_id_anywhere(root, cursor_id).expect("cursor is on a node in the tree");
    let kind = landed.kind().to_string();
    let text = source[landed.byte_range()].to_string();
    (app, kind, text, root.id())
}

#[test]
fn a_in_the_text_view_puts_the_tree_cursor_on_the_leaf_under_it() {
    let (app, kind, text, _) = press_reveal_node("let alpha = 1;\nlet beta = 2;\n", 0, 1, 4);
    assert_eq!((kind.as_str(), text.as_str()), ("identifier", "beta"));
    assert!(
        matches!(app.modal, Some(Modal::TextView { .. })),
        "the text view stays open, so several rows can be checked in one pass"
    );
    assert_eq!(
        app.focus,
        Focus::Before,
        "focus follows the panel that moved, so Esc returns to it"
    );
}

#[test]
fn a_in_the_text_view_moves_the_panel_for_the_side_it_is_on() {
    let (app, kind, text, root_id) = press_reveal_node("let alpha = 1;\n", 1, 0, 4);
    assert_eq!((kind.as_str(), text.as_str()), ("identifier", "alpha"));
    assert_eq!(app.focus, Focus::After);
    assert_eq!(
        app.before.cursor_id, root_id,
        "the other side's panel is left exactly where it was"
    );
}

/// Whitespace between tokens belongs to no leaf, so `A` lands on the next one and the status line
/// says so.
#[test]
fn a_in_the_text_view_lands_on_the_next_leaf_from_inter_token_whitespace() {
    let (app, kind, text, _) = press_reveal_node("let alpha  =  1;\n", 0, 0, 10);
    assert_eq!((kind.as_str(), text.as_str()), ("=", "="));
    assert!(
        app.status
            .as_deref()
            .is_some_and(|s| s.contains("next leaf")),
        "got {:?}",
        app.status
    );
}

/// `byte_offset`, not `row_text` (which strips `\r`): offsets from stripped rows fall a byte behind
/// per row, landing on `let` instead of `ccc` here.
#[test]
fn a_in_the_text_view_finds_the_right_leaf_in_a_crlf_file() {
    let (_, kind, text, _) =
        press_reveal_node("let aaa = 1;\r\nlet bbb = 2;\r\nlet ccc = 3;\r\n", 0, 2, 4);
    assert_eq!((kind.as_str(), text.as_str()), ("identifier", "ccc"));
}

#[test]
fn byte_offset_counts_a_crlf_terminator_as_two_bytes() {
    let source = "let aaa = 1;\r\nlet bbb = 2;\r\n";
    assert_eq!(TextPaintState::byte_offset(source, 0, 4), Some(4));
    assert_eq!(
        TextPaintState::byte_offset(source, 1, 4),
        Some(18),
        "row 0 is 12 bytes plus a two-byte terminator, not one"
    );
    assert_eq!(&source[18..21], "bbb");
    assert_eq!(
        TextPaintState::byte_offset(source, 9, 0),
        None,
        "past the last row there is no offset to give"
    );
}

/// A text-only fixture has no tree for `A` to reveal anything in.
#[test]
fn a_in_the_text_view_says_so_on_a_side_with_no_syntax_tree() {
    let text_only = Code::from_string("plain words\n", &Language::Unknown);
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        usize::MAX,
        usize::MAX,
        HumanMapping::default(),
    );
    let state = TextPaintState::default();
    action_paint_reveal_node(&mut app, &state, &text_only, &text_only);
    assert!(
        app.status
            .as_deref()
            .is_some_and(|s| s.contains("text-only")),
        "got {:?}",
        app.status
    );
    assert_eq!(app.before.cursor_id, usize::MAX, "nothing moved");
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// `V`: the invariant list
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// A `Minimal` painting claiming a line's indentation: one invariant-6 violation and no other
/// (the run ends mid-row, the line is partly painted, the bytes are contiguous, no tree mapping).
fn app_with_one_violation(source: &str) -> (App, Code) {
    let code = Code::from_string(source, &Language::Rust);
    let root = code.ast.as_ref().expect("parsed").root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    app.mapping.text_mappings.push(NamedTextMapping {
        name: "Minimal".to_string(),
        mapping: HumanTextMapping {
            entries: vec![HumanTextEntry {
                operation: HumanTextOperation::Delete,
                before: vec![HumanTextSpan {
                    start_row: 1,
                    start_column: 0,
                    end_row: 1,
                    end_column: 8,
                }],
                after: Vec::new(),
            }],
        },
    });
    app.text_solution = "Minimal".to_string();
    (app, code)
}

fn press_with_one_violation(key: KeyCode) -> App {
    let source = "fn f() {\n    let x = 1;\n}\n";
    let (mut app, code) = app_with_one_violation(source);
    let root = code.ast.as_ref().unwrap().root_node();
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    let hashes: rustc_hash::FxHashMap<usize, u64> = Default::default();

    handle_key(
        &mut app,
        key,
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &hashes,
        &hashes,
        &code,
        &code,
    );
    app
}

#[test]
fn v_lists_the_violations_of_the_in_memory_mapping() {
    let app = press_with_one_violation(KeyCode::Char('V'));
    let Some(Modal::InvariantList { entries, selected }) = &app.modal else {
        panic!("V should open the invariant list, got {:?}", app.modal);
    };
    assert_eq!(entries.len(), 1, "{entries:#?}");
    assert_eq!(*selected, 0);
    assert_eq!(entries[0].violation.invariant, 6);
    assert_eq!(entries[0].violation.painting.as_deref(), Some("Minimal"));
    // The painting is only in memory, so a check reading `human_mapping.json` would find nothing.
    assert_eq!(
        entries[0].details,
        vec!["before 2:0-2:4  \"    \"  in block 7..25".to_string()],
        "the site is the claimed indentation itself - columns 0..4, not the whole painted \
         range - with the text under it and the node it falls in"
    );
}

#[test]
fn v_says_so_when_a_case_breaks_nothing() {
    let source = "fn f() {\n    let x = 1;\n}\n";
    let code = Code::from_string(source, &Language::Rust);
    let root = code.ast.as_ref().unwrap().root_node();
    let mut app = App::new(
        "test".to_string(),
        CaseOrigin::Diffs,
        root.id(),
        root.id(),
        HumanMapping::default(),
    );
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);
    let hashes: rustc_hash::FxHashMap<usize, u64> = Default::default();
    handle_key(
        &mut app,
        KeyCode::Char('V'),
        &flat,
        &flat,
        root,
        root,
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &hashes,
        &hashes,
        &code,
        &code,
    );
    assert!(app.modal.is_none(), "no popup for a clean case");
    assert!(
        app.status.as_deref().is_some_and(|s| s.contains("none")),
        "got {:?}",
        app.status
    );
}

/// Enter puts both a tree cursor and a text cursor on the row an invariant names.
#[test]
fn enter_in_the_invariant_list_moves_the_tree_and_the_text_cursor() {
    let source = "fn f() {\n    let x = 1;\n}\n";
    let (mut app, code) = app_with_one_violation(source);
    let root = code.ast.as_ref().unwrap().root_node();
    let entries = invariant_entries(&app.mapping, &code, &code).expect("checks run");
    assert_eq!(entries.len(), 1);
    app.modal = Some(Modal::InvariantList {
        entries,
        selected: 0,
    });
    let flat = FlatIndex::new(flatten_visible(root, &app.before.collapsed, None));
    let caches = rebuild_caches(&app.mapping.entries, root, root);

    handle_modal_key(
        &mut app,
        KeyCode::Enter,
        &flat,
        &flat,
        Some(root),
        Some(root),
        &caches,
        source.as_bytes(),
        source.as_bytes(),
        &code,
        &code,
    );

    let Some(Modal::TextView { state }) = &app.modal else {
        panic!("Enter should open the text view, got {:?}", app.modal);
    };
    assert_eq!(state.side, 0);
    assert_eq!(
        state.cursor[0],
        (1, 0),
        "the text cursor sits on the site's first byte"
    );
    let landed =
        find_node_by_id_anywhere(root, app.before.cursor_id).expect("the cursor is on a node");
    assert_eq!(
        (landed.kind(), &source[landed.byte_range()]),
        ("let", "let"),
        "column 0 is indentation, so the tree lands on the next leaf"
    );
    assert_eq!(app.focus, Focus::Before);
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Contracts of the flatten/navigate/stubs/actions/render helpers
// ─────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn algo_disagrees_never_flags_a_node_the_human_has_not_marked() {
    let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
    let (stmt_a, _) = two_statements(tree.root_node());
    let mut diff = ASTDiff::default();
    diff.before_node_map.insert(stmt_a.id(), 0);

    assert!(!algo_disagrees(
        Side::Before,
        stmt_a,
        &Caches::default(),
        &diff
    ));
}

#[test]
fn algo_disagrees_flags_a_match_to_a_different_partner() {
    let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
    let (stmt_a, stmt_b) = two_statements(tree.root_node());
    let mut caches = Caches::default();
    caches.before_match.insert(stmt_a.id(), stmt_a.id());
    let mut diff = ASTDiff::default();

    diff.before_node_map.insert(stmt_a.id(), stmt_a.id());
    assert!(!algo_disagrees(Side::Before, stmt_a, &caches, &diff));

    diff.before_node_map.insert(stmt_a.id(), stmt_b.id());
    assert!(
        algo_disagrees(Side::Before, stmt_a, &caches, &diff),
        "both sides say matched, but to different nodes"
    );
}

#[test]
fn advance_to_next_mismatch_wraps_around_in_both_directions() {
    let tree = parse_rust("fn main() {\n    a();\n    b();\n}\n");
    let root = tree.root_node();
    let (stmt_a, stmt_b) = two_statements(root);
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    // Stands in for "disagrees": true only on the first statement.
    fn is_first_statement(node: Node, _: &Caches, _: &ASTDiff) -> bool {
        node.kind() == "expression_statement" && node.prev_named_sibling().is_none()
    }

    let mut panel = PanelState::new(stmt_b.id());
    let found = advance_to_next_mismatch(
        &mut panel,
        &flat,
        &Caches::default(),
        &ASTDiff::default(),
        is_first_statement,
        true,
    );
    assert_eq!(
        found.map(|n| n.id()),
        Some(stmt_a.id()),
        "forward wraps past the end"
    );

    let mut panel = PanelState::new(root.id());
    let found = advance_to_next_mismatch(
        &mut panel,
        &flat,
        &Caches::default(),
        &ASTDiff::default(),
        is_first_statement,
        false,
    );
    assert_eq!(
        found.map(|n| n.id()),
        Some(stmt_a.id()),
        "backward wraps past the start"
    );
}

#[test]
fn advance_to_next_mismatch_leaves_the_cursor_put_when_nothing_disagrees() {
    let tree = parse_rust("fn main() {\n    a();\n}\n");
    let root = tree.root_node();
    let flat = FlatIndex::new(flatten_visible(
        root,
        &std::collections::HashSet::new(),
        None,
    ));
    let mut panel = PanelState::new(root.id());

    let found = advance_to_next_mismatch(
        &mut panel,
        &flat,
        &Caches::default(),
        &ASTDiff::default(),
        |_, _, _| false,
        true,
    );

    assert!(found.is_none());
    assert_eq!(panel.cursor_id, root.id());
}

#[test]
fn reveal_node_expands_collapsed_ancestors_and_centers_an_offscreen_target() {
    let source = (0..40)
        .map(|i| format!("    f{i}();\n"))
        .collect::<String>();
    let tree = parse_rust(&format!("fn main() {{\n{source}}}\n"));
    let root = tree.root_node();
    let block = find_first(root, "block").unwrap();
    let mut cursor = block.walk();
    let target = block
        .children(&mut cursor)
        .filter(|n| n.kind() == "expression_statement")
        .nth(30)
        .unwrap();

    let mut panel = PanelState::new(root.id());
    panel.viewport_height = 10;
    panel.collapsed.insert(block.id());

    let revealed = reveal_node(&mut panel, root, target.id());

    assert_eq!(revealed.map(|n| n.id()), Some(target.id()));
    assert_eq!(panel.cursor_id, target.id());
    assert!(!panel.collapsed.contains(&block.id()), "ancestor expanded");
    let flat = FlatIndex::new(flatten_visible(root, &panel.collapsed, None));
    let idx = flat.index_of(target.id()).unwrap();
    assert_eq!(panel.scroll, idx - 5, "target centered in the viewport");
}

#[test]
fn reveal_node_moves_nothing_for_an_id_not_in_the_tree() {
    let tree = parse_rust("fn main() {\n    a();\n}\n");
    let root = tree.root_node();
    let mut panel = PanelState::new(root.id());
    panel.scroll = 3;

    assert!(reveal_node(&mut panel, root, usize::MAX).is_none());
    assert_eq!(panel.cursor_id, root.id());
    assert_eq!(panel.scroll, 3);
}

#[test]
fn insert_use_line_leaves_a_file_that_already_has_the_import_untouched() {
    let line =
        "use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;\n";
    let once = insert_use_line(&stub_test_contents("rust-x", None, false), line);

    assert_eq!(insert_use_line(&once, line), once);
    assert_eq!(once.matches(line).count(), 1);
}

#[test]
fn delete_with_children_drops_a_prior_match_on_a_descendant() {
    let source = "fn main() {\n    a();\n}\n";
    let before_tree = parse_rust(source);
    let after_tree = parse_rust(source);
    let (before_root, after_root) = (before_tree.root_node(), after_tree.root_node());
    let before_stmt = find_first(before_root, "expression_statement").unwrap();
    let before_call = find_first(before_stmt, "call_expression").unwrap();
    let after_call = find_first(after_root, "call_expression").unwrap();
    let mut mapping = HumanMapping {
        entries: vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(before_call)),
            after_path: Some(path_for_node(after_call)),
        }],
        ..Default::default()
    };
    let flat = FlatIndex::new(flatten_visible(
        before_root,
        &std::collections::HashSet::new(),
        None,
    ));
    let caches = rebuild_caches(&mapping.entries, before_root, after_root);

    action_delete(
        &mut mapping,
        &flat,
        before_stmt.id(),
        before_root,
        after_root,
        true,
        &caches,
    )
    .unwrap();

    assert_eq!(mapping.entries.len(), 1, "{:?}", mapping.entries);
    assert_eq!(
        mapping.entries[0].operation,
        HumanOperation::DeleteWithChildren
    );
}

/// Promotion moves the note to the fixture's `description.md`, so the sample.csv row keeps no copy
/// that could drift from it.
#[test]
fn update_sample_csv_clears_the_rows_comment() {
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
    assert!(set_sample_comment_at(file.path(), &source, "a note").unwrap());

    assert!(update_sample_csv_at(file.path(), &source, "rust-new-case").unwrap());

    let rows = read_csv_with_status(file.path());
    assert_eq!(rows[0].4, "", "the promoted row must not keep the comment");
}

/// A sample's recorded dataset is checked, not trusted: a typo in source.json would otherwise
/// create a diffs/ folder nothing reads. The check runs before anything is written.
#[test]
fn action_promote_refuses_a_sample_whose_recorded_dataset_is_unknown() {
    let source = SampleSource {
        language: "Rust".to_string(),
        repository: "repo".to_string(),
        commit: "abc123".to_string(),
        path: "src/a.rs".to_string(),
        dataset: "not-a-dataset".to_string(),
    };
    let mut app = App::new(
        "sample-name".to_string(),
        CaseOrigin::Sample(source),
        0,
        0,
        HumanMapping::default(),
    );
    let name = "rust-promote-refused-for-unknown-dataset";

    let err = action_promote(&mut app, name, b"fn a() {}\n", b"fn b() {}\n").unwrap_err();

    assert!(format!("{err:#}").contains("not-a-dataset"), "{err:#}");
    assert!(diffs_case_dir(name).is_none(), "nothing may be written");
    assert!(matches!(app.origin, CaseOrigin::Sample(_)));
}

/// Esc in the text view backs out one step at a time - the live selection, then this side's
/// banked ranges - so an accidental `v` or a half-built N:M group does not close the view.
#[test]
fn esc_in_the_text_view_clears_the_selection_then_the_bank_before_closing() {
    let source = "let a = 1;\n";

    let (_, state) = press_in_text_view(
        source,
        source,
        paint_state_at(0, (0, 2), Some((0, 0))),
        KeyCode::Esc,
    );
    assert_eq!(state.anchor[0], None, "the first Esc clears the selection");

    let mut banked = paint_state_at(0, (0, 0), None);
    banked.pending[0].push(HumanTextSpan {
        start_row: 0,
        start_column: 0,
        end_row: 0,
        end_column: 3,
    });
    let (_, state) = press_in_text_view(source, source, banked, KeyCode::Esc);
    assert!(state.pending[0].is_empty(), "the next Esc clears the bank");
}

/// Esc while typing a painting name goes back to the picker's list rather than closing it, so a
/// mistyped name costs one key.
#[test]
fn esc_while_naming_a_painting_returns_to_the_picker_list() {
    let mut keys = vec![KeyCode::Char('j'); 5];
    keys.extend([KeyCode::Enter, KeyCode::Char('x'), KeyCode::Esc]);

    let app = press_in_solution_picker(&["Full"], &keys);

    assert!(
        matches!(
            &app.modal,
            Some(Modal::SolutionPicker { new_name: None, .. })
        ),
        "expected the picker list, got {:?}",
        app.modal
    );
}

/// `diff_case_has_text_mapping` searches the saved file for the quoted key, so a fixture `Z`
/// marked as having nothing to paint must still carry the key, and an unpainted one must not.
#[test]
fn the_paint_column_counts_a_deliberately_empty_painting_as_painted() {
    let mut app = test_app();
    let unpainted = serde_json::to_string(&app.mapping).unwrap();
    assert!(!unpainted.contains("\"text_mappings\""));

    action_paint_mark_empty(&mut app);

    let painted = serde_json::to_string(&app.mapping).unwrap();
    assert!(painted.contains("\"text_mappings\""));
}

#[test]
fn first_leaf_from_skips_whitespace_to_the_next_leaf_and_is_none_past_the_last_one() {
    let source = "fn main() {\n    a();\n}\n   \n";
    let tree = parse_rust(source);
    let root = tree.root_node();

    let in_indent = source.find("a();").unwrap() - 2;
    let leaf = first_leaf_from(root, in_indent).unwrap();
    assert_eq!(&source[leaf.byte_range()], "a");

    assert!(first_leaf_from(root, source.len() - 2).is_none());
}
