'use strict';
// Zapret Manager UI. One state snapshot (overview) drives every page; actions run one at a time.
const $ = id => document.getElementById(id);
const invoke = (name, args) => window.__TAURI__?.core.invoke(name, args) ?? Promise.reject(new Error('Запустите приложение Windows.'));
const S = { ov: null, err: null, list: null, listErr: null, listTag: null, busy: null, release: null, seq: 0, checkedAt: null, lastKey: '' };

function h(tag, props = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v; else if (k === 'text') el.textContent = v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v); else if (k === 'disabled') el.disabled = true;
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const c of kids.flat()) if (c != null && c !== false) el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  return el;
}
// Replays a one-shot CSS animation class on an element.
function flash(el, cls, ms = 1000) {
  if (!el) return;
  el.classList.remove(cls); void el.offsetWidth; el.classList.add(cls);
  clearTimeout(el._flashTimer); el._flashTimer = setTimeout(() => el.classList.remove(cls), ms);
}
function icon(name) { return h('span', { 'data-icon': name, 'aria-hidden': 'true' }); }
function actionIcon(label) {
  if (/Скачать/.test(label)) return 'download';
  if (/Провер|Повтор/.test(label)) return 'refresh-cw';
  if (/Отключ|Останов/.test(label)) return 'power';
  if (/Вернуть/.test(label)) return 'rotate-ccw';
  return 'layout-grid';
}
function btn({ label, cls = 'ghost', key, busyLabel, onclick, disabled }) {
  const mine = key && S.busy === key;
  return h('button', { type: 'button', class: cls, disabled: disabled || !!S.busy, onclick }, mine ? [h('span', { class: 'spin', 'aria-hidden': 'true' }), busyLabel || label] : [icon(actionIcon(label)), label]);
}

let toastTimer;
function hideToast() { $('toast').hidden = true; }
function toast(text, tone = 'info', action) {
  clearTimeout(toastTimer);
  const t = $('toast'); t.dataset.tone = tone; $('toast-text').textContent = text;
  const a = $('toast-action'); a.hidden = !action;
  if (action) { a.textContent = action.label; a.onclick = () => { hideToast(); action.fn(); }; }
  // A new message over an open toast fades in again instead of silently resizing in place.
  if (!t.hidden) { t.hidden = true; void t.offsetWidth; }
  t.hidden = false; toastTimer = setTimeout(hideToast, action ? 12000 : 7000);
}
$('toast-close').addEventListener('click', hideToast);

// ---------- strategy names: "general (ALT10).bat" -> group ALT, title ALT10
const base = n => String(n || '').replace(/\.bat$/i, '');
function parseName(name) {
  const tag = (base(name).match(/\(([^)]+)\)/) || [])[1] || '';
  const group = /^ALT\d*$/i.test(tag) ? 'ALT' : /^SIMPLE FAKE/i.test(tag) ? 'Simple Fake' : /^FAKE TLS/i.test(tag) ? 'Fake TLS' : 'Основные';
  return { title: tag || 'general', group };
}
const GROUPS = ['Основные', 'ALT', 'Simple Fake', 'Fake TLS'];
const APP_NAMES = { redshieldvpn: 'RedShield VPN', adguardsvc: 'AdGuard', adguard: 'AdGuard', goodbyedpi: 'GoodbyeDPI', 'warp-svc': 'Cloudflare WARP' };
const appName = n => APP_NAMES[String(n).toLowerCase()] || n;

function derive(ov) {
  const legacyActive = ov.legacy.present && (ov.legacy.running || ov.legacy.automatic);
  const foreign = ov.foreign.length > 0;
  const mgrForeign = ov.manager.present && !ov.manager.owned;
  const cleared = !legacyActive && !foreign && !mgrForeign;
  const connected = ov.manager.present && ov.manager.owned && ov.manager.running;
  return { legacyActive, foreign, mgrForeign, cleared, connected, downloaded: ov.downloaded, ready: cleared && !!ov.downloaded };
}
const arr = v => Array.isArray(v) ? v : v == null ? [] : [v];

let snapshotRequest = 0;
async function refresh() {
  const seq = ++snapshotRequest; S.seq = seq; render();
  try {
    const ov = await invoke('overview');
    if (seq !== S.seq) return;
    ov.foreign = arr(ov.foreign); ov.apps = arr(ov.apps); ov.services = arr(ov.services); ov.dns = ov.dns || {};
    S.ov = ov; S.err = null; S.checkedAt = new Date();
    if (ov.downloaded && ov.downloaded !== S.listTag) loadList(ov.downloaded);
  } catch (e) { if (seq === S.seq) { S.err = String(e); S.ov = null; } }
  if (seq === S.seq) S.seq = 0;
  render();
}
async function loadList(tag) {
  S.listTag = tag;
  try { S.list = (await invoke('strategies')).strategies; S.listErr = null; } catch (e) { S.list = null; S.listErr = String(e); }
  render();
}
async function run(key, fn, serviceOnly = false) {
  if (S.busy) return;
  // Invalidate an older overview so it cannot overwrite this operation.
  ++snapshotRequest; S.seq = 0;
  S.busy = key; render();
  try { await fn(); } catch (e) { toast(String(e), 'bad'); }
  finally {
    if (serviceOnly) {
      try {
        const manager = await invoke('strategy_status');
        if (S.ov) { S.ov.manager = manager; S.err = null; }
      } catch (e) {
        S.ov = null; S.err = 'Не удалось подтвердить состояние службы: ' + String(e);
        toast(S.err, 'bad');
      }
      S.busy = null; render();
      // DNS checks must not hold the strategy animation open.
      refresh();
    } else { await refresh(); S.busy = null; render(); }
  }
}

// ---------- actions
const install = () => run('install', async () => {
  const r = await invoke('install_release'); S.release = r.tag;
  const extra = S.ov && derive(S.ov).connected ? ' Переподключите стратегию, чтобы перейти на неё.' : '';
  toast((r.reused ? `Версия ${r.tag} уже скачана и проверена.` : `Скачана и проверена версия ${r.tag}.`) + extra, 'ok', { label: 'К стратегиям', fn: () => go('strategies') });
});
const checkUpdate = () => run('update', async () => {
  const r = await invoke('latest_release'); S.release = r.tag;
  if (S.ov?.downloaded === r.tag) toast(`У вас последняя версия ${r.tag}.`, 'ok');
  else toast(`Вышла версия ${r.tag}.`, 'info', { label: 'Скачать', fn: install });
});
const connect = name => run('connect:' + name, async () => {
  await invoke('strategy_action', { action: 'connect', name });
  toast(`Включено: ${base(name)}. Проверьте YouTube и Discord; не работает — переключите стратегию.`, 'ok', { label: 'Проверить сайты', fn: () => { go('checks'); $('test').click(); } });
}, true);
const disconnect = () => run('disconnect', async () => { await invoke('strategy_action', { action: 'disconnect', name: null }); toast('Обход выключен.', 'ok'); }, true);

// ---------- navigation
function go(page) {
  document.querySelectorAll('.nav').forEach(b => b.classList.toggle('active', b.dataset.page === page));
  document.querySelectorAll('.page').forEach(p => { p.hidden = p.id !== page; });
  if (page === 'strategies' && S.ov?.downloaded && !S.list) loadList(S.ov.downloaded);
}
document.querySelectorAll('.nav').forEach(b => b.addEventListener('click', () => go(b.dataset.page)));
document.addEventListener('click', e => { const t = e.target.closest('[data-go]'); if (t) go(t.dataset.go); });
$('refresh').addEventListener('click', () => { if (!S.busy) refresh(); });
// After a UAC window or a manual change elsewhere, re-read the state.
window.addEventListener('focus', () => { if (!S.busy && !S.seq && (!S.checkedAt || Date.now() - S.checkedAt > 3000)) refresh(); });

// ---------- render
function render() {
  document.body.dataset.busy = S.busy ? S.busy.split(':')[0] : '';
  $('refresh').disabled = !!S.busy || !!S.seq;
  // A background re-check must not rebuild buttons under the cursor: the "checking" flag only matters
  // before the first snapshot exists; afterwards redraw only when the data really changed.
  // Each section redraws only from its own inputs, so a late strategy list does not rebuild home buttons mid-click.
  const homeKey = JSON.stringify([S.ov, S.err, S.busy, S.release, !S.ov && !!S.seq]);
  const strat = JSON.stringify([homeKey, S.list, S.listErr]);
  if (homeKey !== S.lastKey) { S.lastKey = homeKey; renderHome(); renderSide(); renderVersions(); }
  if (strat !== S.lastStratKey) { S.lastStratKey = strat; renderStrategies(); }
}
function setTone(el, tone, busy) { el.dataset.tone = tone; el.dataset.busy = busy ? '1' : '0'; }

function renderHome() {
  const hero = $('hero'), act = $('hero-action');
  let tone = 'idle', title = 'Проверяем компьютер…', detail = 'Ничего не меняем, только смотрим.', action = null;
  if (S.err) { tone = 'bad'; title = 'Не удалось проверить компьютер'; detail = S.err; action = { label: 'Повторить', fn: refresh }; }
  else if (S.ov) {
    const d = derive(S.ov), m = S.ov.manager;
    if (d.connected) { tone = 'ok'; title = 'Обход включён'; detail = `Стратегия ${base(m.strategy)}${m.automatic ? ' · запускается с Windows' : ''}`; action = { label: 'Сменить стратегию', fn: () => go('strategies') }; }
    else if (d.legacyActive) { tone = 'warn'; title = 'Работает старый Zapret'; detail = 'Отключите его в шаге 2: две копии мешают друг другу.'; }
    else if (d.foreign) { tone = 'warn'; title = 'winws запущен вручную'; detail = 'Остановите его в шаге 2, потом включайте стратегию здесь.'; }
    else { title = 'Обход выключен'; detail = d.downloaded ? 'Выберите стратегию — шаг 4.' : 'Скачайте Zapret — шаг 3.'; }
  }
  if (tone === 'ok' && S.heroTone && S.heroTone !== 'ok') flash(hero, 'celebrate');
  S.heroTone = tone;
  setTone(hero, tone, !!S.seq && !S.ov);
  $('hero-title').textContent = title; $('hero-detail').textContent = detail;
  act.hidden = !action; act.disabled = !!S.busy;
  if (action) { act.textContent = action.label; act.onclick = action.fn; }

  const alerts = $('alerts'), fresh = document.createDocumentFragment();
  const alert = (tone, t, p) => fresh.append(h('div', { class: 'alert', 'data-tone': tone }, icon(tone === 'info' ? 'info' : 'triangle-alert'), h('div', {}, h('b', { text: t }), h('p', { text: p }))));
  if (S.ov) {
    const ov = S.ov;
    const dnsBad = [['YouTube', ov.dns.youtube], ['Discord', ov.dns.discord]].filter(([, v]) => v && v !== 'ok').map(([n]) => n);
    if (dnsBad.length) alert('bad', `DNS блокирует ${dnsBad.join(' и ')}`, 'Провайдер подменяет ответы DNS, и компьютер не может узнать адрес сайта. Zapret это не лечит, поэтому стратегии не помогали. Включите зашифрованный DNS: Параметры Windows → Сеть и Интернет → Ethernet (или Wi-Fi → свойства вашей сети) → Назначение DNS-сервера → Изменить → Вручную → IPv4 → предпочитаемый DNS 1.1.1.1, шифрование «Только зашифрованный (DNS через HTTPS)». Или «Безопасный DNS» в браузере (тогда это предупреждение здесь останется, это нормально).');
    if (ov.services.length) alert('bad', `Найден другой обход: ${ov.services.join(', ')}`, 'Он занимает тот же драйвер WinDivert и ломает Zapret. Удалите его или остановите службу.');
    if (ov.apps.length) alert('warn', `Может мешать: ${ov.apps.map(appName).join(', ')}`, 'VPN и другие обходчики перехватывают тот же трафик. Полностью закройте их, пока работает Zapret.');
    if (ov.proxy) alert('warn', 'Включён системный прокси', 'Если вы им не пользуетесь, выключите: Параметры → Сеть и Интернет → Прокси.');
    if (ov.timestamps === false) alert('info', 'TCP timestamps выключены', 'Включим автоматически при подключении стратегии.');
  }
  const alertsKey = [...fresh.childNodes].map(n => n.dataset.tone + n.textContent).join('|');
  if (alerts.dataset.key !== alertsKey) { alerts.dataset.key = alertsKey; alerts.replaceChildren(fresh); }

  const steps = $('steps'); steps.replaceChildren();
  const step = (n, state, title, desc, actions = []) => steps.append(h('li', { class: 'step ' + state }, h('span', { class: 'num', 'aria-hidden': 'true' }, state === 'done' ? icon('check') : String(n)), h('div', { class: 'step-body' }, h('b', { text: title }), h('p', { text: desc }), h('div', { class: 'step-actions' }, actions))));
  const ov = S.ov;
  if (!ov) {
    step(1, 'current', 'Проверка компьютера', S.err || 'Смотрим службы, процессы winws, VPN и DNS.', S.err ? [btn({ label: 'Повторить', cls: 'primary', onclick: refresh })] : []);
    step(2, 'locked', 'Старый Zapret', 'Станет доступно после проверки.');
    step(3, 'locked', 'Скачать Zapret', 'Станет доступно после шага 2.');
    step(4, 'locked', 'Включить стратегию', 'Станет доступно после шагов 2 и 3.');
    return;
  }
  const d = derive(ov), L = ov.legacy;
  step(1, 'done', 'Проверка компьютера', `Проверено в ${S.checkedAt.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}.`);
  if (d.mgrForeign) step(2, 'current', 'Старый Zapret', 'Есть служба ZapretManagerWinws, созданная не этой программой. Удалите её вручную.');
  else if (d.legacyActive && L.flowseal) step(2, 'current', 'Отключить старый Zapret', `Служба zapret ${L.running ? 'работает' : 'включена в автозапуск'}${L.strategy ? ` (${L.strategy})` : ''}. Отключим её, файлы останутся на месте.`, [btn({ label: 'Отключить старый Zapret', cls: 'primary', key: 'disable', busyLabel: 'Отключаем…', onclick: () => run('disable', async () => toast(await invoke('disable_old_service'), 'ok')) })]);
  else if (d.legacyActive) step(2, 'current', 'Старый Zapret', 'Служба zapret не похожа на Flowseal, программа её не трогает. Остановите её через service.bat → Remove Services.');
  else if (d.foreign) step(2, 'current', 'Остановить winws', 'winws.exe запущен вне службы (обычно это открытый general*.bat). Закроем его.', [btn({ label: 'Остановить winws', cls: 'primary', key: 'kill', busyLabel: 'Останавливаем…', onclick: () => run('kill', async () => { await invoke('strategy_action', { action: 'kill_foreign', name: null }); toast('winws остановлен.', 'ok'); }) })]);
  else step(2, 'done', 'Старый Zapret', L.present ? 'Старая служба отключена.' : 'Старого Zapret нет, конфликтов нет.', L.restorable && !d.connected && !ov.manager.automatic ? [btn({ label: 'Вернуть старую службу', key: 'restore', busyLabel: 'Возвращаем…', onclick: () => run('restore', async () => toast(await invoke('restore_old_service'), 'ok')) })] : []);

  if (!d.cleared) step(3, 'locked', 'Скачать Zapret', 'Станет доступно после шага 2.');
  else if (d.downloaded) {
    const newer = S.release && S.release !== d.downloaded;
    step(3, 'done', 'Скачать Zapret', `Версия ${d.downloaded}, проверена по SHA-256.${newer ? ` Вышла ${S.release}.` : ''}`, [newer ? btn({ label: `Скачать ${S.release}`, cls: 'primary', key: 'install', busyLabel: 'Скачиваем…', onclick: install }) : btn({ label: 'Проверить обновление', key: 'update', busyLabel: 'Проверяем…', onclick: checkUpdate })]);
  } else step(3, 'current', 'Скачать Zapret', 'Официальный архив Flowseal с GitHub, проверка SHA-256.', [btn({ label: 'Скачать', cls: 'primary', key: 'install', busyLabel: 'Скачиваем…', onclick: install })]);

  if (!d.ready) step(4, 'locked', 'Включить стратегию', 'Станет доступно после шагов 2 и 3.');
  else if (d.connected) step(4, 'done', 'Включить стратегию', `Работает ${base(ov.manager.strategy)}. Если YouTube не открывается, переключите стратегию.`, [btn({ label: 'Стратегии', onclick: () => go('strategies') })]);
  else step(4, 'current', 'Включить стратегию', 'Начните с general. Не заработало — включите следующую.', [btn({ label: 'Выбрать стратегию', cls: 'primary', onclick: () => go('strategies') })]);
}

function renderStrategies() {
  const lock = (t, p) => { $('strategy-locked').hidden = false; $('strategy-body').hidden = true; $('locked-title').textContent = t; $('locked-detail').textContent = p; };
  if (!S.ov) { S.autoTarget = null; return lock(S.err ? 'Не удалось проверить компьютер' : 'Проверяем компьютер…', S.err || ''); }
  const d = derive(S.ov), m = S.ov.manager;
  if (!d.cleared) return lock('Сначала уберите старый Zapret', 'Две копии одновременно мешают друг другу. Это шаг 2 на главной.');
  if (!d.downloaded) return lock('Сначала скачайте Zapret', 'Это шаг 3 на главной.');
  if (!S.list) return lock(S.listErr ? 'Не удалось прочитать стратегии' : 'Загружаем стратегии…', S.listErr || '');
  $('strategy-locked').hidden = true; $('strategy-body').hidden = false;

  const connecting = S.busy?.startsWith('connect:'), stopping = S.busy === 'disconnect', autoBusy = S.busy === 'auto';
  const live = d.connected && !connecting && !stopping;
  const bar = $('statusbar');
  if (live && S.wasLive === false) flash(bar, 'celebrate');
  S.wasLive = live;
  setTone(bar, live ? 'ok' : connecting ? 'info' : 'idle', connecting || stopping);
  $('sb-title').textContent = connecting ? 'Подключаем…' : stopping ? 'Отключаем…' : d.connected ? 'Подключено' : 'Отключено';
  $('sb-detail').textContent = connecting ? `${base(S.busy.slice(8))} · подтвердите запрос Windows` : autoBusy ? `${base(m.strategy)} · ${S.autoTarget ? 'включаем' : 'выключаем'} автозапуск…` : d.connected ? `${base(m.strategy)} · Zapret ${d.downloaded}${m.automatic ? ' · запускается с Windows' : ''}` : `Zapret ${d.downloaded} · выберите стратегию ниже`;
  // Autostart: the switch moves right away, a spinner sits in the knob while Windows applies it,
  // then the knob pops (success) or the switch shakes back (error).
  const auto = $('autostart'), sw = $('autostart-switch'), canAuto = !!(m.present && m.owned);
  const autoOn = autoBusy && S.autoTarget != null ? S.autoTarget : !!m.automatic;
  auto.checked = autoOn; auto.disabled = !canAuto || !!S.busy;
  sw.classList.toggle('pending', autoBusy); sw.classList.toggle('on', autoOn && !autoBusy);
  sw.setAttribute('aria-busy', String(autoBusy));
  $('autostart-label').textContent = autoBusy ? (S.autoTarget ? 'Включаем…' : 'Выключаем…') : 'Запуск с Windows';
  sw.title = !canAuto ? 'Сначала подключите стратегию' : autoOn ? 'Стратегия включится сама после перезагрузки' : 'Включать эту стратегию при старте Windows';
  if (!autoBusy && S.autoTarget != null) {
    const ok = !!m.automatic === S.autoTarget; S.autoTarget = null;
    flash(sw, ok ? (m.automatic ? 'pop-on' : 'pop-off') : 'shake', 900);
  }
  const dis = $('disconnect'); dis.disabled = !d.connected || !!S.busy;

  const groups = $('strategy-groups');
  // Preserve cards and buttons across status updates, including keyboard focus.
  const listKey = JSON.stringify(S.list);
  if (groups.dataset.list !== listKey) {
    groups.dataset.list = listKey; groups.replaceChildren();
    const sorted = [...S.list].sort((a, b) => (/\(/.test(a) - /\(/.test(b)) || a.localeCompare(b, 'en', { numeric: true }));
    for (const g of GROUPS) {
      const items = sorted.filter(n => parseName(n).group === g);
      if (!items.length) continue;
      groups.append(h('div', { class: 'group-title', text: g }), h('div', { class: 'grid' }, items.map(name =>
        h('article', { class: 'scard', 'data-strategy': name, 'aria-label': parseName(name).title },
          h('div', { class: 'scard-top' }, h('b', { text: parseName(name).title }), h('span', { class: 'chip', hidden: true, 'aria-live': 'polite' })),
          h('button', { type: 'button', class: 'ghost', onclick: () => {
            if (S.ov && derive(S.ov).connected && S.ov.manager.strategy === name) disconnect(); else connect(name);
          } }, icon('power'), h('span', { class: 'spin', hidden: true, 'aria-hidden': 'true' }), h('span', { class: 'action-label' })),
          h('span', { class: 'card-progress', 'aria-hidden': 'true' })))));
    }
  }
  for (const card of groups.querySelectorAll('.scard')) {
    const name = card.dataset.strategy;
    const active = d.connected && m.strategy === name;
    const mine = S.busy === 'connect:' + name;
    const pending = !!(mine || (active && (stopping || connecting)));
    card.classList.toggle('active', active && !pending);
    card.classList.toggle('pending', pending);
    card.setAttribute('aria-busy', String(pending));
    const chip = card.querySelector('.chip');
    chip.hidden = !active && !mine; chip.className = 'chip' + (pending ? ' idle' : '');
    chip.textContent = mine ? 'Включаем' : pending ? 'Отключаем' : 'Работает';
    const button = card.querySelector('button');
    button.disabled = !!S.busy; button.className = 'ghost' + (active && !pending ? ' danger' : '');
    card.querySelector('.spin').hidden = !pending;
    card.querySelector('button [data-icon]').hidden = pending;
    card.querySelector('.action-label').textContent = mine ? 'Подключаем…' : pending ? 'Отключаем…' : active ? 'Отключить' : d.connected ? 'Переключить' : 'Подключить';
  }
}
$('disconnect').addEventListener('click', disconnect);
$('autostart').addEventListener('change', e => {
  const on = e.target.checked;
  S.autoTarget = on;
  run('auto', async () => { await invoke('strategy_action', { action: on ? 'auto_on' : 'auto_off', name: null }); toast(on ? 'Стратегия будет запускаться вместе с Windows.' : 'Автозапуск выключен.', 'ok'); }, true);
});

function renderSide() {
  const el = $('side-status');
  let tone = 'idle', t = 'Проверяем…', dd = '';
  if (S.err) { tone = 'bad'; t = 'Ошибка проверки'; }
  else if (S.ov) {
    const d = derive(S.ov);
    if (d.connected) { tone = 'ok'; t = 'Обход включён'; dd = base(S.ov.manager.strategy); }
    else if (!d.cleared) { tone = 'warn'; t = 'Есть конфликт'; dd = 'Откройте главную'; }
    else t = 'Обход выключен';
  }
  setTone(el, tone, !!S.busy); $('side-title').textContent = t; $('side-detail').textContent = dd;
}
function renderVersions() {
  const dl = S.ov?.downloaded;
  $('v-downloaded').textContent = dl || 'не скачано';
  $('release').textContent = S.release || 'не проверено';
  $('check-release').disabled = !!S.busy;
  $('release-detail').textContent = !S.release || !dl ? '' : S.release === dl ? 'У вас последняя версия.' : 'Есть новая версия — скачайте её на главной, шаг 3.';
}
$('check-release').addEventListener('click', () => run('update', async () => { S.release = (await invoke('latest_release')).tag; }));

// ---------- site checks (independent of the service lock)
const STORAGE_KEY = 'zapret-manager-sites-v1';
const DEFAULT_SITES = [{ name: 'YouTube', domain: 'www.youtube.com' }, { name: 'Discord', domain: 'discord.com' }, { name: 'SoundCloud', domain: 'soundcloud.com' }];
let checking = false;
function normalizeDomain(input) {
  const raw = input.trim();
  if (!raw || /\s/.test(raw)) throw new Error('Введите домен, например twitch.tv.');
  let url; try { url = new URL(raw.includes('://') ? raw : `https://${raw}`); } catch { throw new Error('Введите корректный домен, например twitch.tv.'); }
  const host = url.hostname.toLowerCase();
  if (url.protocol !== 'https:' || url.username || url.password || url.port || url.pathname !== '/' || url.search || url.hash || !host.includes('.') || host.endsWith('.')) throw new Error('Укажите только домен, например twitch.tv.');
  return host;
}
function loadSites() {
  try {
    const v = JSON.parse(localStorage.getItem(STORAGE_KEY));
    if (Array.isArray(v) && v.length <= 20) return v.filter(x => x && typeof x.name === 'string' && x.name.trim() && x.name.length <= 60 && typeof x.domain === 'string' && (() => { try { return normalizeDomain(x.domain) === x.domain; } catch { return false; } })());
  } catch (_) {}
  return DEFAULT_SITES.map(s => ({ ...s }));
}
let sites = loadSites();
function saveSites() { try { localStorage.setItem(STORAGE_KEY, JSON.stringify(sites)); } catch (_) { $('test-msg').textContent = 'Не удалось сохранить список сайтов.'; } }
function drawSites() {
  const list = $('site-list'); list.replaceChildren();
  if (!sites.length) { list.append(h('div', { class: 'empty-row', text: 'Список пуст — добавьте сайт.' })); return; }
  sites.forEach((site, i) => list.append(h('div', { class: 'site' },
    h('div', { class: 'site-info' }, h('b', { text: site.name }), h('small', { text: site.domain })),
    h('div', { class: 'site-right' }, h('span', { class: 'site-result', text: 'Не проверено' }),
      h('button', { type: 'button', class: 'remove-site', 'aria-label': `Убрать ${site.name}`, title: 'Убрать', disabled: checking, onclick: () => { sites.splice(i, 1); saveSites(); drawSites(); } }, icon('trash-2'))))));
}
drawSites();
$('show-add').addEventListener('click', () => { $('site-form').hidden = false; $('show-add').setAttribute('aria-expanded', 'true'); $('form-error').textContent = ''; $('site-name').focus(); });
$('cancel-add').addEventListener('click', () => { $('site-form').hidden = true; $('show-add').setAttribute('aria-expanded', 'false'); $('form-error').textContent = ''; $('site-form').reset(); });
$('site-form').addEventListener('submit', e => {
  e.preventDefault(); $('form-error').textContent = '';
  try {
    const name = $('site-name').value.trim(); if (!name) throw new Error('Введите название сайта.');
    const domain = normalizeDomain($('site-domain').value);
    if (sites.some(s => s.domain === domain)) throw new Error('Этот домен уже добавлен.');
    if (sites.length >= 20) throw new Error('Можно добавить не больше 20 сайтов.');
    sites.push({ name, domain }); saveSites(); drawSites(); $('cancel-add').click();
  } catch (err) { $('form-error').textContent = err.message; }
});
$('test').addEventListener('click', async () => {
  if (checking) return;
  if (!sites.length) { $('test-msg').textContent = 'Сначала добавьте сайт.'; return; }
  checking = true; $('test').disabled = $('show-add').disabled = true; $('site-form').hidden = true; drawSites();
  const all = [...document.querySelectorAll('.site-result')];
  all.forEach(el => { el.textContent = 'Проверяем…'; el.className = 'site-result pending'; });
  let done = 0; $('test-msg').textContent = `Проверено 0 из ${sites.length}`;
  await Promise.all(sites.map(async (site, i) => {
    let timer;
    try {
      const timeout = new Promise((_, rej) => { timer = setTimeout(() => rej(new Error('время ожидания истекло')), 15000); });
      const r = await Promise.race([invoke('check_site', { domain: site.domain }), timeout]);
      all[i].textContent = `${r.status} · ${r.detail}`; all[i].className = 'site-result ' + (r.status === 'Ответ получен' ? 'good' : 'bad');
    } catch (err) { all[i].textContent = 'Не удалось проверить · ' + String(err); all[i].className = 'site-result bad'; }
    finally { clearTimeout(timer); done++; $('test-msg').textContent = `Проверено ${done} из ${sites.length}`; }
  }));
  checking = false; $('test').disabled = $('show-add').disabled = false;
  document.querySelectorAll('.remove-site').forEach(b => { b.disabled = false; });
});

render();
refresh();
