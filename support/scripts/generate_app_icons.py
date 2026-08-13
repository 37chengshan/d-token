#!/usr/bin/env python3
"""Regenerate the full d-token application icon set from the brand cat mark.

Why this script exists
----------------------
The checked-in icons in `code/apps/desktop/src-tauri/icons/` are full-bleed
squares (every PNG is 100% opaque, corner alpha = 255). macOS renders Dock
icons as-is and does NOT round corners automatically, so the app shows as a
large square in the Dock. This script rebuilds every platform asset through
`tauri icon` from a generated 1024x1024 master that is a rounded white card
(Apple-style corner radius) with the brand cat mark inside a 10% safe margin.

Behavior (deterministic, no prompts)
------------------------------------
1. Load `code/apps/desktop/src/assets/dtoken-cat-icon.png` (512x512, white bg).
2. Build a 1024x1024 RGBA master:
   - cat mark scaled to 820x820 (Lanczos), centred -> 10% safe margin
   - rounded-rect corner radius 229px (1024 * 0.2237, Apple macOS ratio),
     anti-aliased via a 4x supersampled mask
3. Write the master to a temp PNG.
4. Run `tauri icon <master>` (tauri-cli 2.11.4) from `code/apps/desktop`,
   which regenerates `src-tauri/icons/` (icns, ico, all PNG sizes, ios/, android/).
5. Self-check + protection:
   - assert `icons/icon.png` corner alpha is 0 (rounded corners took effect)
   - `iconTemplate.png` / `iconTemplate@2x.png` are macOS tray/menu-bar
     template icons consumed by `tray.rs` via include_bytes!; `tauri icon`
     does not regenerate them. If they changed anyway, restore them with
     `git checkout --` (audited via support/scripts/git_operation_audit.py).
   - on `tauri icon` failure: restore the whole icons dir from git (audited)
     and exit non-zero.
6. Print a summary of every generated file.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

REPO_ROOT = Path(__file__).resolve().parents[2]
DESKTOP_DIR = REPO_ROOT / "code" / "apps" / "desktop"
SOURCE_ICON = DESKTOP_DIR / "src" / "assets" / "dtoken-cat-icon.png"
ICONS_DIR = DESKTOP_DIR / "src-tauri" / "icons"
TAURI_BIN = DESKTOP_DIR / "node_modules" / ".bin" / "tauri"
AUDIT_SCRIPT = REPO_ROOT / "support" / "scripts" / "git_operation_audit.py"

MASTER_SIZE = 1024
CONTENT_SIZE = 820  # 80% of master -> 10% safe margin on every side
CONTENT_OFFSET = (MASTER_SIZE - CONTENT_SIZE) // 2  # 102
CORNER_RADIUS = 229  # 1024 * 0.2237 (Apple macOS icon ratio)
SUPERSAMPLE = 4

TEMPLATE_ICONS = ("iconTemplate.png", "iconTemplate@2x.png")


def audit(operation: str, summary: str) -> None:
    """Record a git operation in the repo audit ledger (AGENTS.md section 9)."""
    subprocess.run(
        [
            sys.executable,
            str(AUDIT_SCRIPT),
            "record",
            "--operation",
            operation,
            "--summary",
            summary,
        ],
        check=True,
        cwd=REPO_ROOT,
    )


def build_master() -> Image.Image:
    with Image.open(SOURCE_ICON) as src:
        if src.size != (512, 512):
            raise SystemExit(f"unexpected source size {src.size}, expected 512x512")
        cat = src.convert("RGBA").resize((CONTENT_SIZE, CONTENT_SIZE), Image.LANCZOS)

    master = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE), (0, 0, 0, 0))
    master.paste(cat, (CONTENT_OFFSET, CONTENT_OFFSET), cat)

    # Anti-aliased rounded-rect alpha mask via 4x supersampling.
    big = Image.new("L", (MASTER_SIZE * SUPERSAMPLE, MASTER_SIZE * SUPERSAMPLE), 0)
    draw = ImageDraw.Draw(big)
    draw.rounded_rectangle(
        [0, 0, MASTER_SIZE * SUPERSAMPLE - 1, MASTER_SIZE * SUPERSAMPLE - 1],
        radius=CORNER_RADIUS * SUPERSAMPLE,
        fill=255,
    )
    mask = big.resize((MASTER_SIZE, MASTER_SIZE), Image.LANCZOS)
    master.putalpha(mask)
    return master


def corner_alpha(png_path: Path) -> tuple[int, int, int, int]:
    """Return alpha of the four corners of a PNG (all are 255 for full-bleed)."""
    with Image.open(png_path) as img:
        rgba = img.convert("RGBA")
        w, h = rgba.size
        return tuple(rgba.getpixel(p)[3] for p in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)])


def restore_from_git(paths: list[Path], summary: str) -> None:
    audit("checkout", summary)
    subprocess.run(["git", "checkout", "--", *[str(p) for p in paths]], check=True, cwd=REPO_ROOT)


def main() -> int:
    if not SOURCE_ICON.exists():
        raise SystemExit(f"source icon not found: {SOURCE_ICON}")
    if not TAURI_BIN.exists():
        raise SystemExit(f"tauri CLI not found: {TAURI_BIN}")

    master = build_master()
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    try:
        master.save(tmp_path, format="PNG")
        result = subprocess.run(
            [str(TAURI_BIN), "icon", str(tmp_path)],
            cwd=DESKTOP_DIR,
            capture_output=True,
            text=True,
        )
    finally:
        tmp_path.unlink(missing_ok=True)

    if result.returncode != 0:
        print("tauri icon failed:", file=sys.stderr)
        print(result.stdout, file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        restore_from_git(
            [ICONS_DIR],
            "restore src-tauri/icons after failed tauri icon regeneration",
        )
        return 1

    # Self-check: rounded corners must have landed in the generated icon.png.
    icon_png = ICONS_DIR / "icon.png"
    alphas = corner_alpha(icon_png)
    if any(a != 0 for a in alphas):
        restore_from_git(
            [ICONS_DIR],
            "restore src-tauri/icons after tauri icon produced non-rounded corners",
        )
        raise SystemExit(f"icon.png corners not transparent: {alphas}; icons restored")

    # Protect macOS tray template icons (consumed by tray.rs include_bytes!).
    changed_templates = [
        ICONS_DIR / name for name in TEMPLATE_ICONS if (ICONS_DIR / name).exists()
    ]
    status = subprocess.run(
        ["git", "status", "--short", "--", *[str(p) for p in changed_templates]],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if status:
        restore_from_git(
            changed_templates,
            "restore tray template icons after tauri icon regeneration",
        )
        print(f"restored tray templates: {status.replace(chr(10), '; ')}")

    print("generated icon set (all under src-tauri/icons/):")
    for p in sorted(ICONS_DIR.rglob("*")):
        if p.is_file():
            print(f"  {p.relative_to(ICONS_DIR)}  {p.stat().st_size} bytes")
    print(f"self-check: {icon_png} corner alpha = {alphas} (all 0 = rounded)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
