const { invoke } = window.__TAURI__.core;
const sel = document.getElementById('sel'), dim = document.getElementById('dim');
let start = null, rect = null, scale = 1;

// Rust shows the window, then calls this. No pixels cross IPC: the frozen
// frame stays in Rust and gets cropped there.
window.__arm = () => {
  sel.style.display = dim.style.display = 'none';
  start = rect = null;
  scale = window.devicePixelRatio || 1;
  document.body.style.background = 'rgba(0,0,0,.22)';
};

addEventListener('mousedown', e => { start = { x: e.clientX, y: e.clientY }; document.body.style.background = 'transparent'; });
addEventListener('mousemove', e => {
  if (!start) return;
  const x = Math.min(start.x, e.clientX), y = Math.min(start.y, e.clientY);
  const w = Math.abs(e.clientX - start.x), h = Math.abs(e.clientY - start.y);
  rect = { x, y, w, h };
  Object.assign(sel.style, { display: 'block', left: x + 'px', top: y + 'px', width: w + 'px', height: h + 'px' });
  dim.textContent = `${Math.round(w * scale)} × ${Math.round(h * scale)}`;
  Object.assign(dim.style, { display: 'block', left: x + 'px', top: Math.max(0, y - 24) + 'px' });
});
addEventListener('mouseup', e => {
  if (!rect || rect.w < 2 || rect.h < 2) return invoke('cancel');
  const s = scale;
  invoke('commit', { edit: e.shiftKey, x: Math.round(rect.x * s), y: Math.round(rect.y * s), w: Math.round(rect.w * s), h: Math.round(rect.h * s) });
});
addEventListener('keydown', e => { if (e.key === 'Escape') invoke('cancel'); });
