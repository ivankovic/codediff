#!/bin/bash
#
#  This file is part of the CodeDiff code diffing tool.
#
#  Copyright (C) 2025 Marko Ivankovic
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU Affero General Public License as published
#  by the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
case "$1" in
  rust-*)   lang="rs" ;;
  python-*) lang="py" ;;
  java-*)   lang="java" ;;
  c-*)      lang="c" ;;
  cpp-*)    lang="cpp" ;;
  go-*)     lang="go" ;;
  js-*)     lang="js" ;;
  javascript-*)     lang="js" ;;
  ts-*)     lang="ts" ;;
  typescript-*)     lang="ts" ;;
  ruby-*)   lang="rb" ;;
  swift-*)  lang="swift" ;;
  kotlin-*) lang="kt" ;;
  scala-*)  lang="scala" ;;
  *)        lang="unknown" ;;
esac

nvim -d "./src/test/data/diffs/$1/before.$lang.test" "./src/test/data/diffs/$1/after.$lang.test"
