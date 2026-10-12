"""Fit generated stand-in art to the pixel sizes Slot draws.

Outputs RGBA PNGs in templates/. Slot does not load that folder.
The live art is assets/.
"""

from __future__ import annotations

import re
from pathlib import Path

from PIL import Image, ImageDraw
from svgpathtools import svg2paths2

ROOT = Path(__file__).resolve().parents[1]
SRC = Path(r"C:\Users\marku\.cursor\projects\c-Users-marku-Documents-slot-fork\assets")
SVG = ROOT / "crates" / "slot-ui" / "assets"
OUT = ROOT / "templates"

CREAM = (244, 239, 228, 255)
GROUND = (0x23, 0x1F, 0x20, 255)


def viewbox(path: Path) -> tuple[float, float, float, float]:
    text = path.read_text(encoding="utf-8")
    match = re.search(r'viewBox="([^"]+)"', text)
    if not match:
        raise SystemExit(f"no viewBox in {path}")
    parts = [float(v) for v in match.group(1).replace(",", " ").split()]
    return parts[0], parts[1], parts[2], parts[3]


def raster_path(svg: Path, index: int, w: int, h: int, samples: int = 4000) -> Image.Image:
    paths, _, _ = svg2paths2(str(svg))
    path = paths[index]
    minx, miny, vw, vh = viewbox(svg)
    scale = 8
    canvas = Image.new("L", (w * scale, h * scale), 0)
    draw = ImageDraw.Draw(canvas)
    for sub in path.continuous_subpaths():
        pts = []
        n = max(samples, int(sub.length() * 2))
        for i in range(n):
            z = sub.point(i / n)
            x = (z.real - minx) / vw * (w * scale)
            y = (z.imag - miny) / vh * (h * scale)
            pts.append((x, y))
        draw.polygon(pts, fill=255)
    return canvas.resize((w, h), Image.Resampling.BOX)


def key_magenta(im: Image.Image) -> Image.Image:
    """Drop the flat backdrop by distance from the corner colour.

    The generated plates are hot pink, not pure magenta, and the host plug
    is purple, so a channel threshold eats the plug and keeps the backdrop.
    """
    im = im.convert("RGBA")
    w, h = im.size
    corners = [
        im.getpixel((1, 1)),
        im.getpixel((w - 2, 1)),
        im.getpixel((1, h - 2)),
        im.getpixel((w - 2, h - 2)),
    ]
    kr = sum(c[0] for c in corners) // 4
    kg = sum(c[1] for c in corners) // 4
    kb = sum(c[2] for c in corners) // 4
    tol, soft = 42.0, 78.0
    out = []
    for r, g, b, _a in im.getdata():
        d = ((r - kr) ** 2 + (g - kg) ** 2 + (b - kb) ** 2) ** 0.5
        if d < tol:
            out.append((0, 0, 0, 0))
        elif d < soft:
            na = int((d - tol) / (soft - tol) * 255)
            out.append((r, g, b, na))
        else:
            out.append((r, g, b, 255))
    im.putdata(out)
    return im


def opaque_bbox(im: Image.Image, thresh: int = 20) -> tuple[int, int, int, int]:
    w, h = im.size
    px = im.load()
    minx, miny, maxx, maxy = w, h, 0, 0
    for y in range(h):
        for x in range(w):
            if px[x, y][3] > thresh:
                minx = min(minx, x)
                miny = min(miny, y)
                maxx = max(maxx, x)
                maxy = max(maxy, y)
    if maxx < minx:
        return 0, 0, w, h
    return minx, miny, maxx + 1, maxy + 1


def crop_content(im: Image.Image) -> Image.Image:
    box = opaque_bbox(im)
    return im.crop(box)


def cover(im: Image.Image, w: int, h: int, extra: float = 1.0) -> Image.Image:
    sw, sh = im.size
    scale = max(w / sw, h / sh) * extra
    nw = max(w, int(round(sw * scale)))
    nh = max(h, int(round(sh * scale)))
    resized = im.resize((nw, nh), Image.Resampling.LANCZOS)
    x = max(0, (nw - w) // 2)
    y = max(0, (nh - h) // 2)
    return resized.crop((x, y, x + w, y + h))


def contain(im: Image.Image, w: int, h: int) -> Image.Image:
    sw, sh = im.size
    scale = min(w / sw, h / sh)
    nw = max(1, int(round(sw * scale)))
    nh = max(1, int(round(sh * scale)))
    resized = im.resize((nw, nh), Image.Resampling.LANCZOS)
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    canvas.paste(resized, ((w - nw) // 2, (h - nh) // 2), resized)
    return canvas


def apply_mask(color: Image.Image, mask: Image.Image) -> Image.Image:
    """The traced outline is the alpha. The photo only supplies colour."""
    color = color.convert("RGBA")
    pixels = list(color.getdata())
    opaque = [p for p in pixels if p[3] > 200]
    if opaque:
        mid = len(opaque) // 2
        fill = opaque[mid]
    else:
        fill = (40, 40, 44, 255)
    out = []
    for (r, g, b, a), m in zip(pixels, mask.getdata()):
        if a < 160:
            r, g, b = fill[0], fill[1], fill[2]
        out.append((r, g, b, m))
    color.putdata(out)
    return color


def fill_rect(im: Image.Image, x0: int, y0: int, x1: int, y1: int, colour: tuple[int, int, int, int]) -> None:
    px = im.load()
    w, h = im.size
    for y in range(max(0, y0), min(h, y1)):
        for x in range(max(0, x0), min(w, x1)):
            if px[x, y][3] > 0:
                px[x, y] = (colour[0], colour[1], colour[2], px[x, y][3])


def masked_photo(src_name: str, svg_name: str, w: int, h: int, path_index: int = 0) -> Image.Image:
    photo = cover(crop_content(key_magenta(Image.open(SRC / src_name))), w, h, extra=1.08)
    mask = raster_path(SVG / svg_name, path_index, w, h)
    return apply_mask(photo, mask)


def label_gba(w: int, h: int) -> tuple[int, int, int, int]:
    return (
        (w * 135 + 500) // 1000,
        (h * 231 + 500) // 1000,
        (w * 875 + 500) // 1000,
        (h * 882 + 500) // 1000,
    )


def label_gb(w: int, h: int) -> tuple[int, int, int, int]:
    return (
        (w * 147 + 500) // 1000,
        (h * 291 + 500) // 1000,
        (w * 854 + 500) // 1000,
        (h * 868 + 500) // 1000,
    )


def save(im: Image.Image, name: str) -> None:
    path = OUT / name
    im.save(path, "PNG")
    print(f"{name} {im.size[0]}x{im.size[1]}")


def fit_carts() -> None:
    gba = masked_photo("gba_cart_src.jpg", "cart.svg", 360, 212)
    x0, y0, x1, y1 = label_gba(360, 212)
    fill_rect(gba, x0, y0, x1, y1, CREAM)
    save(gba, "gba_cart.png")

    gb = masked_photo("gb_cart_src.jpg", "gb_cart.svg", 312, 337)
    x0, y0, x1, y1 = label_gb(312, 337)
    fill_rect(gb, x0, y0, x1, y1, CREAM)
    save(gb, "gb_cart.png")

    gbc = masked_photo("gbc_cart_src.jpg", "gbc_cart.svg", 312, 337)
    # Below the shoulder the two paks are the same mould. One antialiased
    # row can disagree by a pixel; the shelf treats those rows as equal.
    gb_px, gbc_px = gb.load(), gbc.load()
    for y in range(30, gb.size[1]):
        for x in range(gb.size[0]):
            r, gch, b, _a = gbc_px[x, y]
            gbc_px[x, y] = (r, gch, b, gb_px[x, y][3])
    fill_rect(gbc, x0, y0, x1, y1, CREAM)
    save(gbc, "gbc_cart.png")


def crop_board(im: Image.Image) -> Image.Image:
    """Drop a flat backdrop under or around the PCB."""
    im = im.convert("RGBA")
    w, h = im.size
    px = im.load()

    def on_board(y: int) -> bool:
        hits = 0
        step = 4
        for x in range(0, w, step):
            r, g, b, _a = px[x, y]
            green = g > r + 8 and g > b
            gold = r > 140 and g > 100 and r > b + 30
            if green or gold:
                hits += 1
        return hits > (w // step) * 0.25

    top, bot = 0, h - 1
    while top < bot and not on_board(top):
        top += 1
    while bot > top and not on_board(bot):
        bot -= 1
    return im.crop((0, top, w, bot + 1))


def bottom_cover(im: Image.Image, w: int, h: int) -> Image.Image:
    """Scale to cover, keeping the gold fingers at the bottom of the cart."""
    sw, sh = im.size
    scale = max(w / sw, h / sh)
    nw = max(w, int(round(sw * scale)))
    nh = max(h, int(round(sh * scale)))
    resized = im.resize((nw, nh), Image.Resampling.LANCZOS)
    x = max(0, (nw - w) // 2)
    y = max(0, nh - h)
    return resized.crop((x, y, x + w, y + h))


def fit_interiors() -> None:
    for src, svg, w, h, name in (
        ("gba_interior_src.jpg", "cart.svg", 360, 212, "gba_interior.png"),
        ("gb_interior_src.jpg", "gb_cart.svg", 312, 337, "gb_interior.png"),
    ):
        photo = bottom_cover(crop_board(Image.open(SRC / src)), w, h)
        mask = raster_path(SVG / svg, 0, w, h)
        save(apply_mask(photo, mask), name)


def fit_board() -> None:
    board = masked_photo("board_src.jpg", "board.svg", 360, 212, 0)
    save(board, "board.png")


def fit_part(src_name: str, w: int, h: int, name: str) -> None:
    photo = cover(crop_content(key_magenta(Image.open(SRC / src_name))), w, h)
    save(photo, name)


def fit_plug(src_name: str, name: str) -> None:
    w, h = 92, 244
    photo = crop_content(key_magenta(Image.open(SRC / src_name)))
    target_h = int(h * 0.86)
    scale = target_h / photo.size[1]
    nw = max(1, int(round(photo.size[0] * scale)))
    nh = target_h
    if nw > w - 4:
        scale = (w - 4) / photo.size[0]
        nw = w - 4
        nh = max(1, int(round(photo.size[1] * scale)))
    resized = photo.resize((nw, nh), Image.Resampling.LANCZOS)
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    x = (w - nw) // 2
    y = h - nh - 2
    canvas.paste(resized, (x, y), resized)
    px = canvas.load()
    for row in range(18):
        fade = row / 18
        for col in range(w):
            r, g, b, a = px[col, row]
            px[col, row] = (r, g, b, int(a * fade))
    save(canvas, name)


def fit_port() -> None:
    im = Image.open(SRC / "link_port_src.jpg").convert("RGB")
    w, h = im.size
    px = im.load()
    weights = []
    for y in range(h):
        gold = 0
        for x in range(w):
            r, g, b = px[x, y]
            if r > 140 and g > 100 and b < 110:
                gold += 1
        weights.append(gold)
    if max(weights) < 8:
        cy = h // 2
    else:
        total = sum(i * n for i, n in enumerate(weights))
        cy = total // sum(weights)
    band_h = max(8, int(round(w * 58 / 720)))
    y0 = min(max(0, cy - band_h // 2), h - band_h)
    band = im.crop((0, y0, w, y0 + band_h)).resize((720, 58), Image.Resampling.LANCZOS)
    out = Image.new("RGBA", (720, 58))
    out.paste(band)
    save(out, "link_port.png")


def fit_adapter() -> None:
    photo = contain(crop_content(key_magenta(Image.open(SRC / "adapter_src.jpg"))), 268, 182)
    save(photo, "link_adapter.png")


def sample(tex: Image.Image, x: int, y: int, tw: int, th: int) -> tuple[int, int, int, int]:
    sx = int(x / tw * tex.size[0]) % tex.size[0]
    sy = int(y / th * tex.size[1]) % tex.size[1]
    r, g, b = tex.convert("RGB").getpixel((sx, sy))[:3]
    return r, g, b, 255


def fit_slot() -> None:
    housing = Image.open(SRC / "housing_src.jpg")
    cavity = Image.open(SRC / "cavity_src.jpg")
    out_w, mouth_h = 720, 58
    mouth_w = 254.0
    mouth_x = (out_w - mouth_w) / 2
    lip_h = 2.0
    bay_w = mouth_w + 18
    bay_x = (out_w - bay_w) / 2
    bay_y = lip_h
    slit_h = 9.0
    slit_y = bay_y + 5
    scoop_w = mouth_w * 0.88
    recess_h = 42.0
    scoop_y = slit_y + slit_h
    scoop_d = recess_h - (scoop_y - bay_y)
    rim_w = 2.0
    cx = out_w / 2
    hw = scoop_w / 2
    floor = scoop_y + scoop_d + rim_w

    def depth(x: float) -> float:
        t = abs(x) / hw
        return max(0.0, scoop_d * (1.0 - t**4))

    back = Image.new("RGBA", (out_w, mouth_h), (0, 0, 0, 0))
    front = Image.new("RGBA", (out_w, mouth_h), (0, 0, 0, 0))
    bp, fp = back.load(), front.load()

    def put(img_px, x: int, y: int, colour: tuple[int, int, int, int]) -> None:
        if 0 <= x < out_w and 0 <= y < mouth_h:
            img_px[x, y] = colour

    for y in range(mouth_h):
        for x in range(out_w):
            xf, yf = float(x), float(y)
            in_bay = bay_x <= xf < bay_x + bay_w and bay_y <= yf < bay_y + recess_h
            in_slit = mouth_x <= xf < mouth_x + mouth_w and slit_y <= yf < slit_y + slit_h
            in_scoop = False
            if abs(xf - cx) < hw and yf >= scoop_y:
                in_scoop = yf < scoop_y + round(depth(xf - cx))
            on_lip = mouth_x <= xf < mouth_x + mouth_w and yf < lip_h
            if in_slit or in_scoop:
                put(bp, x, y, sample(cavity, x, y, out_w, mouth_h))
            elif in_bay or on_lip:
                r, g, b, a = sample(cavity, x, y + 20, out_w, mouth_h)
                put(bp, x, y, (r, g, b, a))

            # Front housing, matching the bands drawn over the inserted cart.
            side = xf < bay_x or xf >= bay_x + bay_w
            near = cx - scoop_w / 2
            far = cx + scoop_w / 2
            frame = scoop_y <= yf < floor and (bay_x <= xf < near or far <= xf < bay_x + bay_w)
            below_scoop = False
            if abs(xf - cx) < hw:
                top = scoop_y + round(depth(xf - cx)) + rim_w
                below_scoop = top <= yf < floor
            under = yf >= floor
            rim = False
            if abs(xf - cx) < hw:
                rim_y = scoop_y + round(depth(xf - cx))
                rim = rim_y <= yf < rim_y + rim_w
            if side or frame or below_scoop or under:
                put(fp, x, y, sample(housing, x, y, out_w, mouth_h))
            elif rim:
                r, g, b, _a = sample(housing, x, y, out_w, mouth_h)
                put(fp, x, y, (min(255, r + 28), min(255, g + 28), min(255, b + 28), 255))

    save(back, "slot_back.png")
    save(front, "slot_front.png")


def fit_sticker() -> None:
    w, h = 660, 228
    body = raster_path(SVG / "sticker.svg", 0, w, h, samples=2500)
    panel = raster_path(SVG / "sticker.svg", 1, w, h, samples=1200)
    paper = cover(Image.open(SRC / "sticker_src.jpg").convert("RGBA"), w, h)
    housing = cover(Image.open(SRC / "housing_src.jpg").convert("RGB"), w, h)
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    bp, pp, hp, paper_px = body.load(), panel.load(), housing.load(), paper.load()
    dest = out.load()
    for y in range(h):
        for x in range(w):
            if bp[x, y] == 0:
                continue
            if pp[x, y] > 128:
                r, g, b, _a = paper_px[x, y]
                dest[x, y] = (r, g, b, bp[x, y])
            else:
                r, g, b = hp[x, y]
                # Keep the body dark enough for the white plate type.
                dest[x, y] = (
                    (0x23 * 3 + r // 5) // 4,
                    (0x1F * 3 + g // 5) // 4,
                    (0x20 * 3 + b // 5) // 4,
                    bp[x, y],
                )
    save(out, "sticker.png")


def main() -> None:
    raise SystemExit(
        "the live pictures are in assets/. This script does not write them:\n"
        "  cargo run -p slot-ui --example raster_templates"
    )
    OUT.mkdir(parents=True, exist_ok=True)
    fit_carts()
    fit_interiors()
    fit_board()
    fit_part("socket_src.jpg", 64, 46, "socket.png")
    fit_part("chip_src.jpg", 59, 41, "chip.png")
    fit_plug("plug_host_src.jpg", "link_plug_host.png")
    fit_plug("plug_join_src.jpg", "link_plug_join.png")
    fit_port()
    fit_adapter()
    fit_slot()
    fit_sticker()

    host = Image.open(OUT / "link_plug_host.png")
    join = Image.open(OUT / "link_plug_join.png")
    print("host tip", host.getpixel((46, 240)), "body", host.getpixel((36, 154)), "top", host.getpixel((46, 2)))
    print("join tip", join.getpixel((46, 240)), "body", join.getpixel((36, 154)), "top", join.getpixel((46, 2)))


if __name__ == "__main__":
    main()
