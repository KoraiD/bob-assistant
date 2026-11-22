#!/usr/bin/env python3
"""Generate BOB's 1024x1024 source icon (a friendly ghost) with the stdlib only.

Usage: python3 scripts/gen-icon.py [out.png]
Then:  npx tauri icon assets/bob-icon.png   (derives every size + .icns)
The result is deterministic, so the icon is reproducible without design tools.
"""
import math
import struct
import sys
import zlib

SIZE = 1024
SS = 3  # supersampling per axis for anti-aliasing

BG_TOP = (28, 24, 58)
BG_BOT = (10, 9, 22)
GHOST = (138, 255, 193)  # ectoplasm
INK = (7, 19, 13)


def in_rounded_square(x, y, r=0.22):
    # x, y in [0, 1]
    cx = min(max(x, r), 1 - r)
    cy = min(max(y, r), 1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def in_ghost(x, y):
    # Normalised 0..1 canvas. Dome + body + three wavy "feet".
    cx, top, left, right, bottom = 0.5, 0.2, 0.26, 0.74, 0.78
    rad = (right - left) / 2
    if y < top + rad:  # dome
        return (x - cx) ** 2 + (y - (top + rad)) ** 2 <= rad * rad
    if y <= bottom - 0.04:
        return left <= x <= right
    # wavy hem: three bumps
    if left <= x <= right:
        phase = (x - left) / (right - left) * 3 * math.pi
        edge = bottom + 0.035 * math.sin(phase * 2 - math.pi / 2) * -1
        return y <= edge
    return False


def in_eye(x, y):
    for ex in (0.42, 0.58):
        if (x - ex) ** 2 / 0.028**2 + (y - 0.43) ** 2 / 0.04**2 <= 1:
            return True
    return False


def in_smile(x, y):
    dx, dy = x - 0.5, y - 0.5
    d = math.hypot(dx, dy)
    return 0.095 <= d <= 0.115 and dy > 0.03 and abs(dx) < 0.085


def pixel(px, py):
    r = g = b = a = 0.0
    for sy in range(SS):
        for sx in range(SS):
            x = (px + (sx + 0.5) / SS) / SIZE
            y = (py + (sy + 0.5) / SS) / SIZE
            if not in_rounded_square(x, y):
                continue
            t = y
            col = tuple(BG_TOP[i] * (1 - t) + BG_BOT[i] * t for i in range(3))
            if in_ghost(x, y):
                col = INK if (in_eye(x, y) or in_smile(x, y)) else GHOST
            r += col[0]
            g += col[1]
            b += col[2]
            a += 255
    n = SS * SS
    if a == 0:
        return (0, 0, 0, 0)
    covered = a / 255
    return (int(r / covered), int(g / covered), int(b / covered), int(a / n))


def write_png(path, rows):
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "assets/bob-icon.png"
    rows = []
    for py in range(SIZE):
        row = bytearray()
        for px in range(SIZE):
            row.extend(pixel(px, py))
        rows.append(row)
    write_png(out, rows)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
