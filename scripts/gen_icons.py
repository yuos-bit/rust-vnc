#!/usr/bin/env python3
"""Generate RustVNC application icons using only the Python standard library."""
from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "apps" / "pc" / "icons"
SS = 4  # supersampling factor


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def rounded_rect(x: float, y: float, x0: float, y0: float, x1: float, y1: float, r: float) -> bool:
    if x < x0 or x > x1 or y < y0 or y > y1:
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def sample_color(x: float, y: float, size: float) -> tuple[int, int, int, int]:
    """Return RGBA for a point in icon space."""
    u = x / size
    v = y / size
    if not rounded_rect(x, y, 0, 0, size, size, size * 0.22):
        return (0, 0, 0, 0)

    # Background gradient approximating the app's glass palette.
    t = max(0.0, min(1.0, (u + v) / 2))
    r = int(lerp(26, 15, t))
    g = int(lerp(33, 52, t))
    b = int(lerp(62, 96, t))

    # Monitor outer frame.
    frame = rounded_rect(x, y, size * 0.16, size * 0.18, size * 0.84, size * 0.64, size * 0.055)
    inner = rounded_rect(x, y, size * 0.195, size * 0.215, size * 0.805, size * 0.605, size * 0.04)
    if frame and not inner:
        return (142, 197, 252, 255)

    # Stand.
    if rounded_rect(x, y, size * 0.465, size * 0.64, size * 0.535, size * 0.735, size * 0.012):
        return (142, 197, 252, 255)
    if rounded_rect(x, y, size * 0.36, size * 0.735, size * 0.64, size * 0.78, size * 0.015):
        return (142, 197, 252, 255)

    # Center dot.
    dx = x - size * 0.5
    dy = y - size * 0.405
    if dx * dx + dy * dy <= (size * 0.085) ** 2:
        return (224, 195, 252, 255)

    return (r, g, b, 255)


def render(size: int) -> list[list[tuple[int, int, int, int]]]:
    rows = []
    step = 1.0 / SS
    for py in range(size):
        row = []
        for px in range(size):
            acc = [0, 0, 0, 0]
            for sy in range(SS):
                for sx in range(SS):
                    color = sample_color(px + (sx + 0.5) * step, py + (sy + 0.5) * step, size)
                    for i in range(4):
                        acc[i] += color[i]
            n = SS * SS
            row.append(tuple(int(round(c / n)) for c in acc))
        rows.append(row)
    return rows


def png_bytes(rows: list[list[tuple[int, int, int, int]]]) -> bytes:
    height = len(rows)
    width = len(rows[0])
    raw = bytearray()
    for row in rows:
        raw.append(0)
        for r, g, b, a in row:
            raw.extend((r, g, b, a))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def ico_bytes(images: list[tuple[int, list[list[tuple[int, int, int, int]]]]]) -> bytes:
    entries: list[tuple[int, bytes]] = []
    for size, rows in images:
        # BMP-in-ICO: BITMAPINFOHEADER + bottom-up BGRA + AND mask.
        xor = bytearray()
        for row in reversed(rows):
            for r, g, b, a in row:
                xor.extend((b, g, r, a))
        mask_row = ((size + 31) // 32) * 4
        mask = bytes(mask_row * size)
        header = struct.pack(
            "<IiiHHIIiiII",
            40,
            size,
            size * 2,
            1,
            32,
            0,
            len(xor) + len(mask),
            0,
            0,
            0,
            0,
        )
        entries.append((size, header + bytes(xor) + mask))

    count = len(entries)
    offset = 6 + 16 * count
    directory = struct.pack("<HHH", 0, 1, count)
    payload = bytearray()
    for size, data in entries:
        directory += struct.pack(
            "<BBBBHHII",
            0 if size >= 256 else size,
            0 if size >= 256 else size,
            0,
            0,
            1,
            32,
            len(data),
            offset,
        )
        offset += len(data)
        payload += data
    return directory + bytes(payload)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    sizes = [16, 32, 48, 64, 128, 256]
    rendered = {size: render(size) for size in sizes}

    for size in (32, 128, 256):
        (OUT / f"{size}x{size}.png").write_bytes(png_bytes(rendered[size]))
    (OUT / "128x128@2x.png").write_bytes(png_bytes(rendered[256]))
    (OUT / "icon.ico").write_bytes(ico_bytes([(size, rendered[size]) for size in sizes]))
    print(f"wrote icons to {OUT}")


if __name__ == "__main__":
    main()
