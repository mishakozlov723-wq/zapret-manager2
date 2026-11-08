"""Build static Manrope TTFs for the WPF installer from the vendored variable font.

WPF cannot pick weights inside a variable font (bold/semibold fall back to faux
or wrong glyphs), so the installer embeds real static instances.
Run: uv run --with fonttools==4.55.0 python scripts/make-installer-fonts.py
"""
from pathlib import Path
from fontTools.ttLib import TTFont, newTable
from fontTools.varLib import instancer

root = Path(__file__).resolve().parent.parent
src = root / "ui" / "assets" / "fonts" / "Manrope-Variable.ttf"
out = root / "installer" / "Assets" / "Fonts"
out.mkdir(parents=True, exist_ok=True)

for weight, style in [(400, "Regular"), (500, "Medium"), (600, "SemiBold"), (700, "Bold")]:
    font = instancer.instantiateVariableFont(TTFont(src), {"wght": weight})
    legacy_family = "Manrope" if style in ("Regular", "Bold") else f"Manrope {style}"
    legacy_style = "Bold" if style == "Bold" else "Regular"
    names = {1: legacy_family, 2: legacy_style, 4: f"Manrope {style}", 6: f"Manrope-{style}", 16: "Manrope", 17: style}
    table = font["name"]
    for name_id in (1, 2, 3, 4, 6, 16, 17, 25):
        table.removeNames(nameID=name_id)
    names[3] = f"Manrope-{style}-installer"
    for name_id, value in names.items():
        table.setName(value, name_id, 3, 1, 0x409)
    os2 = font["OS/2"]
    os2.usWeightClass = weight
    os2.fsSelection = (os2.fsSelection & ~0b1100001) | (0b100000 if style == "Bold" else 0) | (0b1000000 if style == "Regular" else 0)
    font["head"].macStyle = 1 if style == "Bold" else 0
    gasp = newTable('gasp'); gasp.version = 1; gasp.gaspRange = {0xFFFF: 0x000A}; font['gasp'] = gasp  # smooth, no grid-fit (unhinted)
    font.save(out / f"Manrope-{style}.ttf")
    print("OK", style, weight)
