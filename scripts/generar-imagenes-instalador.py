"""Genera las imágenes HUD del instalador NSIS (cabecera y lateral) con Pillow.

Uso: python scripts/generar-imagenes-instalador.py
Salida: src-tauri/windows/nsis-header.bmp (150x57) y nsis-sidebar.bmp (164x314).
"""
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

RAIZ = Path(__file__).resolve().parent.parent
TAURI = RAIZ / "src-tauri"
SALIDA = TAURI / "windows"

FONDO = (7, 8, 12)
AMARILLO = (245, 229, 10)
CIAN = (45, 226, 230)
CIAN_TENUE = (16, 62, 68)
GRIS = (120, 124, 130)


def fuente(tam, negrita=False):
    for nombre in ("bahnschrift.ttf", "consolab.ttf" if negrita else "consola.ttf"):
        try:
            return ImageFont.truetype(f"C:/Windows/Fonts/{nombre}", tam)
        except OSError:
            continue
    return ImageFont.load_default()


def marco(d, x0, y0, x1, y1, corte, color, grosor=1):
    """Rectángulo con las esquinas superior-izquierda e inferior-derecha cortadas."""
    pts = [(x0 + corte, y0), (x1, y0), (x1, y1 - corte), (x1 - corte, y1), (x0, y1), (x0, y0 + corte)]
    d.line(pts + [pts[0]], fill=color, width=grosor)


def logo(tam):
    return Image.open(TAURI / "icons" / "icon.png").convert("RGB").resize((tam, tam), Image.LANCZOS)


def cabecera():
    w, h = 150, 57
    img = Image.new("RGB", (w, h), FONDO)
    d = ImageDraw.Draw(img)
    marco(d, 0, 0, w - 1, h - 1, 8, AMARILLO)
    img.paste(logo(41), (8, 8))
    d.text((55, 11), "NETRUNNER", font=fuente(15, True), fill=AMARILLO)
    d.text((56, 30), "SYS//SETUP", font=fuente(11), fill=CIAN)
    d.line([(56, 45), (w - 12, 45)], fill=CIAN_TENUE)
    d.rectangle([(w - 12, 43), (w - 9, 46)], fill=CIAN)
    return img.convert("RGB")


def lateral(version):
    w, h = 164, 314
    img = Image.new("RGB", (w, h), FONDO)
    d = ImageDraw.Draw(img)
    # rejilla cian muy tenue
    for x in range(0, w, 16):
        d.line([(x, 0), (x, h)], fill=(11, 24, 28))
    for y in range(0, h, 16):
        d.line([(0, y), (w, y)], fill=(11, 24, 28))
    # banda diagonal de advertencia amarillo/negro (zona inferior)
    y0, y1 = h - 26, h - 8
    banda = Image.new("RGB", (w - 16, y1 - y0), FONDO)
    bd = ImageDraw.Draw(banda)
    for i in range(-20, w, 12):
        bd.polygon([(i, y1 - y0), (i + 6, y1 - y0), (i + 6 + (y1 - y0), 0), (i + (y1 - y0), 0)], fill=AMARILLO)
    img.paste(banda, (8, y0))
    marco(d, 0, 0, w - 1, h - 1, 14, AMARILLO, 2)
    marco(d, 6, 6, w - 7, h - 7, 10, CIAN_TENUE)
    # logo y títulos
    img.paste(logo(96), ((w - 96) // 2, 24))
    f_t, f_p = fuente(15, True), fuente(10)
    for texto, y, fte, col in (("NETRUNNER", 134, f_t, AMARILLO), ("PANEL", 152, f_t, AMARILLO)):
        d.text(((w - d.textlength(texto, font=fte)) / 2, y), texto, font=fte, fill=col)
    v = f"v{version}"
    d.text(((w - d.textlength(v, font=fuente(13))) / 2, 176), v, font=fuente(13), fill=CIAN)
    d.line([(20, 202), (w - 20, 202)], fill=CIAN)
    d.rectangle([(20, 200), (26, 204)], fill=CIAN)
    d.text((20, 212), "OS_SYSTEM", font=f_p, fill=GRIS)
    d.text((20, 227), "INSTALL // 0x13", font=f_p, fill=CIAN)
    d.text((20, 242), "USER_MODE: OK", font=f_p, fill=GRIS)
    return img.convert("RGB")


def main():
    version = json.loads((TAURI / "tauri.conf.json").read_text(encoding="utf-8"))["version"]
    SALIDA.mkdir(parents=True, exist_ok=True)
    cabecera().save(SALIDA / "nsis-header.bmp", format="BMP")
    lateral(version).save(SALIDA / "nsis-sidebar.bmp", format="BMP")
    print(f"Imágenes generadas en {SALIDA} (versión {version})")


if __name__ == "__main__":
    main()
