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
// Empty until the first `stratified` fixture is promoted (see `test::helper::DIFF_DATASETS` and
// `sample_test_diffs --stratified`) - `human_solver`'s `insert_mod_declaration` adds entries here
// the same way it does for `handmade.rs`/`small.rs`/`full.rs`.
#[cfg(test)]
mod c_freeciv_freeciv_update_version;
#[cfg(test)]
mod c_genymobile_scrcpy_add_a_define;
#[cfg(test)]
mod c_genymobile_scrcpy_add_a_define_;
#[cfg(test)]
mod c_genymobile_scrcpy_add_a_define_2;
#[cfg(test)]
mod c_genymobile_scrcpy_add_to_import_path_and_move_imports_around;
#[cfg(test)]
mod c_genymobile_scrcpy_big_change;
#[cfg(test)]
mod c_genymobile_scrcpy_rename_and_add_a_define;
#[cfg(test)]
mod c_genymobile_scrcpy_rename_defines;
#[cfg(test)]
mod c_htop_dev_htop_add_comment;
#[cfg(test)]
mod c_htop_dev_htop_add_function;
#[cfg(test)]
mod c_htop_dev_htop_update_import;
#[cfg(test)]
mod c_ladybirdbrowser_ladybird_change_to_a_different_class;
#[cfg(test)]
mod c_ladybirdbrowser_ladybird_move_to_a_different_class;
#[cfg(test)]
mod c_microsoft_terminal_add_two_includes;
#[cfg(test)]
mod c_microsoft_terminal_change_import_path;
#[cfg(test)]
mod c_mozilla_firefox_firefox_remove_two_comments;
#[cfg(test)]
mod c_neovim_neovim_add_an_include;
#[cfg(test)]
mod c_neovim_neovim_small_change;
#[cfg(test)]
mod c_ollama_ollama_change_imports;
#[cfg(test)]
mod c_openssl_openssl_add_import;
#[cfg(test)]
mod c_openssl_openssl_add_two_clang_comments;
#[cfg(test)]
mod c_openssl_openssl_format_only_change;
#[cfg(test)]
mod c_openssl_openssl_whitepsace_only;
#[cfg(test)]
mod c_openssl_openssl_whitespace_only;
#[cfg(test)]
mod c_postgres_postgres_copyright_year_update;
#[cfg(test)]
mod c_postgres_postgres_copyright_year_update_2;
#[cfg(test)]
mod c_postgres_postgres_copyright_year_update_3;
#[cfg(test)]
mod c_postgres_postgres_preprocessor_heavy_change;
#[cfg(test)]
mod c_postgres_postgres_update_copyright_year;
#[cfg(test)]
mod c_rust_lang_rust_add_two_consts;
#[cfg(test)]
mod cpp_electron_electron_add_imports;
#[cfg(test)]
mod cpp_godotengine_godot_add_include;
#[cfg(test)]
mod cpp_ladybirdbrowser_ladybird_add_real_logic;
#[cfg(test)]
mod cpp_ladybirdbrowser_ladybird_change_inherited_class_name;
#[cfg(test)]
mod cpp_libreoffice_add_const_2;
#[cfg(test)]
mod cpp_libreoffice_add_imports_and_function_param;
#[cfg(test)]
mod cpp_libreoffice_delete_function;
#[cfg(test)]
mod cpp_mozilla_firefox_firefox_update_file_comment;
#[cfg(test)]
mod cpp_mozilla_firefox_firefox_update_file_comment_2;
#[cfg(test)]
mod cpp_nzbgetcom_nzbget_add_include;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_2;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_3;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_4;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_5;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_6;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_7;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_8;
#[cfg(test)]
mod cpp_ollama_ollama_update_commit_hash_string_constant;
#[cfg(test)]
mod cpp_opencv_opencv_delete_string_const_from_preprocessor;
#[cfg(test)]
mod cpp_paddlepaddle_paddleocr_add_namespace_closing_comment;
#[cfg(test)]
mod cpp_protocolbuffers_protobuf_add_preprocessor_commands;
#[cfg(test)]
mod csharp_jellyfin_jellyfin_update_version_string;
#[cfg(test)]
mod csharp_lidarr_lidarr_add_enum_value;
#[cfg(test)]
mod csharp_lidarr_lidarr_add_function;
#[cfg(test)]
mod csharp_lidarr_lidarr_add_function_signature_to_class;
#[cfg(test)]
mod csharp_lidarr_lidarr_add_import_and_func;
#[cfg(test)]
mod csharp_lidarr_lidarr_add_method_to_class;
#[cfg(test)]
mod csharp_radarr_radarr_add_base_class;
#[cfg(test)]
mod csharp_radarr_radarr_add_func;
#[cfg(test)]
mod csharp_radarr_radarr_remove_import_and_func;
#[cfg(test)]
mod csharp_sonarr_sonarr_add_attribute;
#[cfg(test)]
mod csharp_sonarr_sonarr_add_attribute_2;
#[cfg(test)]
mod csharp_sonarr_sonarr_add_func;
#[cfg(test)]
mod csharp_sonarr_sonarr_add_import_and_annotation;
#[cfg(test)]
mod csharp_sonarr_sonarr_add_list_item;
#[cfg(test)]
mod csharp_sonarr_sonarr_delete_func;
#[cfg(test)]
mod csharp_sonarr_sonarr_use_a_different_class;
#[cfg(test)]
mod css_wordpress_wordpress_go_to_one_line;
#[cfg(test)]
mod css_wordpress_wordpress_re_format_in_one_line;
#[cfg(test)]
mod css_wordpress_wordpress_reformat_and_fix_lint_errors;
#[cfg(test)]
mod css_wordpress_wordpress_remove_one_rule;
#[cfg(test)]
mod css_wordpress_wordpress_remove_webkit_prefix;
#[cfg(test)]
mod go_fatedier_frp_build_comment_insert_only;
#[cfg(test)]
mod go_gin_gonic_gin_one_space_removed_in_a_comment;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_2;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_3;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_4;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_5;
#[cfg(test)]
mod go_gin_gonic_gin_update_version_string_6;
#[cfg(test)]
mod go_gohugoio_hugo_add_and_upadate_list_items;
#[cfg(test)]
mod go_gohugoio_hugo_update_and_add_list_items;
#[cfg(test)]
mod go_gohugoio_hugo_update_and_add_some_values;
#[cfg(test)]
mod go_gohugoio_hugo_version;
#[cfg(test)]
mod go_golang_go_update_copyright_year;
#[cfg(test)]
mod go_grafana_grafana_real_small_change_with_a_move;
#[cfg(test)]
mod go_jesseduffield_lazygit_add_a_func;
#[cfg(test)]
mod go_junegunn_fzf_real_small_change;
#[cfg(test)]
mod go_ollama_ollama_add_go_build_comment;
#[cfg(test)]
mod go_ollama_ollama_remove_go_build_comment;
#[cfg(test)]
mod go_prometheus_prometheus_remove_copyright_year;
#[cfg(test)]
mod html_axios_axios_add_script_element;
#[cfg(test)]
mod html_fatedier_frp_update_hashes;
#[cfg(test)]
mod html_fatedier_frp_update_hashes_2;
#[cfg(test)]
mod html_fatedier_frp_update_hashes_3;
#[cfg(test)]
mod html_fatedier_frp_update_hashes_4;
#[cfg(test)]
mod html_fatedier_frp_version;
#[cfg(test)]
mod html_gohugoio_hugo_template_not_pure_html;
#[cfg(test)]
mod html_gohugoio_hugo_template_not_pure_html_2;
#[cfg(test)]
mod html_gohugoio_hugo_update_href_template;
#[cfg(test)]
mod html_ladybirdbrowser_ladybird_remove_meta_attribute;
#[cfg(test)]
mod html_ladybirdbrowser_ladybird_update_pixel_value;
#[cfg(test)]
mod html_ladybirdbrowser_ladybird_update_two_pixel_numbers;
#[cfg(test)]
mod html_mozilla_firefox_firefox_href_path;
#[cfg(test)]
mod html_mozilla_firefox_firefox_interesting_case;
#[cfg(test)]
mod html_mozilla_firefox_firefox_path;
#[cfg(test)]
mod html_mozilla_firefox_firefox_test_span;
#[cfg(test)]
mod html_mozilla_pdf_add_closing_tags;
#[cfg(test)]
mod html_pandas_dev_pandas_release_banner_update;
#[cfg(test)]
mod html_prettier_prettier_not_pure_html_includes_yaml_as_well;
#[cfg(test)]
mod html_twbs_bootstrap_not_html_template_extract_two_vars;
#[cfg(test)]
mod java_genymobile_scrcpy_add_enum_value;
#[cfg(test)]
mod java_genymobile_scrcpy_add_func;
#[cfg(test)]
mod java_genymobile_scrcpy_add_func_2;
#[cfg(test)]
mod java_genymobile_scrcpy_add_parameter;
#[cfg(test)]
mod java_genymobile_scrcpy_char_to_string_bugfix;
#[cfg(test)]
mod java_genymobile_scrcpy_only_insert;
#[cfg(test)]
mod java_genymobile_scrcpy_whitespace_only;
#[cfg(test)]
mod java_paddlepaddle_paddleocr_whitespace_only;
#[cfg(test)]
mod java_paddlepaddle_paddleocr_whitespace_only_2;
#[cfg(test)]
mod java_protocolbuffers_protobuf_update_comment;
#[cfg(test)]
mod java_protocolbuffers_protobuf_update_comment_2;
#[cfg(test)]
mod javascript_axios_axios_real_small_change;
#[cfg(test)]
mod javascript_facebook_react_update_comment_only;
#[cfg(test)]
mod javascript_microsoft_typescript_add_use_strict;
#[cfg(test)]
mod javascript_microsoft_typescript_add_use_strict_2;
#[cfg(test)]
mod javascript_microsoft_typescript_concat_to_template;
#[cfg(test)]
mod javascript_mozilla_firefox_firefox_remove_one_comment;
#[cfg(test)]
mod javascript_mui_material_ui_delete_one_import;
#[cfg(test)]
mod javascript_vercel_next_add_doccomment;
#[cfg(test)]
mod json_apache_superset_js_to_ts;
#[cfg(test)]
mod json_gorhill_ublock_version;
#[cfg(test)]
mod json_grafana_grafana_add_pair;
#[cfg(test)]
mod json_microsoft_playwright_version_update;
#[cfg(test)]
mod json_puppeteer_puppeteer_update_version;
#[cfg(test)]
mod json_puppeteer_puppeteer_version_update;
#[cfg(test)]
mod json_puppeteer_puppeteer_version_update_2;
#[cfg(test)]
mod json_vercel_next_version;
#[cfg(test)]
mod kotlin_mozilla_firefox_firefox_rename;
#[cfg(test)]
mod kotlin_nextcloud_android_add_param;
#[cfg(test)]
mod kotlin_nextcloud_android_add_param_to_class;
#[cfg(test)]
mod kotlin_nextcloud_android_different_function;
#[cfg(test)]
mod kotlin_nextcloud_android_real_small_change;
#[cfg(test)]
mod kotlin_nextcloud_android_remove_function;
#[cfg(test)]
mod kotlin_nextcloud_android_rename;
#[cfg(test)]
mod kotlin_nextcloud_android_rename_field;
#[cfg(test)]
mod kotlin_nextcloud_android_small_change;
#[cfg(test)]
mod kotlin_rustdesk_rustdesk_add_func;
#[cfg(test)]
mod lua_awesomewm_awesome_add_comment;
#[cfg(test)]
mod lua_awesomewm_awesome_add_func;
#[cfg(test)]
mod lua_awesomewm_awesome_add_func_call;
#[cfg(test)]
mod lua_awesomewm_awesome_add_h_to_align;
#[cfg(test)]
mod lua_awesomewm_awesome_add_to_table_constructor;
#[cfg(test)]
mod lua_awesomewm_awesome_comment_only;
#[cfg(test)]
mod lua_awesomewm_awesome_comment_only_2;
#[cfg(test)]
mod lua_awesomewm_awesome_halign;
#[cfg(test)]
mod lua_awesomewm_awesome_insert;
#[cfg(test)]
mod lua_awesomewm_awesome_insert_only;
#[cfg(test)]
mod lua_awesomewm_awesome_update_comment;
#[cfg(test)]
mod lua_neovim_neovim_rename;
#[cfg(test)]
mod php_nextcloud_server_add_a_few_types;
#[cfg(test)]
mod php_nextcloud_server_add_declare;
#[cfg(test)]
mod php_nextcloud_server_add_declare_2;
#[cfg(test)]
mod php_nextcloud_server_add_declare_3;
#[cfg(test)]
mod php_nextcloud_server_add_declare_4;
#[cfg(test)]
mod php_nextcloud_server_add_declare_5;
#[cfg(test)]
mod php_nextcloud_server_add_declare_6;
#[cfg(test)]
mod php_nextcloud_server_add_declare_7;
#[cfg(test)]
mod php_nextcloud_server_add_readonly;
#[cfg(test)]
mod php_nextcloud_server_real_small_change;
#[cfg(test)]
mod php_wordpress_wordpress_one_line_file_insert_and_update;
#[cfg(test)]
mod php_wordpress_wordpress_one_line_file_with_real_insert_and_update;
#[cfg(test)]
mod php_wordpress_wordpress_version;
#[cfg(test)]
mod php_wordpress_wordpress_version_2;
#[cfg(test)]
mod php_wordpress_wordpress_version_3;
#[cfg(test)]
mod php_wordpress_wordpress_version_4;
#[cfg(test)]
mod php_wordpress_wordpress_version_5;
#[cfg(test)]
mod php_wordpress_wordpress_version_6;
#[cfg(test)]
mod php_wordpress_wordpress_version_7;
#[cfg(test)]
mod php_wordpress_wordpress_version_8;
#[cfg(test)]
mod python_ansible_ansible_add_comment;
#[cfg(test)]
mod python_langchain_ai_langchain_version_change;
#[cfg(test)]
mod python_langflow_ai_langflow_bob;
#[cfg(test)]
mod python_langflow_ai_langflow_real_small_change;
#[cfg(test)]
mod python_nvbn_thefuck_add_three_arguments;
#[cfg(test)]
mod python_nvbn_thefuck_small_change;
#[cfg(test)]
mod python_nvbn_thefuck_small_change_2;
#[cfg(test)]
mod python_nvbn_thefuck_small_change_3;
#[cfg(test)]
mod python_nvbn_thefuck_stdout_stderr_change;
#[cfg(test)]
mod python_odoo_odoo_add_import;
#[cfg(test)]
mod python_odoo_odoo_add_import_2;
#[cfg(test)]
mod python_odoo_odoo_add_two_imports;
#[cfg(test)]
mod python_odoo_odoo_version;
#[cfg(test)]
mod python_openhands_openhands_small_change;
#[cfg(test)]
mod python_paddlepaddle_paddleocr_remove_import;
#[cfg(test)]
mod python_paddlepaddle_paddleocr_whitespace_only;
#[cfg(test)]
mod python_paddlepaddle_paddleocr_whitespace_only_change;
#[cfg(test)]
mod python_scrapy_scrapy_comment_update;
#[cfg(test)]
mod python_ytdl_org_youtube_dl_version_update;
#[cfg(test)]
mod python_ytdl_org_youtube_dl_version_update_2;
#[cfg(test)]
mod ruby_homebrew_brew_version;
#[cfg(test)]
mod ruby_jekyll_jekyll_version;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_2;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_3;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_4;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_5;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_6;
#[cfg(test)]
mod ruby_jekyll_jekyll_version_7;
#[cfg(test)]
mod ruby_jekyll_jekyll_whitespace_only;
#[cfg(test)]
mod ruby_jekyll_jekyll_whitespace_only_2;
#[cfg(test)]
mod ruby_jekyll_jekyll_whitespace_only_3;
#[cfg(test)]
mod ruby_mastodon_mastodon_add_func_and_attribute;
#[cfg(test)]
mod ruby_mastodon_mastodon_add_method;
#[cfg(test)]
mod ruby_mastodon_mastodon_add_one_line;
#[cfg(test)]
mod ruby_mastodon_mastodon_insert_only;
#[cfg(test)]
mod ruby_mastodon_mastodon_move;
#[cfg(test)]
mod ruby_mastodon_mastodon_normal_change;
#[cfg(test)]
mod ruby_mastodon_mastodon_one_operator;
#[cfg(test)]
mod ruby_mastodon_mastodon_rare_example_of_true_move;
#[cfg(test)]
mod ruby_mastodon_mastodon_smal_change;
#[cfg(test)]
mod rust_gyulyvgc_sniffnet_add_mod;
#[cfg(test)]
mod rust_gyulyvgc_sniffnet_add_mod_2;
#[cfg(test)]
mod rust_gyulyvgc_sniffnet_add_mod_3;
#[cfg(test)]
mod rust_gyulyvgc_sniffnet_remoev_attribute;
#[cfg(test)]
mod rust_rust_lang_rust_add_note_comment;
#[cfg(test)]
mod rust_rust_lang_rust_change_use;
#[cfg(test)]
mod rust_rust_lang_rust_remove_min_version_comment;
#[cfg(test)]
mod rust_rust_lang_rust_remove_starting_comment;
#[cfg(test)]
mod rust_rust_lang_rust_remove_warn_comment;
#[cfg(test)]
mod rust_rust_lang_rust_remove_warn_comment_2;
#[cfg(test)]
mod rust_rust_lang_rust_remove_warn_comment_3;
#[cfg(test)]
mod rust_rust_lang_rust_remove_warn_comments;
#[cfg(test)]
mod rust_rust_lang_rust_update_comment;
#[cfg(test)]
mod rust_tauri_apps_tauri_add_use_and_function;
#[cfg(test)]
mod rust_tauri_apps_tauri_rename_mod;
#[cfg(test)]
mod rust_vercel_next_add_mode;
#[cfg(test)]
mod rust_vercel_next_remove_mod;
#[cfg(test)]
mod rust_zed_industries_zed_add_mod;
#[cfg(test)]
mod rust_zed_industries_zed_change_mod_and_use;
#[cfg(test)]
mod rust_zed_industries_zed_change_mods;
#[cfg(test)]
mod shellscript_ansible_ansible_a_small_add;
#[cfg(test)]
mod shellscript_ansible_ansible_add_commadn;
#[cfg(test)]
mod shellscript_ansible_ansible_only_insert;
#[cfg(test)]
mod shellscript_ansible_ansible_small_add;
#[cfg(test)]
mod shellscript_ansible_ansible_small_change;
#[cfg(test)]
mod shellscript_genymobile_scrcpy_version;
#[cfg(test)]
mod shellscript_jesseduffield_lazygit_version;
#[cfg(test)]
mod shellscript_microsoft_playwright_version;
#[cfg(test)]
mod shellscript_openhands_openhands_update_string_value;
#[cfg(test)]
mod shellscript_paddlepaddle_paddleocr_insert_inside_a_string;
#[cfg(test)]
mod shellscript_stgpetrovic_stacuist_pure_add;
#[cfg(test)]
mod shellscript_stgpetrovic_stacuist_pure_add_2;
#[cfg(test)]
mod shellscript_vercel_next_change_command_params;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_1;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_2;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_4;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_5;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_6;
#[cfg(test)]
mod swift_swiftlang_swift_add_to_typecheck_comment_7;
#[cfg(test)]
mod swift_swiftlang_swift_delete_and_insert_in_the_typecheck_comment;
#[cfg(test)]
mod swift_swiftlang_swift_update_leading_comment;
#[cfg(test)]
mod swift_swiftlang_swift_update_typecheck_comment;
#[cfg(test)]
mod tsx_excalidraw_excalidraw_add_type_to_import;
#[cfg(test)]
mod tsx_langflow_ai_langflow_add_type_to_import;
#[cfg(test)]
mod tsx_langflow_ai_langflow_split_import;
#[cfg(test)]
mod tsx_langflow_ai_langflow_split_import_2;
#[cfg(test)]
mod tsx_langflow_ai_langflow_split_import_3;
#[cfg(test)]
mod tsx_mui_material_ui_add_to_empty_block;
#[cfg(test)]
mod tsx_mui_material_ui_remove_import;
#[cfg(test)]
mod tsx_mui_material_ui_remove_import_2;
#[cfg(test)]
mod tsx_mui_material_ui_remove_import_3;
#[cfg(test)]
mod tsx_mui_material_ui_remove_import_4;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target_2;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target_3;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target_4;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target_5;
#[cfg(test)]
mod typescript_microsoft_typescript_add_es_target_6;
#[cfg(test)]
mod typescript_microsoft_typescript_add_eslint;
#[cfg(test)]
mod typescript_microsoft_typescript_add_strict;
#[cfg(test)]
mod typescript_microsoft_typescript_add_strict_2;
#[cfg(test)]
mod typescript_microsoft_typescript_add_strict_false;
#[cfg(test)]
mod xml_genymobile_scrcpy_remove_package_attribute;
#[cfg(test)]
mod xml_jellyfin_jellyfin_update_attribute_values;
#[cfg(test)]
mod xml_mozilla_firefox_firefox_update_value;
#[cfg(test)]
mod xml_paddlepaddle_paddleocr_whitespace_only_change;
#[cfg(test)]
mod yaml_ansible_ansible_add_block_sequence;
#[cfg(test)]
mod yaml_ansible_ansible_add_item_to_sequence;
#[cfg(test)]
mod yaml_ansible_ansible_add_item_to_sequence_2;
#[cfg(test)]
mod yaml_ansible_ansible_add_mapping_pair;
#[cfg(test)]
mod yaml_ansible_ansible_change_in_string_scalar;
#[cfg(test)]
mod yaml_ansible_ansible_rename_string_scalar;
#[cfg(test)]
mod yaml_ansible_ansible_version;
#[cfg(test)]
mod yaml_gyulyvgc_sniffnet_version;
#[cfg(test)]
mod yaml_jekyll_jekyll_true_to_false;
#[cfg(test)]
mod yaml_puppeteer_puppeteer_false_to_true;
