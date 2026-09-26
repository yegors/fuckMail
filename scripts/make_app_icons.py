#!/usr/bin/env python3
"""Regenerate gui/app-icon/app-icon.ico and .icns from app-icon.png.

Run only when the icon changes; the outputs are committed. Needs Pillow,
which is not a project dependency: `python -m pip install pillow`.
"""
import io
from pathlib import Path
import struct

from PIL import Image

ICON_DIR = Path(__file__).resolve().parents[1] / "gui" / "app-icon"
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)
# macOS icon types that hold PNG data, by pixel size.
ICNS_TYPES = ((b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128), (b"ic08", 256),
              (b"ic09", 512), (b"ic10", 1024), (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512))


def png(source, size):
    out = io.BytesIO()
    source.resize((size, size), Image.Resampling.LANCZOS).save(out, "PNG", optimize=True)
    return out.getvalue()


def main():
    source = Image.open(ICON_DIR / "app-icon.png").convert("RGBA")
    # Windows Vista and later read PNG-compressed ICO entries at every size.
    images = [png(source, size) for size in ICO_SIZES]
    offset = 6 + 16 * len(images)
    ico = struct.pack("<HHH", 0, 1, len(images))
    for size, data in zip(ICO_SIZES, images):
        ico += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    (ICON_DIR / "app-icon.ico").write_bytes(ico + b"".join(images))
    entries = b"".join(kind + struct.pack(">I", 8 + len(data)) + data
                       for kind, data in ((kind, png(source, size)) for kind, size in ICNS_TYPES))
    (ICON_DIR / "app-icon.icns").write_bytes(b"icns" + struct.pack(">I", 8 + len(entries)) + entries)


if __name__ == "__main__":
    main()
