#!/usr/bin/env python3
"""Reproduce Klicky's original geometric icon using only Python's standard library."""
import struct
import zlib
from pathlib import Path

SIZES = (16, 20, 24, 32, 40, 48, 64, 256)
SAMPLES = 4


def rounded(x, y, bottom):
    cx = min(192, max(64, x))
    cy = min(bottom - 32, max(56, y))
    return (x - cx) ** 2 + (y - cy) ** 2 <= 32 ** 2


def polygon(x, y, points):
    inside = False
    previous = points[-1]
    for current in points:
        ax, ay = previous
        bx, by = current
        if (ay > y) != (by > y) and x < (bx - ax) * (y - ay) / (by - ay) + ax:
            inside = not inside
        previous = current
    return inside


def color(x, y):
    if not rounded(x, y, 232):
        return (0, 0, 0, 0)
    if not rounded(x, y, 216):
        return (30, 64, 175, 255)
    if (78 <= x <= 104 and 66 <= y <= 186) or polygon(
        x, y, ((104, 120), (150, 66), (184, 66), (126, 128), (188, 186), (150, 186), (104, 144))
    ):
        return (255, 255, 255, 255)
    return (37, 99, 235, 255)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def png(size):
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            pixels = [
                color((x + (sx + 0.5) / SAMPLES) * 256 / size,
                      (y + (sy + 0.5) / SAMPLES) * 256 / size)
                for sy in range(SAMPLES) for sx in range(SAMPLES)
            ]
            alpha = sum(p[3] for p in pixels)
            # Unassociated alpha prevents dark fringes on light Windows themes.
            rgb = [round(sum(p[c] * p[3] for p in pixels) / alpha) if alpha else 0 for c in range(3)]
            rows.extend((*rgb, round(alpha / len(pixels))))
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def main():
    images = [png(size) for size in SIZES]
    offset = 6 + 16 * len(SIZES)
    directory = bytearray(struct.pack("<HHH", 0, 1, len(SIZES)))
    for size, image in zip(SIZES, images):
        directory.extend(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(image), offset))
        offset += len(image)
    output = Path(__file__).resolve().parent.parent / "packaging/windows/klicky.ico"
    output.write_bytes(directory + b"".join(images))
    print(output)


if __name__ == "__main__":
    main()
