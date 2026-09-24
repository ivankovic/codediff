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

"""Rasterize a viewer still into the README's screenshot.

`render_tui_screenshot` (src/bin/) draws the frame with the TUI's own widgets into an offscreen
terminal and prints it as styled runs; this file only turns those runs into pixels. Nothing here
decides what is painted where, for the same reason scripts/record_diff_gif.py leaves that to the
viewer: a second implementation would drift from the product without anyone noticing.

    make readme-screenshot

Fonts come from matplotlib's bundled DejaVu Sans Mono, the GIF's face, so the two README images
match and the output is identical on any machine that can run this.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import matplotlib
from PIL import Image, ImageDraw, ImageFont

FONT_SIZE = 15
PAD = 12
# Solarized base3 and base01: what a light terminal shows for cells that carry no colour of their
# own. The bin leaves those `null`, since an offscreen terminal has no defaults to inherit.
DEFAULT_BG = "#fdf6e3"
DEFAULT_FG = "#586e75"
# How far a dim run's foreground moves toward the background.
DIM_BLEND = 0.5

FACES = {
    (False, False): "DejaVuSansMono.ttf",
    (True, False): "DejaVuSansMono-Bold.ttf",
    (False, True): "DejaVuSansMono-Oblique.ttf",
    (True, True): "DejaVuSansMono-BoldOblique.ttf",
}


def font(bold: bool = False, italic: bool = False) -> ImageFont.FreeTypeFont:
    path = pathlib.Path(matplotlib.get_data_path()) / "fonts" / "ttf" / FACES[(bold, italic)]
    return ImageFont.truetype(str(path), FONT_SIZE)


def metrics() -> tuple[int, int, int]:
    """Cell width, row height and baseline offset for the monospace face."""
    regular = font()
    cell_w = round(regular.getlength("M"))
    ascent, descent = regular.getmetrics()
    return cell_w, ascent + descent + 2, 1


def rgb(color: str) -> tuple[int, int, int]:
    return tuple(int(color[i : i + 2], 16) for i in (1, 3, 5))  # type: ignore[return-value]


def blend(
    color: tuple[int, int, int], toward: tuple[int, int, int], t: float
) -> tuple[int, int, int]:
    return tuple(round(a + (b - a) * t) for a, b in zip(color, toward))  # type: ignore[return-value]


def render(still: dict, background: str, foreground: str) -> Image.Image:
    cell_w, row_h, baseline = metrics()
    width = still["cols"] * cell_w + 2 * PAD
    height = still["rows"] * row_h + 2 * PAD
    page_bg = rgb(background)
    page_fg = rgb(foreground)
    image = Image.new("RGB", (width, height), page_bg)
    draw = ImageDraw.Draw(image)
    fonts: dict[tuple[bool, bool], ImageFont.FreeTypeFont] = {}

    for row, runs in enumerate(still["lines"]):
        y = PAD + row * row_h
        x = PAD
        for run in runs:
            cells = len(run["text"])
            run_w = cells * cell_w
            bg = rgb(run["bg"]) if run["bg"] else page_bg
            fg = rgb(run["fg"]) if run["fg"] else page_fg
            if run["dim"]:
                fg = blend(fg, bg, DIM_BLEND)
            if bg != page_bg:
                draw.rectangle((x, y, x + run_w - 1, y + row_h - 1), fill=bg)
            face = (run["bold"], run["italic"])
            if face not in fonts:
                fonts[face] = font(*face)
            if run["text"].strip():
                draw.text((x, y + baseline), run["text"], font=fonts[face], fill=fg)
            if run["underline"]:
                line_y = y + row_h - 2
                draw.line((x, line_y, x + run_w - 1, line_y), fill=fg)
            x += run_w
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument(
        "--still", type=pathlib.Path, required=True, help="JSON from render_tui_screenshot"
    )
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument(
        "--background", default=DEFAULT_BG, help="page colour for unset backgrounds"
    )
    parser.add_argument(
        "--foreground", default=DEFAULT_FG, help="text colour for unset foregrounds"
    )
    args = parser.parse_args()

    still = json.loads(args.still.read_text())
    image = render(still, args.background, args.foreground)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.out, optimize=True)
    print(f"wrote {args.out} ({image.width}x{image.height})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
