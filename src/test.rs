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
use anyhow::Result;

pub mod helper;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handmade_test_code_loads() -> Result<()> {
        let test_codes = helper::handmade_test_code()?;

        assert!(!test_codes.is_empty());

        Ok(())
    }

    #[test]
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
