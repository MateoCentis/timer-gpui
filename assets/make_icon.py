"""Genera el logo de Temporizador: assets/temporizador.ico (ícono del .exe y de la
bandeja) y assets/logo.png (barra de título y notificaciones).

Uso:  python assets/make_icon.py   (requiere Pillow)

El dibujo se hace a 4x y se reduce, para bordes suaves. En tamaños chicos los trazos
son más gruesos para que siga leyéndose.
"""

import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

HERE = Path(__file__).parent
TOP_LEFT = (251, 191, 36)  # ámbar  #FBBF24
BOTTOM_RIGHT = (234, 88, 12)  # naranja #EA580C
HAND = (194, 65, 12)  # #C2410C


def gradient(n: int) -> Image.Image:
    """Degradado diagonal de arriba-izquierda a abajo-derecha."""
    small = Image.new("RGB", (2, 2))
    small.putpixel((0, 0), TOP_LEFT)
    small.putpixel((1, 1), BOTTOM_RIGHT)
    mid = tuple((a + b) // 2 for a, b in zip(TOP_LEFT, BOTTOM_RIGHT))
    small.putpixel((1, 0), mid)
    small.putpixel((0, 1), mid)
    return small.resize((n, n), Image.BILINEAR)


def draw(size: int) -> Image.Image:
    s = size * 4
    small = size <= 24
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Fondo: cuadrado redondeado con degradado.
    mask = Image.new("L", (s, s), 0)
    pad = round(s * 0.04)
    ImageDraw.Draw(mask).rounded_rectangle(
        (pad, pad, s - pad, s - pad), radius=round(s * 0.24), fill=255
    )
    img.paste(gradient(s), (0, 0), mask)
    d = ImageDraw.Draw(img)

    # Esfera blanca del cronómetro.
    cx, cy = s * 0.5, s * 0.56
    r = s * (0.31 if small else 0.29)
    d.ellipse((cx - r, cy - r, cx + r, cy + r), fill="white")

    # Botón de arriba.
    bw, bh = s * 0.14, s * 0.075
    top = cy - r - bh * 0.55
    d.rounded_rectangle(
        (cx - bw / 2, top - bh / 2, cx + bw / 2, top + bh / 2), radius=bh * 0.4, fill="white"
    )

    # Marcas de cuarto (solo en tamaños grandes).
    if not small:
        for k in range(4):
            a = k * math.pi / 2
            r0, r1 = r * 0.72, r * 0.86
            d.line(
                [
                    (cx + r0 * math.sin(a), cy - r0 * math.cos(a)),
                    (cx + r1 * math.sin(a), cy - r1 * math.cos(a)),
                ],
                fill=(253, 186, 116),
                width=round(s * 0.025),
            )

    # Aguja: de las 12 a las ~2, y un sector "transcurrido" suave.
    sector = r * 0.62
    d.pieslice(
        (cx - sector, cy - sector, cx + sector, cy + sector), start=-90, end=-30, fill=(254, 215, 170)
    )
    a = math.radians(60)
    hand = r * 0.62
    width = round(s * (0.07 if small else 0.05))
    d.line([(cx, cy), (cx, cy - hand)], fill=HAND, width=width)
    d.line([(cx, cy), (cx + hand * math.sin(a), cy - hand * math.cos(a))], fill=HAND, width=width)
    hub = width * 0.85
    d.ellipse((cx - hub, cy - hub, cx + hub, cy + hub), fill=HAND)

    img.putalpha(ImageChops.multiply(img.getchannel("A"), mask))
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    images = [draw(n) for n in sizes]
    images[-1].save(
        HERE / "temporizador.ico",
        format="ICO",
        sizes=[(n, n) for n in sizes],
        append_images=images[:-1],
    )
    draw(64).save(HERE / "logo.png")
    draw(256).save(HERE / "logo-256.png")
    print("ok:", HERE / "temporizador.ico", HERE / "logo.png")


if __name__ == "__main__":
    main()
