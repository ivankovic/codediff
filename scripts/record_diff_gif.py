"""Record the GIF that contrasts GNU diff with codediff on one change.

Both halves are the *same* two files in the *same* viewer: only the ranges differ, because
`generate_showcase` bakes every case twice - once as codediff maps it, once as the real GNU `diff`
marks it (whole touched lines). That is what makes a wipe the right transition here: the code does
not move, so a bar sweeping across it swaps one reading for the other in place and the reader's eye
stays on the line it was already looking at.

Nothing here decides what to paint. `scripts/diff_gif_segments.js` runs the browser viewer's own
`assets/web/model.js` over each baked payload and hands back per-row coloured runs; this file only
turns those runs into pixels. See that script's header for why.

    make diff-gif

Fonts come from matplotlib's bundled DejaVu Sans Mono rather than from fontconfig, so the frames
are identical on any machine that can run this - a system font lookup would make the recording
depend on what happens to be installed.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

import matplotlib
from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The viewer's own chrome (assets/web/style.css `:root`).
PAGE_BG = "#1e1e1e"
PAGE_FG = "#d4d4d4"
CHROME = "#7f7f7f"

FONT_SIZE = 15
PAD = 12
GUTTER_COLS = 4
PANEL_GAP = 2
HEADER_H = 30
# The band above the panels carrying the tool's name. It is drawn at *both* ends of every frame so
# that a bar anywhere in the middle leaves one copy on each side: the reader sees "codediff" over
# the revealed half and "GNU diff" over the half still to be swept, which is the whole point of the
# transition. A single centred label would sit exactly under the bar at the halfway frame.
LABEL_H = 34
BAR_W = 3
# Frames for one sweep. The GIF plays the sweep out and back, so the file holds twice this many
# frames plus the two holds - the dominant term in its size, and the first thing to cut if it grows.
SWEEP_FRAMES = 34
# GIF stores delays in hundredths of a second, so anything not a multiple of 10 is floored on the
# way out - 45 would be written as 40. Named as what it actually produces.
FRAME_MS = 40
HOLD_MS = 1500
# Every frame is quantised against one shared palette rather than getting its own. The frames are
# two still images and a bar, so they draw on the same ~400 colours; one global colour table lets
# consecutive frames compress as deltas and takes the file from ~980 KiB to ~420 KiB. Measured at
# 48/64/96/128: 64 is where the antialiased text stops changing and the file is still small.
PALETTE_COLORS = 64


def font(bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "DejaVuSansMono-Bold.ttf" if bold else "DejaVuSansMono.ttf"
    path = pathlib.Path(matplotlib.get_data_path()) / "fonts" / "ttf" / name
    return ImageFont.truetype(str(path), FONT_SIZE)


def segments_for(payload: pathlib.Path, state: pathlib.Path) -> dict:
    """Per-row coloured runs, straight from the viewer's model."""
    out = subprocess.run(
        ["node", str(ROOT / "scripts" / "diff_gif_segments.js"), str(payload), str(state)],
        capture_output=True,
        check=True,
        text=True,
    )
    return json.loads(out.stdout)


def metrics() -> tuple[int, int]:
    """Cell width and row height for the monospace face."""
    regular = font()
    cell_w = round(regular.getlength("M"))
    ascent, descent = regular.getmetrics()
    return cell_w, ascent + descent + 3


def canvas_size(data: dict) -> tuple[int, int, int]:
    """`(width, height, panel_cols)` wide enough for the longest line on either side."""
    cell_w, row_h = metrics()
    rows = max(len(data["before"]["rows"]), len(data["after"]["rows"]))
    longest = max(
        (
            sum(len(run["text"]) for run in row)
            for side in ("before", "after")
            for row in data[side]["rows"]
        ),
        default=0,
    )
    panel_cols = GUTTER_COLS + 1 + longest
    width = PAD * 2 + panel_cols * cell_w * 2 + PANEL_GAP * cell_w
    height = PAD * 2 + LABEL_H + HEADER_H + rows * row_h
    return width, height, panel_cols


def draw_panel(draw: ImageDraw.ImageDraw, data: dict, side: str, x0: int, panel_cols: int) -> None:
    cell_w, row_h = metrics()
    regular, bold = font(), font(bold=True)
    palette = data["palette"]
    title_fg = palette["before_title_fg" if side == "before" else "after_title_fg"]

    y = PAD + LABEL_H
    draw.text((x0, y), data[side]["name"], font=bold, fill=title_fg)
    draw.text(
        (x0 + (len(data[side]["name"]) + 2) * cell_w, y),
        data[side]["language"],
        font=regular,
        fill=CHROME,
    )
    y += HEADER_H

    for number, row in enumerate(data[side]["rows"], start=1):
        draw.text((x0, y), f"{number:>{GUTTER_COLS}}", font=regular, fill=CHROME)
        x = x0 + (GUTTER_COLS + 1) * cell_w
        for run in row:
            width = len(run["text"]) * cell_w
            if run["bg"]:
                # Full row height, so consecutive painted rows read as one block the way they do
                # in the viewer rather than as separate stripes.
                draw.rectangle([x, y, x + width - 1, y + row_h - 1], fill=run["bg"])
            draw.text((x, y), run["text"], font=regular, fill=run["fg"] or PAGE_FG)
            x += width
        y += row_h


def render(data: dict, label: str, label_fg: str) -> Image.Image:
    width, height, panel_cols = canvas_size(data)
    cell_w, _ = metrics()
    image = Image.new("RGB", (width, height), PAGE_BG)
    draw = ImageDraw.Draw(image)
    draw_panel(draw, data, "before", PAD, panel_cols)
    draw_panel(draw, data, "after", PAD + (panel_cols + PANEL_GAP) * cell_w, panel_cols)
    # Both ends, so the label survives the wipe from either direction - see LABEL_H.
    bold = font(bold=True)
    draw.text((PAD, PAD), label, font=bold, fill=label_fg)
    draw.text((width - PAD - round(bold.getlength(label)), PAD), label, font=bold, fill=label_fg)
    return image


def frames(left: Image.Image, right: Image.Image) -> tuple[list[Image.Image], list[int]]:
    """A bar sweeping right then left; `left` is revealed behind it, `right` is what it covers."""
    width, height = left.size
    out: list[Image.Image] = []
    durations: list[int] = []
    positions = [round(width * i / SWEEP_FRAMES) for i in range(SWEEP_FRAMES + 1)]
    for sweep in (positions, list(reversed(positions))):
        for index, x in enumerate(sweep):
            frame = right.copy()
            if x > 0:
                frame.paste(left.crop((0, 0, x, height)), (0, 0))
            if 0 < x < width:
                ImageDraw.Draw(frame).rectangle([x - BAR_W, 0, x + BAR_W, height], fill="#00cdcd")
            out.append(frame)
            # Hold at each end so a reader can actually read the state before it is swept away.
            durations.append(HOLD_MS if index in (0, len(sweep) - 1) else FRAME_MS)
    return out, durations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--showcase", type=pathlib.Path, required=True, help="generate_showcase output directory"
    )
    parser.add_argument("--case", default="python-refactoring", help="case name to record")
    parser.add_argument("--out", type=pathlib.Path, required=True, help="GIF to write")
    args = parser.parse_args()

    state = args.showcase / "state.json"
    cases = args.showcase / "cases"
    codediff = segments_for(cases / f"{args.case}.codediff.json", state)
    unix = segments_for(cases / f"{args.case}.diff.json", state)

    if canvas_size(codediff) != canvas_size(unix):
        # Both bakes are the same two files, so this cannot differ unless the payloads are not a
        # pair - and a wipe between differently-sized images would silently misalign the code.
        print("error: the two bakes disagree about canvas size", file=sys.stderr)
        return 1

    palette = codediff["palette"]
    images, durations = frames(
        render(codediff, "codediff", palette["insert_bg"]),
        render(unix, "GNU diff", palette["delete_bg"]),
    )
    # One palette for the whole animation - see PALETTE_COLORS. `paste` stacks every frame into a
    # single image first so the palette is chosen over colours that actually appear, rather than
    # over whichever frame happened to be first.
    stacked = images[0].copy()
    for image in images[1:]:
        stacked.paste(image)
    shared = stacked.quantize(colors=PALETTE_COLORS, method=Image.Quantize.MAXCOVERAGE)
    quantized = [image.quantize(palette=shared, dither=Image.Dither.NONE) for image in images]

    args.out.parent.mkdir(parents=True, exist_ok=True)
    quantized[0].save(
        args.out,
        save_all=True,
        append_images=quantized[1:],
        duration=durations,
        loop=0,
        optimize=True,
    )
    size_kb = args.out.stat().st_size / 1024
    print(
        f"{args.out}: {len(images)} frames, {images[0].size[0]}x{images[0].size[1]}, {size_kb:.0f} KiB"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
