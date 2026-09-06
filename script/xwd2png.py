#!/usr/bin/env python3
"""Convert an X `xwd` dump to PNG using only the Python standard library.

Used by the agent UI-verification flow so screenshots can be produced without
ImageMagick / ffmpeg installed. See docs/agent-ui-verification.md.

Usage: python3 script/xwd2png.py input.xwd output.png [--fail-if-uniform]

`--fail-if-uniform` still writes the PNG but exits with UNIFORM_EXIT when every
pixel is the same color, which is how a window that has never presented a frame
looks. `ui-run.sh launch` polls on this to decide when the GUI is up.
"""
import sys, struct, zlib

UNIFORM_EXIT = 3


def read_xwd(path):
    with open(path, "rb") as f:
        data = f.read()
    # XWDFileHeader: 25 big-endian u32 fields, then a null-terminated window name.
    fields = struct.unpack(">25I", data[:100])
    (header_size, _version, _pixmap_format, _pixmap_depth, pixmap_width,
     pixmap_height, _xoffset, byte_order, _bitmap_unit, _bitmap_bit_order,
     _bitmap_pad, bits_per_pixel, bytes_per_line, _visual_class, red_mask,
     green_mask, blue_mask, _bits_per_rgb, _colormap_entries, ncolors,
     _window_width, _window_height, _window_x, _window_y, _border) = fields

    # header_size covers header + window name; the colormap (12 bytes/entry) follows.
    pixels = data[header_size + ncolors * 12:]

    def mask_shift(mask):
        if mask == 0:
            return 0, 0
        shift = 0
        m = mask
        while m & 1 == 0:
            m >>= 1
            shift += 1
        width = 0
        while m & 1:
            m >>= 1
            width += 1
        return shift, width

    rsh, rw = mask_shift(red_mask)
    gsh, gw = mask_shift(green_mask)
    bsh, bw = mask_shift(blue_mask)
    bpp = bits_per_pixel // 8

    rows = bytearray()
    for y in range(pixmap_height):
        row_start = y * bytes_per_line
        rows.append(0)  # PNG filter type "none"
        for x in range(pixmap_width):
            p = row_start + x * bpp
            chunk = pixels[p:p + bpp]
            if len(chunk) < bpp:
                px = 0
            elif byte_order == 0:  # LSBFirst
                px = int.from_bytes(chunk, "little")
            else:
                px = int.from_bytes(chunk, "big")
            r = (px & red_mask) >> rsh
            g = (px & green_mask) >> gsh
            b = (px & blue_mask) >> bsh
            if rw and rw < 8:
                r <<= (8 - rw)
            if gw and gw < 8:
                g <<= (8 - gw)
            if bw and bw < 8:
                b <<= (8 - bw)
            rows += bytes((r & 0xFF, g & 0xFF, b & 0xFF))
    return pixmap_width, pixmap_height, bytes(rows)


def write_png(path, w, h, raw):
    def chunk(tag, payload):
        c = tag + payload
        return struct.pack(">I", len(payload)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)  # 8-bit RGB
    idat = zlib.compress(raw, 9)
    with open(path, "wb") as f:
        f.write(sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b""))


def is_uniform(width, height, raw):
    """Whether every pixel of a scanline buffer has the same color.

    `raw` is in PNG scanline layout: one filter byte per row, then RGB triples.
    Each row is compared whole against a repeat of the first pixel, so every
    pixel is inspected without a per-pixel Python loop. Sampling instead would
    let sparse content fall between the sample points and read as uniform,
    which would strand `ui-run.sh launch` on an already-rendered window.
    """
    if width == 0 or height == 0:
        return True
    stride = width * 3 + 1
    expected = raw[1:4] * width
    return all(
        raw[y * stride + 1:(y + 1) * stride] == expected for y in range(height)
    )


if __name__ == "__main__":
    arguments = sys.argv[1:]
    fail_if_uniform = "--fail-if-uniform" in arguments
    paths = [argument for argument in arguments if not argument.startswith("--")]
    unknown = [
        argument
        for argument in arguments
        if argument.startswith("--") and argument != "--fail-if-uniform"
    ]
    if len(paths) != 2 or unknown:
        sys.exit("usage: xwd2png.py input.xwd output.png [--fail-if-uniform]")
    width, height, raw = read_xwd(paths[0])
    write_png(paths[1], width, height, raw)
    print(f"wrote {paths[1]} {width}x{height}")
    if fail_if_uniform and is_uniform(width, height, raw):
        sys.exit(UNIFORM_EXIT)
