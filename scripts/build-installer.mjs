import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = fileURLToPath(new URL('../', import.meta.url));
function run(command, args) {
  // npm exposes its JS entry point, so Windows .cmd files and shell quoting are unnecessary.
  if (command === 'npm') {
    if (!process.env.npm_execpath) throw new Error('Run this script through npm run build:installer');
    args = [process.env.npm_execpath, ...args];
    command = process.execPath;
  }
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}
// Repackage the existing app without rewriting its running exe. Full builds use build:release.
run('npm', ['run', 'tauri', '--', 'bundle', '--no-binary-patching']);
const { version } = JSON.parse(readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const payload = path.join(root, `src-tauri/target/release/bundle/nsis/Zapret Manager_${version}_x64-setup.exe`);
if (statSync(payload).size < 100000) throw new Error('NSIS payload is missing or incomplete');
mkdirSync(path.join(root, 'dist'), { recursive: true });
run('dotnet', ['publish', 'installer/Setup.csproj', '-c', 'Release', '-o', 'dist', `-p:Version=${version}`]);
console.log('Installer: dist/Zapret-Manager-Setup.exe');
