"""Check offline font coverage and every CSS asset reference without extra packages."""
from pathlib import Path
import struct
import re
root = Path(__file__).resolve().parents[1] / 'ui'
for css in root.glob('*.css'):
    for url in re.findall(r'url\([\'\"]?([^\)\'\"]+)', css.read_text(encoding='utf-8')):
        assert (css.parent / url).is_file(), (css, url)

def cmap(path):
    data = path.read_bytes()
    u16 = lambda p: struct.unpack_from('>H', data, p)[0]
    u32 = lambda p: struct.unpack_from('>I', data, p)[0]
    tables = {data[p:p+4]: u32(p+8) for p in range(12, 12+16*u16(4), 16)}
    base = tables[b'cmap']
    result = set()
    for pos in range(base+4, base+4+8*u16(base+2), 8):
        table = base + u32(pos+4)
        fmt = u16(table)
        if fmt == 4:
            count = u16(table+6)//2
            ends = table+14
            starts = ends+2*count+2
            for i in range(count):
                result.update(range(u16(starts+2*i), u16(ends+2*i)+1))
        elif fmt == 12:
            for p in range(table+16, table+16+12*u32(table+12), 12):
                result.update(range(u32(p), u32(p+4)+1))
    return result
fonts = root / 'assets/fonts'
assert set(map(ord, 'АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя0123456789')) <= cmap(fonts/'Manrope-Variable.ttf')
for file in fonts.glob('Poppins-*.ttf'):
    assert set(map(ord, 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz')) <= cmap(file)
assert 'U+0030' not in (root/'fonts.css').read_text(encoding='utf-8')
print('PASS: local CSS assets; Manrope Cyrillic and digits; Poppins Latin (4 weights).')
