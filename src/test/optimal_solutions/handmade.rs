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
#[cfg(test)]
mod cpp_add_const_correctness;
#[cfg(test)]
mod cpp_add_memory_management;
#[cfg(test)]
mod cpp_add_templates;
#[cfg(test)]
mod cpp_fix_segfault;
#[cfg(test)]
mod cpp_optimize_algorithm;
#[cfg(test)]
mod java_add_exception_handling;
#[cfg(test)]
mod java_add_interface;
#[cfg(test)]
mod java_add_logging;
#[cfg(test)]
mod java_fix_array_index;
#[cfg(test)]
mod java_refactor_constants;
#[cfg(test)]
mod javascript_add_array_method;
#[cfg(test)]
mod javascript_add_destructuring;
#[cfg(test)]
mod javascript_add_event_listener;
#[cfg(test)]
mod javascript_fix_promises;
#[cfg(test)]
mod javascript_refactor_arrow_func;
#[cfg(test)]
mod kotlin_add_data_class;
#[cfg(test)]
mod kotlin_add_null_check;
#[cfg(test)]
mod kotlin_add_validation;
#[cfg(test)]
mod kotlin_fix_loop_bug;
#[cfg(test)]
mod kotlin_refactor_function;
#[cfg(test)]
mod python_add_remove_block;
#[cfg(test)]
mod python_added_if_block;
#[cfg(test)]
mod python_added_if_block_small;
#[cfg(test)]
mod python_api_change;
#[cfg(test)]
mod python_bugfix_loop;
#[cfg(test)]
mod python_refactoring;
#[cfg(test)]
mod rust_add_comments_and_real_new_logic;
#[cfg(test)]
mod rust_add_if;
#[cfg(test)]
mod rust_add_to_existing_use;
#[cfg(test)]
mod rust_add_value_to_enum;
#[cfg(test)]
mod rust_adding_many_identical_cfg_test_statements_to_a_signle_file_doesnt_prefer_the_local_insert_but_rather_goes_to_some_other_existing_cfg;
#[cfg(test)]
mod rust_adding_to_a_list_of_identical_attributes_should_favour_near_matches;
#[cfg(test)]
mod rust_algorithm_change;
#[cfg(test)]
mod rust_cost_optimization;
#[cfg(test)]
mod rust_data_structure;
#[cfg(test)]
mod rust_error_handling;
#[cfg(test)]
mod rust_firefox_webrenderer_borders;
#[cfg(test)]
mod rust_hash_optimization;
#[cfg(test)]
mod rust_hello_world_added_message;
#[cfg(test)]
mod rust_hello_world_removed_message;
#[cfg(test)]
mod rust_leetcode_1_bugfix;
#[cfg(test)]
mod rust_multi_map_duplicate_calls;
#[cfg(test)]
mod rust_next_font_imports_generator;
#[cfg(test)]
mod rust_no_change;
#[cfg(test)]
mod rust_real_logic_change_in_a_huge_75k_node_file;
#[cfg(test)]
mod rust_small_addition_with_reuse_of_binary_expressions;
#[cfg(test)]
mod rust_sniffnet_protocol;
#[cfg(test)]
mod rust_tauri_api_build_1;
#[cfg(test)]
mod rust_tauri_api_build_2;
#[cfg(test)]
mod rust_tauri_cli_ios_dev;
#[cfg(test)]
mod rust_turbopack_module_rule;
#[cfg(test)]
mod rust_turbopack_persistence_tools_main;
#[cfg(test)]
mod rust_zed_git_panel_settings;
#[cfg(test)]
mod rust_zed_workspace_tasks;
#[cfg(test)]
mod typescript_add_error_handling;
#[cfg(test)]
mod typescript_add_generics;
#[cfg(test)]
mod typescript_add_type_annotations;
#[cfg(test)]
mod typescript_async_await;
#[cfg(test)]
mod typescript_refactor_interface;
