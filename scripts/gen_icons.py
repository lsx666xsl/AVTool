#!/usr/bin/env python3
"""生成 AVTool 应用图标（icon.ico / png 系列）。
纯 Python 实现（不依赖 PIL）：渐变底 + 波形柱 + 圆点，主题与前端一致。
"""
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

BG_DARK = (15, 23, 42)      # #0F172A
TEAL = (20, 184, 166)       # #14B8A6
BLUE = (59, 130, 246)       # #3B82F6
AMBER = (251, 146, 60)      # #FB923C
WHITE = (248, 250, 252)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def rounded_rect_mask(x, y, w, h, r):
    if x < r and y < r:
        return (r - x) ** 2 + (r - y) ** 2 <= r * r
    if x >= w - r and y < r:
        return (x - (w - 1 - r)) ** 2 + (r - y) ** 2 <= r * r
    if x < r and y >= h - r:
        return (r - x) ** 2 + (y - (h - 1 - r)) ** 2 <= r * r
    if x >= w - r and y >= h - r:
        return (x - (w - 1 - r)) ** 2 + (y - (h - 1 - r)) ** 2 <= r * r
    return True


def draw_icon(size):
    """返回 size×size 的 RGBA 像素列表（每元素 (r,g,b,a)）。"""
    px = [[None] * size for _ in range(size)]
    m = max(1, size // 16)          # 圆角边距
    bars = [0.45, 0.7, 0.55, 0.85, 0.6]  # 波形柱高度比例
    n = len(bars)
    bar_w = max(1, size // 12)
    gap = bar_w
    total_w = n * bar_w + (n - 1) * gap
    x0 = (size - total_w) // 2
    base_y = int(size * 0.68)

    for y in range(size):
        for x in range(size):
            if not rounded_rect_mask(x, y, size, size, size // 5):
                continue
            t = (x + y) / (2 * size)
            color = lerp(TEAL, BLUE, t)
            # 边框高光
            if x < m or y < m:
                color = lerp(color, WHITE, 0.18)
            px[y][x] = (*color, 255)

    # 波形柱
    for i, ratio in enumerate(bars):
        bx = x0 + i * (bar_w + gap)
        bh = int(size * 0.5 * ratio)
        for y in range(base_y - bh, base_y):
            for dx in range(bar_w):
                x = bx + dx
                if 0 <= x < size and 0 <= y < size:
                    px[y][x] = (*WHITE, 255)
    # 峰值圆点
    dot_r = max(1, size // 22)
    cx = x0 + 3 * (bar_w + gap) + bar_w // 2
    cy = base_y - int(size * 0.5 * 0.85) - dot_r * 3
    for y in range(max(0, cy - dot_r * 2), min(size, cy + dot_r * 2)):
        for x in range(max(0, cx - dot_r * 2), min(size, cx + dot_r * 2)):
            if (x - cx) ** 2 + (y - cy) ** 2 <= dot_r * dot_r:
                px[y][x] = (*AMBER, 255)
    return px


def write_png(path, size, px):
    raw = b""
    for y in range(size):
        raw += b"\x00"
        for x in range(size):
            p = px[y][x]
            if p is None:
                raw += b"\x00\x00\x00\x00"
            else:
                raw += bytes(p)
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def write_ico(path, size, px):
    # BMP 格式 ICO（32bpp + AND mask），兼容性最好
    row_size = size * 4
    mask_row = ((size + 31) // 32) * 4
    img_size = 40 + row_size * size + mask_row * size
    with open(path, "wb") as f:
        f.write(struct.pack("<HHH", 0, 1, 1))  # ICONDIR
        f.write(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, img_size, 22))
        # BITMAPINFOHEADER（高度×2：XOR+AND）
        f.write(struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, row_size * size + mask_row * size, 0, 0, 0, 0))
        for y in range(size - 1, -1, -1):  # 自底向上
            for x in range(size):
                p = px[y][x]
                if p is None:
                    f.write(b"\x00\x00\x00\x00")
                else:
                    f.write(bytes((p[2], p[1], p[0], p[3])))  # BGRA
        f.write(b"\x00" * (mask_row * size))


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    for name, size in [("32x32.png", 32), ("128x128.png", 128), ("icon.png", 256)]:
        px = draw_icon(size)
        write_png(os.path.join(OUT_DIR, name), size, px)
        print(f"写入 {name} ({size}x{size})")
    px = draw_icon(32)
    write_ico(os.path.join(OUT_DIR, "icon.ico"), 32, px)
    print("写入 icon.ico (32x32)")


if __name__ == "__main__":
    main()
