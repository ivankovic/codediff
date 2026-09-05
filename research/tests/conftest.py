# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

"""Makes the report scripts (research/analysis/) and the repository's scripts/ importable as
plain modules. Both directories hold executables, not packages, and every one guards its `main`
behind `if __name__ == "__main__"`, so importing them runs nothing."""

import sys
from pathlib import Path

RESEARCH_DIR = Path(__file__).resolve().parents[1]
for directory in (RESEARCH_DIR / "analysis", RESEARCH_DIR.parent / "scripts"):
    if str(directory) not in sys.path:
        sys.path.insert(0, str(directory))
