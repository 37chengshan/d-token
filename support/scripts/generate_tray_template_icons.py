"""Draw the d-token menu-bar template icon at native pixel sizes.

Menu-bar glyphs are tiny, so the shape is authored per-pixel at 16px and 32px
rather than downscaled from the full-colour brand mark, whose thin strokes turn
to grey mush at this size.

The mark reads as compression: two solid wedges closing inward on a fixed centre
axis. '#' is solid ink, '+' a half-coverage edge, '.' a light antialiasing tap,
and space is empty.

Regenerate with:
    python3 support/scripts/generate_tray_template_icons.py \\
        code/apps/desktop/src-tauri/icons/iconTemplate.png \\
        code/apps/desktop/src-tauri/icons/iconTemplate@2x.png
"""
import struct
import sys
import zlib

# 16x16 — the 1x menu-bar slot. Apple's guidance leaves the outer ring clear,
# so the glyph occupies rows 3..12 and stays inside the horizontal margins.
GLYPH_16 = [
    "                ",
    "                ",
    "                ",
    "  #     ##     #",
    "  ##    ##    ##",
    "  ###   ##   ###",
    "  ####  ##  ####",
    "  ##### ## #####",
    "  ##### ## #####",
    "  ####  ##  ####",
    "  ###   ##   ###",
    "  ##    ##    ##",
    "  #     ##     #",
    "                ",
    "                ",
    "                ",
]

# 32x32 — the 2x slot. Same silhouette at double the stroke weight.
GLYPH_32 = [
    "                                ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
    "    ##        ####        ##    ",
    "    ###       ####       ###    ",
    "    ####      ####      ####    ",
    "    #####     ####     #####    ",
    "    ######    ####    ######    ",
    "    #######   ####   #######    ",
    "    ########  ####  ########    ",
    "    ######### #### #########    ",
    "    ########## #### ########    ",
    "    ########## #### ########    ",
    "    ########## #### ########    ",
    "    ########## #### ########    ",
    "    ######### #### #########    ",
    "    ########  ####  ########    ",
    "    #######   ####   #######    ",
    "    ######    ####    ######    ",
    "    #####     ####     #####    ",
    "    ####      ####      ####    ",
    "    ###       ####       ###    ",
    "    ##        ####        ##    ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
]

COVERAGE = {" ": 0, ".": 64, "+": 128, "#": 255}


def rasterize(glyph, size):
    if len(glyph) != size:
        raise SystemExit(f"glyph has {len(glyph)} rows, expected {size}")
    rows = []
    for index, line in enumerate(glyph):
        if len(line) != size:
            raise SystemExit(f"glyph row {index} has {len(line)} cols, expected {size}")
        rows.append([COVERAGE[ch] for ch in line])
    return rows


def write_template_png(path, size, alpha_rows):
    raw = bytearray()
    for line in alpha_rows:
        raw.append(0)
        for a in line:
            # Template images must be pure black; macOS inverts via alpha.
            raw += bytes((0, 0, 0, a))

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(png)


def preview(rows):
    for line in rows:
        print("   " + "".join("#" if v > 200 else ("+" if v > 96 else ("." if v > 32 else " ")) for v in line))


def main():
    out_1x, out_2x = sys.argv[1], sys.argv[2]
    rows16 = rasterize(GLYPH_16, 16)
    rows32 = rasterize(GLYPH_32, 32)
    write_template_png(out_1x, 16, rows16)
    write_template_png(out_2x, 32, rows32)
    print(f"wrote {out_1x} (16x16)")
    preview(rows16)
    print(f"wrote {out_2x} (32x32)")
    preview(rows32)


if __name__ == "__main__":
    main()
