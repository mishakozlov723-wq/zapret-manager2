'use strict';
// Settings page: storage folders (own in-app folder picker) and the user's sites for list-general-user.txt.
// Uses helpers from app.js ($, h, invoke, toast, S, derive, connect, render).
const ST = { data: null, err: null, saving: false, key: '' };
const FOLDERS = [
  { kind: 'downloads', title: 'Новые версии', desc: 'Сюда скачиваются релизы Zapret. При смене папки скачанное переедет туда же.' },
  { kind: 'archive', title: 'Старые версии', desc: 'Сюда откладываются прошлые релизы после скачивания новой.' },
  { kind: 'install', title: 'Папка службы', desc: 'Откуда работает служба. Внутри будет создана папка ZapretManager с доступом только для администраторов. Применится при следующем подключении.' },
];
// Ready sets of domains; subdomains (api-v2.soundcloud.com, *.sndcdn.com) are covered automatically.
const PRESETS = [
  { name: 'Instagram', domains: ['instagram.com', 'cdninstagram.com', 'ig.me'] },
  { name: 'Facebook', domains: ['facebook.com', 'fbcdn.net', 'fb.com', 'facebook.net', 'messenger.com'] },
  { name: 'X (Twitter)', domains: ['x.com', 'twitter.com', 'twimg.com', 't.co'] },
  { name: 'Telegram', domains: ['telegram.org', 't.me', 'telegram.me', 'telesco.pe', 'cdn-telegram.org'] },
  { name: 'SoundCloud', domains: ['soundcloud.com', 'sndcdn.com', 'soundcloud.cloud'] },
  { name: 'LinkedIn', domains: ['linkedin.com', 'licdn.com'] },
  { name: 'Patreon', domains: ['patreon.com', 'patreonusercontent.com'] },
  { name: 'Rutracker', domains: ['rutracker.org', 'rutracker.cc'] },
];

// Re-read on opening the page, but rebuild rows only if something really changed (no lost clicks).
async function loadSettings() {
  // Compare with the state at arrival time: two overlapping loads must not both rebuild the rows.
  let data = null, err = null;
  try { data = await invoke('get_settings'); } catch (e) { err = String(e); }
  if (JSON.stringify([data, err]) === JSON.stringify([ST.data, ST.err])) return syncSettings();
  ST.data = data; ST.err = err; drawSettings();
}
const sameList = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && [...a].sort().join() === [...b].sort().join();

function drawSettings() {
  const box = $('folders'); box.replaceChildren();
  if (ST.err) { box.append(h('div', { class: 'empty-row', text: ST.err })); return; }
  if (!ST.data) { box.append(h('div', { class: 'empty-row', text: 'Загружаем…' })); return; }
  const locked = !!S.busy || ST.saving;
  for (const f of FOLDERS) {
    const custom = ST.data[f.kind + '_custom'];
    box.append(h('div', { class: 'row folder' },
      h('div', { class: 'folder-info' },
        h('div', { class: 'folder-title' }, icon({ downloads: 'download', archive: 'archive', install: 'server' }[f.kind]), h('b', { text: f.title }), custom ? null : h('span', { class: 'chip idle', text: 'ПО УМОЛЧАНИЮ' })),
        h('code', { class: 'path', text: ST.data[f.kind], title: ST.data[f.kind] }),
        h('small', { text: f.desc })),
      h('div', { class: 'folder-actions' },
        custom ? h('button', { type: 'button', class: 'ghost lockable', disabled: locked, 'data-reset': f.kind }, icon('rotate-ccw'), 'Сбросить') : null,
        h('button', { type: 'button', class: 'ghost lockable', disabled: locked, 'data-pick': f.kind }, icon('folder-open'), 'Изменить'))));
  }
  drawDomains(); drawExclude(); drawBypass();
}
// One delegated handler on the stable container: a click survives a row redraw.
$('folders').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b || b.disabled) return;
  if (b.dataset.pick) openPicker(FOLDERS.find(f => f.kind === b.dataset.pick));
  else if (b.dataset.reset) setFolder(b.dataset.reset, null);
});
// Background re-checks only touch state, never rebuild rows under the cursor.
function syncSettings() {
  if (!ST.data) return;
  document.querySelectorAll('#folders .lockable').forEach(b => { b.disabled = !!S.busy || ST.saving; });
  drawDomainStatus();
}

async function setFolder(kind, path) {
  if (ST.saving || S.busy) return;
  ST.saving = true; drawSettings(true);
  try { ST.data = await invoke('set_folder', { kind, path }); toast(path ? 'Папка сохранена.' : 'Вернули папку по умолчанию.', 'ok'); }
  catch (e) { toast(String(e), 'bad'); }
  finally { ST.saving = false; drawSettings(true); if (kind === 'downloads') refresh(); }
}

// ---------- own sites
function drawDomains() {
  const list = $('domain-list'), acts = $('domain-actions');
  list.replaceChildren(); acts.replaceChildren();
  const ds = ST.data.domains || [];
  if (!ds.length) list.append(h('span', { class: 'chips-empty', text: 'Пока пусто. Добавьте домен или готовый набор ниже.' }));
  ds.forEach(d => list.append(h('span', { class: 'dchip' }, d,
    h('button', { type: 'button', 'aria-label': `Убрать ${d}`, title: 'Убрать', disabled: ST.saving, onclick: () => saveDomains(ds.filter(x => x !== d)) }, icon('x')))));
  acts.append(h('span', { class: 'preset-label', text: 'Готовые наборы:' }));
  for (const p of PRESETS) {
    const missing = p.domains.filter(d => !ds.includes(d)), on = !missing.length;
    acts.append(h('button', { type: 'button', class: 'ghost preset' + (on ? ' on' : ''), 'aria-pressed': String(on), title: on ? 'Убрать набор' : 'Добавить набор', disabled: ST.saving,
      onclick: () => saveDomains(on ? ds.filter(d => !p.domains.includes(d)) : [...ds, ...missing]) }, icon(on ? 'check' : 'plus'), p.name));
  }
  drawDomainStatus();
}
// Where the list stands relative to the running service.
function drawDomainStatus() {
  const d = ST.data, ds = d.domains || [];
  const custom = !!(ds.length || (d.exclude || []).length || (d.game && d.game !== 'off') || d.ipset);
  const connected = S.ov && derive(S.ov).connected;
  const st = $('domain-status');
  if (connected && (!sameList(d.applied || [], ds) || d.extra_pending)) {
    st.dataset.tone = 'warn'; st.hidden = false;
    $('domain-status-text').textContent = 'Настройки изменены. Служба работает со старыми — переподключите стратегию.';
    const b = $('domain-apply'); b.hidden = false; b.disabled = !!S.busy || ST.saving;
    b.onclick = () => connect(S.ov.manager.strategy);
  } else if (connected) {
    st.dataset.tone = 'ok'; st.hidden = !custom; $('domain-apply').hidden = true;
    $('domain-status-text').textContent = 'Ваши настройки работают в текущей стратегии.';
  } else {
    st.dataset.tone = 'idle'; st.hidden = !custom; $('domain-apply').hidden = true;
    $('domain-status-text').textContent = 'Применятся при подключении стратегии.';
  }
}
async function saveDomains(next) {
  if (ST.saving) return;
  ST.saving = true; $('domain-error').textContent = '';
  try { ST.data = await invoke('save_domains', { domains: next }); }
  catch (e) { $('domain-error').textContent = String(e); }
  finally { ST.saving = false; drawSettings(true); }
}
$('domain-form').addEventListener('submit', e => {
  e.preventDefault();
  const parts = $('domain-input').value.split(/[\s,;]+/).map(s => s.trim()).filter(Boolean);
  if (!parts.length) { $('domain-error').textContent = 'Введите домен, например soundcloud.com.'; return; }
  const ds = ST.data?.domains || [];
  saveDomains([...ds, ...parts]).then(() => { if (!$('domain-error').textContent) $('domain-input').value = ''; });
});

// ---------- exclusions (list-exclude-user.txt)
function drawExclude() {
  const list = $('exclude-list'); list.replaceChildren();
  const ex = ST.data.exclude || [];
  if (!ex.length) list.append(h('span', { class: 'chips-empty', text: 'Пока пусто.' }));
  ex.forEach(d => list.append(h('span', { class: 'dchip' }, d,
    h('button', { type: 'button', 'aria-label': `Убрать ${d}`, title: 'Убрать', disabled: ST.saving, onclick: () => saveExclude(ex.filter(x => x !== d)) }, icon('x')))));
}
async function saveExclude(next) {
  if (ST.saving) return;
  ST.saving = true; $('exclude-error').textContent = '';
  try { ST.data = await invoke('save_exclude', { domains: next }); }
  catch (e) { $('exclude-error').textContent = String(e); }
  finally { ST.saving = false; drawSettings(true); }
}
$('exclude-form').addEventListener('submit', e => {
  e.preventDefault();
  const parts = $('exclude-input').value.split(/[\s,;]+/).map(s => s.trim()).filter(Boolean);
  if (!parts.length) { $('exclude-error').textContent = 'Введите домен, например sberbank.ru.'; return; }
  const ex = ST.data?.exclude || [];
  saveExclude([...ex, ...parts]).then(() => { if (!$('exclude-error').textContent) $('exclude-input').value = ''; });
});

// ---------- game mode and IP list
let ipBusy = false;
function drawBypass() {
  const g = $('game-mode'); g.value = ST.data.game || 'off'; g.disabled = ST.saving;
  const ip = ST.data.ipset, why = 'Список адресов от Flowseal (Cloudflare, игровые серверы): помогает, когда сайт или игра блокируются по IP.';
  $('ipset-info').textContent = ip ? `Включён: ${Number(ip.count).toLocaleString('ru-RU')} адресов, обновлён ${new Date(ip.time * 1000).toLocaleDateString('ru-RU')}. ${why}` : `Выключен. ${why}`;
  $('ipset-label').textContent = ipBusy ? 'Загружаем…' : ip ? 'Обновить' : 'Загрузить';
  $('ipset-update').disabled = $('ipset-off').disabled = ST.saving || ipBusy;
  $('ipset-off').hidden = !ip;
}
async function bypassCall(cmd, args, okText) {
  if (ST.saving) return;
  ST.saving = true; drawBypass();
  try { ST.data = await invoke(cmd, args); if (okText) toast(okText, 'ok'); }
  catch (e) { toast(errText(e), 'bad'); }
  finally { ST.saving = false; ipBusy = false; drawSettings(true); }
}
$('game-mode').addEventListener('change', e => bypassCall('set_game', { mode: e.target.value }, e.target.value === 'off' ? 'Игровой режим выключен.' : 'Игровой режим включён.'));
$('ipset-update').addEventListener('click', () => { ipBusy = true; bypassCall('update_ipset', {}, 'Список IP загружен.'); });
$('ipset-off').addEventListener('click', () => bypassCall('clear_ipset', {}, 'Список IP выключен.'));

// ---------- in-app folder picker
const PK = { f: null, path: null, back: null, seq: 0, loading: false, creating: false };
// Only the latest navigation wins; while it loads, nothing can be created or chosen in a stale folder.
async function browseTo(path) {
  if (PK.creating) return;
  const seq = ++PK.seq; PK.loading = true; PK.path = null;
  $('picker-error').textContent = '';
  $('picker-ok').disabled = true; $('picker-new').querySelector('button').disabled = true;
  try {
    const r = await invoke('browse', { path });
    if (seq !== PK.seq) return;
    PK.path = r.path; drawPicker(r);
  } catch (e) { if (seq === PK.seq) $('picker-error').textContent = String(e); }
  finally { if (seq === PK.seq) { PK.loading = false; $('picker-new').querySelector('button').disabled = !PK.path; } }
}
function drawPicker(r) {
  const drives = $('picker-drives'); drives.replaceChildren();
  r.drives.forEach(d => drives.append(h('button', { type: 'button', class: 'drive' + (r.path && r.path.toUpperCase().startsWith(d.slice(0, 2).toUpperCase()) ? ' active' : ''), onclick: () => browseTo(d) }, icon('hard-drive'), d.slice(0, 2))));
  const crumbs = $('picker-path'); crumbs.replaceChildren();
  if (r.path) {
    const parts = r.path.split('\\').filter(Boolean);
    parts.forEach((p, i) => {
      if (i) crumbs.append(icon('chevron-right'));
      const target = parts.slice(0, i + 1).join('\\') + (i === 0 ? '\\' : '');
      crumbs.append(h('button', { type: 'button', class: 'crumb', onclick: () => browseTo(target) }, p));
    });
  } else crumbs.append(h('span', { class: 'crumb-hint', text: 'Выберите диск' }));
  const list = $('picker-list'); list.replaceChildren();
  if (r.path && r.parent) list.append(h('button', { type: 'button', class: 'dir up', onclick: () => browseTo(r.parent) }, icon('arrow-up'), 'Наверх'));
  if (r.path && !r.dirs.length) list.append(h('div', { class: 'empty-row', text: 'Здесь нет папок. Можно создать новую ниже.' }));
  r.dirs.forEach(n => list.append(h('button', { type: 'button', class: 'dir', onclick: () => browseTo(r.path.replace(/\\$/, '') + '\\' + n) }, icon('folder'), n)));
  // A bare drive root is rejected by the backend: versions need their own folder.
  const isRoot = !r.path || /^[A-Za-z]:\\?$/.test(r.path);
  $('picker-ok').disabled = isRoot; $('picker-new').hidden = !r.path;
  $('picker-ok').textContent = isRoot ? 'Откройте папку' : 'Выбрать эту папку';
}
function openPicker(f) {
  PK.f = f; PK.back = document.activeElement;
  $('picker-title').textContent = f.title;
  $('picker').hidden = false; $('picker-name').value = '';
  browseTo(ST.data[f.kind + '_custom'] ? ST.data[f.kind].replace(/\\ZapretManager$/i, '') : null);
  $('picker-close').focus();
}
function closePicker() { if (PK.creating) return; ++PK.seq; PK.loading = false; PK.path = null; $('picker').hidden = true; PK.back?.focus?.(); }
$('picker-close').addEventListener('click', closePicker);
$('picker-cancel').addEventListener('click', closePicker);
$('picker').addEventListener('mousedown', e => { if (e.target === $('picker')) closePicker(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape' && !$('picker').hidden) closePicker(); });
$('picker-ok').addEventListener('click', () => { const { f, path } = PK; closePicker(); setFolder(f.kind, path); });
$('picker-new').addEventListener('submit', async e => {
  e.preventDefault();
  if (PK.loading || PK.creating || !PK.path) return;
  PK.creating = true; $('picker-error').textContent = '';
  const wasDisabled = $('picker-ok').disabled;
  $('picker-ok').disabled = true; $('picker-new').querySelector('button').disabled = true;
  try {
    const p = await invoke('make_folder', { parent: PK.path, name: $('picker-name').value });
    PK.creating = false; $('picker-name').value = ''; await browseTo(p);
  } catch (err) {
    $('picker-error').textContent = String(err); $('picker-ok').disabled = wasDisabled;
  } finally { PK.creating = false; $('picker-new').querySelector('button').disabled = !PK.path; }
});

// Redraw when the service state changes (connect/disconnect affects the domain status line).
const baseRender = render;
render = function () { baseRender(); syncSettings(); };
document.querySelector('.nav[data-page="settings"]').addEventListener('click', loadSettings);
loadSettings();
