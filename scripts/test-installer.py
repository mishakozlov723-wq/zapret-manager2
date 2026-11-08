"""Exercise built native windows, without installing or modifying any service."""
import subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[1]
out = root / 'installer-verification'
out.mkdir(exist_ok=True)
result = subprocess.run([str(root / 'dist/Zapret-Manager-Setup.exe'), '--self-test', str(out)], cwd=root, timeout=90)
report = out / ('results.txt' if result.returncode == 0 else 'failure.txt')
print(report.read_text(encoding='utf-8') if report.exists() else f'No report; exit {result.returncode}')
raise SystemExit(result.returncode)
