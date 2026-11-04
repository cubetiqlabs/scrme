const { invoke } = window.__TAURI__.core;
const cv = document.getElementById('c'), ctx = cv.getContext('2d');
const $ = id => document.getElementById(id);
const TOOLS = { arrow: '↗', rect: '▭', line: '╱', hl: '▮', blur: '▦', num: '①', text: 'T' };
let base = null, shapes = [], redo = [], tool = 'arrow', cur = null, count = 0;

for (const [k, label] of Object.entries(TOOLS)) {
  const b = document.createElement('button');
  b.textContent = label; b.title = k; b.dataset.t = k;
  b.onclick = () => setTool(k);
  $('tools').append(b);
}
function setTool(t) {
  tool = t;
  for (const b of $('tools').children) b.classList.toggle('on', b.dataset.t === t);
}
setTool('arrow');

window.__load = async () => {
  const [w, h] = await invoke('edit_size');
  const buf = await invoke('edit_bytes');
  cv.width = w; cv.height = h;
  base = new ImageData(new Uint8ClampedArray(buf), w, h);
  const off = new OffscreenCanvas(w, h);
  off.getContext('2d').putImageData(base, 0, 0);
  base = off;
  shapes = []; redo = []; count = 0; cur = null;
  draw();
};

const lw = () => Math.max(3, cv.width / 300); // stroke scales with image

function drawShape(s) {
  ctx.strokeStyle = ctx.fillStyle = s.color;
  ctx.lineWidth = lw(); ctx.lineCap = 'round';
  const { x1, y1, x2, y2 } = s;
  switch (s.t) {
    case 'rect': ctx.strokeRect(x1, y1, x2 - x1, y2 - y1); break;
    case 'line': ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke(); break;
    case 'arrow': {
      const a = Math.atan2(y2 - y1, x2 - x1), hd = lw() * 5;
      ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(x2, y2);
      ctx.lineTo(x2 - hd * Math.cos(a - .45), y2 - hd * Math.sin(a - .45));
      ctx.lineTo(x2 - hd * Math.cos(a + .45), y2 - hd * Math.sin(a + .45));
      ctx.closePath(); ctx.fill(); break;
    }
    case 'hl': ctx.globalAlpha = .35; ctx.fillRect(x1, y1, x2 - x1, y2 - y1); ctx.globalAlpha = 1; break;
    case 'blur': { // pixelate from the base image
      const x = Math.min(x1, x2), y = Math.min(y1, y2), w = Math.abs(x2 - x1), h = Math.abs(y2 - y1);
      if (w < 1 || h < 1) break;
      const f = Math.max(6, cv.width / 120), t = new OffscreenCanvas(Math.ceil(w / f), Math.ceil(h / f));
      t.getContext('2d').drawImage(base, x, y, w, h, 0, 0, t.width, t.height);
      ctx.imageSmoothingEnabled = false; ctx.drawImage(t, 0, 0, t.width, t.height, x, y, w, h);
      ctx.imageSmoothingEnabled = true; break;
    }
    case 'num': {
      const r = lw() * 4;
      ctx.beginPath(); ctx.arc(x1, y1, r, 0, 7); ctx.fill();
      ctx.fillStyle = '#fff'; ctx.font = `bold ${r * 1.3}px -apple-system,sans-serif`;
      ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText(s.n, x1, y1 + 1); break;
    }
    case 'text':
      ctx.font = `bold ${lw() * 7}px -apple-system,sans-serif`; ctx.textBaseline = 'top'; ctx.textAlign = 'left';
      ctx.fillText(s.text, x1, y1); break;
  }
}
function draw() {
  if (!base) return;
  ctx.drawImage(base, 0, 0);
  for (const s of shapes) drawShape(s);
  if (cur) drawShape(cur);
}

const pos = e => {
  const r = cv.getBoundingClientRect();
  return [(e.clientX - r.left) * cv.width / r.width, (e.clientY - r.top) * cv.height / r.height];
};
const commit = s => { shapes.push(s); redo = []; draw(); };

cv.onmousedown = e => {
  const [x, y] = pos(e), color = $('col').value;
  if (tool === 'num') return commit({ t: 'num', x1: x, y1: y, color, n: ++count });
  if (tool === 'text') {
    const text = prompt('Text'); // ponytail: native prompt; inline input if it feels clunky
    if (text) commit({ t: 'text', x1: x, y1: y, color, text });
    return;
  }
  cur = { t: tool, x1: x, y1: y, x2: x, y2: y, color };
};
cv.onmousemove = e => { if (cur) { [cur.x2, cur.y2] = pos(e); draw(); } };
addEventListener('mouseup', () => { if (cur) { const s = cur; cur = null; commit(s); } });

const undo = () => { const s = shapes.pop(); if (s) { redo.push(s); if (s.t === 'num') count--; draw(); } };
const redoFn = () => { const s = redo.pop(); if (s) { shapes.push(s); if (s.t === 'num') count++; draw(); } };
$('undo').onclick = undo; $('redo').onclick = redoFn;

function flat() { // render without the in-progress shape
  const c = cur; cur = null; draw(); cur = c;
  return ctx.getImageData(0, 0, cv.width, cv.height).data.buffer;
}
const send = (cmd) => invoke(cmd, flat(), { headers: { w: String(cv.width), h: String(cv.height) } });
const say = t => { $('msg').textContent = t; setTimeout(() => $('msg').textContent = '', 1800); };
const doCopy = () => send('copy_rgba').then(() => { say('Copied'); }).catch(e => say(String(e)));
const doSave = () => send('save_rgba').then(p => say('Saved'), e => say(String(e)));
$('copy').onclick = doCopy; $('save').onclick = doSave;

addEventListener('keydown', e => {
  const m = e.metaKey || e.ctrlKey;
  if (e.key === 'Escape') invoke('close_editor');
  else if (m && e.key === 'c') { e.preventDefault(); doCopy(); }
  else if (m && e.key === 's') { e.preventDefault(); doSave(); }
  else if (m && e.key === 'z') { e.preventDefault(); e.shiftKey ? redoFn() : undo(); }
});
