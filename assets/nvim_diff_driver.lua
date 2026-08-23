-- This file is part of the CodeDiff code diffing tool.
--
-- Copyright (C) 2026 Marko Ivankovic
--
-- This program is free software: you can redistribute it and/or modify
-- it under the terms of the GNU Affero General Public License as published
-- by the Free Software Foundation, either version 3 of the License, or
-- (at your option) any later version.
--
-- This program is distributed in the hope that it will be useful,
-- but WITHOUT ANY WARRANTY; without even the implied warranty of
-- MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
-- GNU Affero General Public License for more details.
--
-- You should have received a copy of the GNU Affero General Public License
-- along with this program.  If not, see <https://www.gnu.org/licenses/>.
--
-- Dumps Neovim's own diff classification for a two-window `nvim -d` session, as JSON on stdout:
--
--   [{"lines": [...], "subline": [...]}, {"lines": [...], "subline": [...]}]   -- [before, after]
--
-- `lines` is every 1-indexed line Neovim marks as part of the change (`diff_hlID` non-zero, i.e.
-- DiffAdd / DiffChange / DiffDelete / DiffText). `subline` is the subset of those lines carrying
-- at least one DiffText column - Neovim's *within-line* highlighting, which is the capability that
-- makes it interesting here and which no `git diff` row in this comparison has.
--
-- Why a Lua driver at all: Neovim's diff result is not written to any stream. It exists only as
-- window state, and `diff_hlID(lnum, col)` is the sole public way to read it back, per line and
-- per column. So the tool has to be driven headless and interrogated from inside.
--
-- `benchmark_other` always invokes this with `-u NONE`, so the measurement reflects Neovim's own
-- shipped defaults rather than whatever `diffopt` a developer happens to have configured. That
-- matters: `diffopt` can change the algorithm (`algorithm:histogram`) and the within-line
-- alignment (`linematch:N`), so a user config would silently make this a measurement of that
-- config instead of of Neovim.

local out = {}
for window = 1, 2 do
  vim.cmd(window == 1 and "wincmd t" or "wincmd b")
  local buf = vim.api.nvim_win_get_buf(0)
  local line_count = vim.api.nvim_buf_line_count(buf)
  local lines, subline = {}, {}
  for lnum = 1, line_count do
    if vim.fn.diff_hlID(lnum, 1) ~= 0 then
      lines[#lines + 1] = lnum
      local content = vim.api.nvim_buf_get_lines(buf, lnum - 1, lnum, false)[1] or ""
      for col = 1, #content do
        if vim.fn.synIDattr(vim.fn.diff_hlID(lnum, col), "name") == "DiffText" then
          subline[#subline + 1] = lnum
          break
        end
      end
    end
  end
  out[window] = { lines = lines, subline = subline }
end

io.stdout:write(vim.json.encode(out) .. "\n")
vim.cmd("qa!")
