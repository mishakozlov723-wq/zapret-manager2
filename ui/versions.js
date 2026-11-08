'use strict';
// Old Zapret versions: list them and delete the ones the user ticks (archives and unpacked service copies).
// Uses helpers from app.js ($, h, invoke, toast, S).
const VR = { list: null, picked: new Set(), busy: false, back: null };
const fmtSize = b => b >= 1048576 ? (b / 1048576).toFixed(1).replace(/\.0$/, '') + ' МБ' : Math.max(1, Math.round(b / 1024)) + ' КБ';
const fmtDate = ms => ms ? new Date(ms).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', year: 'numeric' }) : '';
const pickable = () => (VR.list || []).filter(v => !v.used);

async function loadOld() {
  try { VR.list = await invoke('list_versions'); }
  catch (e) { VR.list = null; $('old-summary').textContent = 'не удалось проверить'; $('old-open').disabled = true; return; }
  drawOldSummary();
}
function drawOldSummary() {
  const old = pickable();
  $('old-summary').textContent = old.length ? `${old.length} шт. · ${fmtSize(old.reduce((s, v) => s + v.size, 0))}` : 'нет';
  $('old-open').disabled = !old.length || VR.busy;
}
function drawCleaner() {
  const list = $('cleaner-list'); list.replaceChildren();
  for (const v of VR.list || []) {
    const box = h('input', { type: 'checkbox', disabled: v.used || VR.busy, 'aria-label': v.tag });
    box.checked = VR.picked.has(v.id);
    box.addEventListener('change', () => { if (box.checked) VR.picked.add(v.id); else VR.picked.delete(v.id); syncCleaner(); });
    list.append(h('label', { class: 'vrow' + (v.used ? ' used' : '') }, box,
      h('span', { class: 'vname' }, h('b', { text: v.tag }), h('small', { text: [fmtDate(v.date), fmtSize(v.size)].filter(Boolean).join(' · ') })),
      v.used ? h('span', { class: 'chip', text: v.note.toUpperCase() }) : null));
  }
  if (!(VR.list || []).length) list.append(h('div', { class: 'empty-row', text: 'Версий пока нет.' }));
  syncCleaner();
}
function syncCleaner() {
  const n = VR.picked.size, free = (VR.list || []).filter(v => VR.picked.has(v.id)).reduce((s, v) => s + v.size, 0);
  const ok = $('cleaner-ok'); ok.disabled = !n || VR.busy;
  ok.textContent = VR.busy ? 'Удаляем…' : n ? `Удалить (${n}) · ${fmtSize(free)}` : 'Удалить';
  const p = pickable(), all = $('cleaner-all');
  all.textContent = p.length && p.every(v => VR.picked.has(v.id)) ? 'Снять все' : 'Выбрать все';
  all.disabled = !p.length || VR.busy;
  $('cleaner-cancel').disabled = VR.busy;
}
async function openCleaner() {
  VR.back = document.activeElement; VR.picked.clear(); $('cleaner-error').textContent = '';
  $('cleaner').hidden = false; drawCleaner();
  await loadOld(); drawCleaner(); $('cleaner-close').focus();
}
function closeCleaner() { if (VR.busy) return; $('cleaner').hidden = true; VR.back?.focus?.(); }
$('old-open').addEventListener('click', openCleaner);
$('cleaner-close').addEventListener('click', closeCleaner);
$('cleaner-cancel').addEventListener('click', closeCleaner);
$('cleaner').addEventListener('mousedown', e => { if (e.target === $('cleaner')) closeCleaner(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape' && !$('cleaner').hidden) closeCleaner(); });
$('cleaner-all').addEventListener('click', () => {
  const p = pickable(), all = p.every(v => VR.picked.has(v.id));
  p.forEach(v => { if (all) VR.picked.delete(v.id); else VR.picked.add(v.id); });
  drawCleaner();
});
$('cleaner-ok').addEventListener('click', async () => {
  if (VR.busy || !VR.picked.size) return;
  if (S.busy) { $('cleaner-error').textContent = 'Дождитесь окончания текущей операции.'; return; }
  const n = VR.picked.size;
  VR.busy = true; $('cleaner-error').textContent = ''; drawCleaner();
  try {
    VR.list = await invoke('delete_versions', { ids: [...VR.picked] });
    VR.picked.clear(); VR.busy = false; $('cleaner').hidden = true; VR.back?.focus?.();
    toast(n === 1 ? 'Старая версия удалена.' : `Удалено версий: ${n}.`, 'ok');
  } catch (e) { $('cleaner-error').textContent = String(e); await loadOld(); }
  finally { VR.busy = false; if (!$('cleaner').hidden) drawCleaner(); drawOldSummary(); }
});
document.querySelector('.nav[data-page="versions"]').addEventListener('click', loadOld);
loadOld();
