"""Vendor fonts and selected user-supplied Lucide icons; no runtime downloads."""
from pathlib import Path
from urllib.request import urlopen
from urllib.parse import quote
import shutil
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[1]
assets = root / 'ui' / 'assets'
fonts = assets / 'fonts'
icons = assets / 'icons'
fonts.mkdir(parents=True, exist_ok=True)
icons.mkdir(parents=True, exist_ok=True)
files = {'manrope': ['Manrope[wght].ttf', 'OFL.txt'], 'poppins': ['Poppins-Regular.ttf', 'Poppins-Medium.ttf', 'Poppins-SemiBold.ttf', 'Poppins-Bold.ttf', 'OFL.txt']}
for family, names in files.items():
    for name in names:
        dest = fonts / (family + '-OFL.txt' if name == 'OFL.txt' else name.replace('[wght]', '-Variable'))
        if dest.exists():
            continue
        url = 'https://raw.githubusercontent.com/google/fonts/main/ofl/' + family + '/' + quote(name)
        with urlopen(url, timeout=40) as response:
            data = response.read()
        if name.endswith('.ttf') and data[:4] not in (b'\x00\x01\x00\x00', b'OTTO'):
            raise ValueError('Invalid font: ' + name)
        dest.write_bytes(data)
        print(dest.name, len(data))
names = 'house layout-grid globe download settings info check circle-check monitor-check shield-check power refresh-cw plus x folder folder-open folder-plus hard-drive archive server arrow-up chevron-right triangle-alert search rotate-ccw trash-2'.split()
for name in names:
    src = root.parent / 'icons' / (name + '.svg')
    tree = ET.parse(src)
    assert tree.getroot().tag.endswith('svg')
    assert all(el.tag.split('}')[-1] in {'svg', 'path', 'circle', 'rect', 'line', 'polyline', 'polygon', 'ellipse', 'g'} for el in tree.iter())
    shutil.copyfile(src, icons / src.name)
license_file = assets / 'lucide-LICENSE.txt'
if not license_file.exists():
    with urlopen('https://raw.githubusercontent.com/lucide-icons/lucide/main/LICENSE', timeout=40) as response:
        license_file.write_bytes(response.read())
print('Vendored', len(names), 'icons')
