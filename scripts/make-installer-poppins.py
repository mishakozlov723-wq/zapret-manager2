"""Installer copy of Poppins without digits: WPF falls back per character, so digits go to Manrope."""
from pathlib import Path
from fontTools.ttLib import TTFont, newTable
from fontTools import subset

root = Path(__file__).resolve().parents[1]
src = root / 'ui' / 'assets' / 'fonts'
dst = root / 'installer' / 'Assets' / 'Fonts'
dst.mkdir(parents=True, exist_ok=True)
for weight in ('Regular', 'Medium', 'SemiBold', 'Bold'):
    font = TTFont(src / f'Poppins-{weight}.ttf')
    keep = [cp for cp in font.getBestCmap() if not 0x30 <= cp <= 0x39]
    opts = subset.Options()
    opts.name_IDs = ['*']
    opts.name_languages = ['*']
    opts.layout_features = ['*']
    opts.notdef_outline = True
    opts.recalc_bounds = True
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=keep)
    sub.subset(font)
    assert not any(0x30 <= cp <= 0x39 for cp in font.getBestCmap()), weight
    # No gasp table = no smoothing at small sizes on Windows (jagged text). Unhinted font: smooth, no grid-fit.
    gasp = newTable('gasp'); gasp.version = 1; gasp.gaspRange = {0xFFFF: 0x000A}; font['gasp'] = gasp
    font.save(dst / f'Poppins-{weight}.ttf')
    print('OK Poppins', weight, len(keep), 'codepoints')
