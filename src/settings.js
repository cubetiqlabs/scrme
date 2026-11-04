const { invoke } = window.__TAURI__.core;
const $ = id => document.getElementById(id);

// Same glyphs as the tray menu, inlined (purple strokes) so Preferences matches it.
const ICONS = {
  area: '<path d="M3 9V4.5A1.5 1.5 0 0 1 4.5 3H9M15 3h4.5A1.5 1.5 0 0 1 21 4.5V9M21 15v4.5a1.5 1.5 0 0 1-1.5 1.5H15M9 21H4.5A1.5 1.5 0 0 1 3 19.5V15"/><circle cx="12" cy="12" r="1.6" fill="currentColor"/>',
  full: '<rect x="3" y="4" width="18" height="12" rx="2.5"/><path d="M12 16v4M8 20h8"/>',
  window: '<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M3 9.5h18"/>',
};
const FIELDS = [['area', 'Capture area'], ['full', 'Capture screen'], ['window', 'Capture window']];
let cfg = {}, recording = null, timer;

const SYM = { Cmd: '⌘', Super: '⌘', Shift: '⇧', Alt: '⌥', Option: '⌥', Ctrl: '⌃', Control: '⌃' };
const caps = acc => acc.split('+').map(p => Object.assign(document.createElement('span'), { className: 'cap', textContent: SYM[p] || p }));

function render() {
  const box = $('keys'); box.replaceChildren();
  for (const [k, label] of FIELDS) {
    const row = document.createElement('div'); row.className = 'row';
    const l = document.createElement('span'); l.className = 'lbl';
    l.insertAdjacentHTML('beforeend', `<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="#7c5cff" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${ICONS[k]}</svg>`);
    l.append(label);
    const b = document.createElement('button'); b.className = 'keys' + (recording === k ? ' rec' : '');
    b.append(...caps(cfg[k]), Object.assign(document.createElement('span'), { className: 'hint', textContent: 'Press new shortcut…' }));
    b.onclick = () => { recording = k; render(); };
    row.append(l, b); box.append(row);
  }
  $('animation').checked = cfg.animation; $('autosave').checked = cfg.autosave;
}

// KeyboardEvent -> tauri accelerator string
function accel(e) {
  const mods = [e.metaKey && 'Cmd', e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift'].filter(Boolean);
  if (!mods.length || /^(Meta|Control|Alt|Shift)/.test(e.code)) return null;
  return [...mods, e.code.replace(/^Key/, '').replace(/^Digit/, '')].join('+');
}

function say(t, ok) {
  const m = $('msg'); m.textContent = t; m.className = 'show ' + (ok ? 'ok' : 'err');
  clearTimeout(timer); timer = setTimeout(() => m.classList.remove('show'), 1800);
}

// Every change saves immediately; on error revert to what Rust last accepted.
async function save(next) {
  try { await invoke('set_config', { cfg: next }); cfg = next; say('Saved ✓', true); }
  catch (e) { say(String(e), false); }
  render();
}

addEventListener('keydown', e => {
  if (!recording) return;
  e.preventDefault();
  if (e.key === 'Escape') { recording = null; return render(); }
  const a = accel(e);
  if (a) { const k = recording; recording = null; save({ ...cfg, [k]: a }); }
});
addEventListener('mousedown', e => { if (recording && !e.target.closest('.rec')) { recording = null; render(); } });
for (const id of ['animation', 'autosave']) $(id).onchange = () => save({ ...cfg, [id]: $(id).checked });
$('reset').onclick = async () => save(await invoke('default_config'));

invoke('get_config').then(c => { cfg = c; render(); });
window.__TAURI__.app?.getVersion().then(v => { $('ver').textContent = `Screenshots, instantly · v${v}`; });
