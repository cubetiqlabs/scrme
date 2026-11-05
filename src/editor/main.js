// Editor controller: wires the document, tools, view, renderer and toolbar together.

import { clampRect, normRect, toHex } from './geom.js';
import { createDoc, nextId, addCmd, removeCmd, setCmd, patchCmd, createHistory } from './doc.js';
import { SHAPES, pick } from './shapes.js';
import { TOOLS, byId, byKey } from './tools.js';
import { createView } from './view.js';
import { createRenderer } from './render.js';
import { createUI } from './ui.js';
import { flatten } from './export.js';

const { invoke } = window.__TAURI__.core;
const $ = id => document.getElementById(id);

const STROKE = { S: 2.5, M: 4.5, L: 8 }; // × unit
const TEXT = { S: 14, M: 20, L: 32 };    // font px × unit

const st = {
  tool: 'arrow', color: '#ff3b30', sizeKey: 'M',
  doc: null, bitmap: null, selected: null, live: null, shift: false, space: false, pointer: null,
  crop: null, hex: null, prevTool: 'arrow',
};

const vp = $('viewport');
const unit = () => Math.max(1, (st.doc?.w ?? 900) / 900);
const sizeFor = type => (type === 'text' ? TEXT : STROKE)[st.sizeKey] * unit();
const dpr = () => window.devicePixelRatio || 1;
const capture = e => { try { vp.setPointerCapture(e.pointerId); } catch { /* synthetic events have no active pointer */ } };

const history = createHistory(() => { ui.setHistory(history.canUndo, history.canRedo); renderer.invalidate(); });
const view = createView($('viewport'), () => { updateInfo(); renderer.invalidate(); });
const renderer = createRenderer($('view'), () => ({
  doc: st.doc, bitmap: st.bitmap, live: st.live, selected: st.selected, view, unit: unit(),
  crop: st.crop, dpr: dpr(),
}));
const ui = createUI({
  onTool: setTool,
  onColor: color => { st.color = color; ui.setColor(color); if (st.selected) history.run(patchCmd(st.selected, { color })); },
  onSize: key => {
    st.sizeKey = key; ui.setSize(key);
    if (st.selected && 'size' in st.selected) history.run(patchCmd(st.selected, { size: sizeFor(st.selected.type) }));
  },
  onUndo: () => undo(), onRedo: () => redo(), onCopy: () => send('copy_rgba', 'Copied'), onSave: () => send('save_rgba', 'Saved'), onPin: () => send('pin_rgba', 'Pinned'),
  onClose: () => invoke('close_editor'),
});

function updateInfo() {
  if (!st.doc) return;
  const w = Math.round(st.doc.w / dpr()), h = Math.round(st.doc.h / dpr());
  ui.setInfo(`${w}×${h}pt · ${Math.round(view.zoom * 100)}%`);
}

function setTool(id) {
  endText(true);
  if (id !== st.tool && st.tool !== 'picker') st.prevTool = st.tool;
  st.tool = id; ui.setTool(id); ui.closePopover();
  if (id !== 'select') select(null);
  cancelCrop(); hideLoupe();
  vp.style.cursor = id === 'select' ? 'default' : id === 'text' ? 'text' : id === 'picker' ? 'none' : 'crosshair';
}

function select(o) { st.selected = o; renderer.invalidate(); }

function undo() { endText(true); if (history.undo() && st.selected && !st.doc.objects.includes(st.selected)) select(null); }
function redo() { endText(true); history.redo(); }

// ── loading ─────────────────────────────────────────────────────────────
async function load() {
  const size = await invoke('edit_size');
  if (!size) return;
  const [w, h] = size;
  const buf = await invoke('edit_bytes'); // Rust releases its copy once we have it
  st.doc = createDoc(w, h);
  setImage(await createImageBitmap(new ImageData(new Uint8ClampedArray(buf), w, h)));
  setTool('arrow'); ui.setColor(st.color); ui.setSize(st.sizeKey); ui.setHistory(false, false);
  updateInfo();
}

/** Swap the base image (load, crop, undo of a crop) and resize everything to match. */
function setImage(bitmap) {
  st.bitmap = bitmap; st.doc.w = bitmap.width; st.doc.h = bitmap.height;
  renderer.resize();
  view.setImage(bitmap.width, bitmap.height);
  updateInfo();
}

// ── output ──────────────────────────────────────────────────────────────
async function send(cmd, done) {
  endText(true);
  if (!st.doc) return;
  const wasSelected = st.selected; st.selected = null;
  try {
    const rgba = flatten(st.doc, st.bitmap, unit());
    const res = await invoke(cmd, rgba, { headers: { w: String(st.doc.w), h: String(st.doc.h) } });
    ui.flash(cmd === 'save_rgba' && typeof res === 'string' ? 'Saved ✓' : `${done} ✓`);
  } catch (e) {
    ui.flash(String(e), true);
  }
  st.selected = wasSelected; renderer.invalidate();
}

// ── pointer input ───────────────────────────────────────────────────────
const toolCtx = () => ({ color: st.color, size: sizeFor('shape'), unit: unit(), shift: st.shift, doc: st.doc, nextId: () => nextId(st.doc) });

vp.addEventListener('pointerdown', e => {
  if (!st.doc) return;
  st.shift = e.shiftKey;
  if (e.button === 1 || (e.button === 0 && st.space)) { st.pointer = { kind: 'pan', x: e.clientX, y: e.clientY }; capture(e); return; }
  if (e.button !== 0) return;
  ui.closePopover();
  endText(true);
  const p = view.toImage(e.clientX, e.clientY);
  const tool = byId[st.tool];

  if (tool.id === 'select') {
    const hit = pick(st.doc, p, 8 / view.scale);
    select(hit);
    if (hit) {
      st.pointer = { kind: 'move', o: hit, from: p, orig: structuredClone(clean(hit)) };
      capture(e);
    }
    return;
  }
  if (tool.text) { openText(p); e.preventDefault(); return; }
  if (tool.picker) { pickColor(p); return; }
  if (tool.crop) { st.crop = { ...p, w: 0, h: 0, _a: p }; st.pointer = { kind: 'crop' }; ui.setHint(null); capture(e); renderer.invalidate(); return; }
  const o = tool.start(p, { ...toolCtx(), size: sizeFor(tool.id) });
  if (!o) return;
  st.live = o; st.pointer = { kind: 'draw', tool };
  capture(e);
  renderer.invalidate();
});

vp.addEventListener('pointermove', e => {
  const ptr = st.pointer;
  st.shift = e.shiftKey;
  if (st.tool === 'picker') hover(e);
  if (!ptr) return;
  if (ptr.kind === 'pan') { view.panBy(e.clientX - ptr.x, e.clientY - ptr.y); ptr.x = e.clientX; ptr.y = e.clientY; return; }
  const p = view.toImage(e.clientX, e.clientY);
  if (ptr.kind === 'crop') {
    const r = normRect(st.crop._a, p), c = clampRect(r, st.doc.w, st.doc.h);
    st.crop = c ? { ...c, _a: st.crop._a } : { x: 0, y: 0, w: 0, h: 0, _a: st.crop._a };
  } else if (ptr.kind === 'draw') ptr.tool.drag(st.live, p, toolCtx());
  else if (ptr.kind === 'move') Object.assign(ptr.o, SHAPES[ptr.o.type].translate(ptr.orig, p.x - ptr.from.x, p.y - ptr.from.y));
  renderer.invalidate();
});

vp.addEventListener('pointerup', e => {
  const ptr = st.pointer; st.pointer = null;
  if (!ptr) return;
  if (ptr.kind === 'crop') {
    if (st.crop.w > 4 && st.crop.h > 4) ui.setHint('↵ Enter to crop · Esc to cancel');
    else cancelCrop();
    renderer.invalidate();
  } else if (ptr.kind === 'draw') {
    const o = st.live; st.live = null;
    if (o && ptr.tool.done(o)) { delete o._a; history.run(addCmd(st.doc, o)); }
    renderer.invalidate();
  } else if (ptr.kind === 'move') {
    const after = SHAPES[ptr.o.type].translate(ptr.o, 0, 0);
    const before = SHAPES[ptr.o.type].translate(ptr.orig, 0, 0);
    if (JSON.stringify(before) !== JSON.stringify(after)) history.run(setCmd(ptr.o, before, after));
  }
});

vp.addEventListener('dblclick', e => {
  if (st.tool === 'crop' && st.crop) { applyCrop(); return; }
  if (st.tool !== 'select') return;
  const hit = pick(st.doc, view.toImage(e.clientX, e.clientY), 8 / view.scale);
  if (hit?.type === 'text') openText(null, hit);
});

/** Object data without render caches (safe for structuredClone). */
const clean = o => Object.fromEntries(Object.entries(o).filter(([k]) => !k.startsWith('_')));

vp.addEventListener('pointerleave', () => { if (st.tool === 'picker') hideLoupe(); });

// ── color picker ────────────────────────────────────────────────────────
// Sampling goes through tiny probe canvases: no full-size 2D canvas is kept just to read pixels.
const probe1 = new OffscreenCanvas(1, 1).getContext('2d', { willReadFrequently: true });
const loupe = Object.assign(document.createElement('canvas'), { className: 'loupe', width: 99, height: 99 });
const probe = new OffscreenCanvas(9, 9);
vp.append(loupe);

function hexAt(p) {
  const x = Math.floor(p.x), y = Math.floor(p.y);
  if (x < 0 || y < 0 || x >= st.doc.w || y >= st.doc.h) return null;
  probe1.clearRect(0, 0, 1, 1);
  probe1.drawImage(st.bitmap, x, y, 1, 1, 0, 0, 1, 1);
  const d = probe1.getImageData(0, 0, 1, 1).data;
  return toHex(d[0], d[1], d[2]);
}

function hover(e) {
  const p = view.toImage(e.clientX, e.clientY);
  st.hex = hexAt(p); ui.setHex(st.hex);
  if (!st.hex) return hideLoupe();
  // 9×9 neighbourhood, magnified without smoothing
  const x = Math.floor(p.x) - 4, y = Math.floor(p.y) - 4;
  const pctx = probe.getContext('2d'); pctx.clearRect(0, 0, 9, 9);
  pctx.drawImage(st.bitmap, x, y, 9, 9, 0, 0, 9, 9);
  const lctx = loupe.getContext('2d'); lctx.imageSmoothingEnabled = false;
  lctx.clearRect(0, 0, 99, 99); lctx.drawImage(probe, 0, 0, 9, 9, 0, 0, 99, 99);
  const r = vp.getBoundingClientRect();
  loupe.style.left = `${e.clientX - r.left + 18}px`; loupe.style.top = `${e.clientY - r.top + 18}px`;
  loupe.style.display = 'block';
}

function hideLoupe() { loupe.style.display = 'none'; st.hex = null; ui.setHex(null); }

function pickColor(p) {
  const hex = hexAt(p);
  if (!hex) return;
  st.color = hex; ui.setColor(hex);
  if (st.selected) history.run(patchCmd(st.selected, { color: hex }));
  ui.flash(`Picked ${hex}`);
  setTool(st.prevTool || 'arrow');
}

async function copyHex() {
  if (!st.hex) return;
  try { await invoke('copy_text', { text: st.hex }); ui.flash(`Copied ${st.hex} ✓`); }
  catch (e) { ui.flash(String(e), true); }
}

// ── crop ────────────────────────────────────────────────────────────────
function cancelCrop() { if (!st.crop) return; st.crop = null; ui.setHint(null); renderer.invalidate(); }

async function applyCrop() {
  const r = st.crop && clampRect(st.crop, st.doc.w, st.doc.h);
  if (!r) return cancelCrop();
  const old = st.bitmap, next = await createImageBitmap(old, r.x, r.y, r.w, r.h);
  const shift = (dx, dy) => { for (const o of st.doc.objects) Object.assign(o, SHAPES[o.type].translate(o, dx, dy)); };
  st.crop = null; ui.setHint(null); select(null);
  history.run({
    do: () => { shift(-r.x, -r.y); setImage(next); },
    undo: () => { shift(r.x, r.y); setImage(old); },
  });
}

// ── inline text editing ─────────────────────────────────────────────────
let editor = null; // { ta, at, obj }

function openText(p, existing = null) {
  endText(true);
  const o = existing, at = o ? { x: o.x, y: o.y } : p;
  const ta = document.createElement('textarea');
  ta.className = 'ta'; ta.rows = 1; ta.spellcheck = false; ta.value = o ? o.text : '';
  const size = o ? o.size : sizeFor('text');
  const place = () => {
    const c = view.toViewport(at);
    Object.assign(ta.style, { left: `${c.x}px`, top: `${c.y}px`, fontSize: `${size * view.scale}px`, color: o ? o.color : st.color });
    ta.style.height = 'auto'; ta.style.height = `${ta.scrollHeight}px`;
  };
  ta.addEventListener('input', place);
  ta.addEventListener('keydown', ev => {
    ev.stopPropagation();
    if (ev.key === 'Escape') { endText(false); }
    else if (ev.key === 'Enter' && !ev.shiftKey) { ev.preventDefault(); endText(true); }
  });
  ta.addEventListener('blur', () => endText(true));
  vp.append(ta); place(); ta.focus();
  editor = { ta, at, obj: o, size };
  if (o) { o.hidden = true; renderer.invalidate(); }
}

function endText(commit) {
  if (!editor) return;
  const { ta, at, obj, size } = editor; editor = null;
  const text = ta.value.replace(/\s+$/, '');
  ta.remove();
  if (obj) {
    obj.hidden = false;
    if (commit && text && text !== obj.text) history.run(patchCmd(obj, { text }));
    else if (commit && !text) { history.run(removeCmd(st.doc, obj)); select(null); }
  } else if (commit && text) {
    history.run(addCmd(st.doc, { id: nextId(st.doc), type: 'text', color: st.color, size, x: at.x, y: at.y, text }));
  }
  renderer.invalidate();
}

// ── keyboard ────────────────────────────────────────────────────────────
addEventListener('keydown', e => {
  if (editor) return;
  const meta = e.metaKey || e.ctrlKey, k = e.key.toLowerCase();
  if (meta) {
    if (k === 'z') { e.preventDefault(); e.shiftKey ? redo() : undo(); }
    else if (k === 'c') { e.preventDefault(); send('copy_rgba', 'Copied'); }
    else if (k === 's') { e.preventDefault(); send('save_rgba', 'Saved'); }
    else if (k === 'p') { e.preventDefault(); send('pin_rgba', 'Pinned'); }
    else if (k === '=' || k === '+') { e.preventDefault(); view.zoomBy(1.25); }
    else if (k === '-') { e.preventDefault(); view.zoomBy(0.8); }
    else if (k === '0') { e.preventDefault(); e.shiftKey ? view.actual() : view.fit(); }
    return;
  }
  if (e.key === 'Tab' && st.tool === 'picker') { e.preventDefault(); copyHex(); return; }
  if (e.key === 'Enter' && st.crop) { e.preventDefault(); applyCrop(); return; }
  if (e.key === ' ') { st.space = true; vp.style.cursor = 'grab'; e.preventDefault(); return; }
  if (e.key === 'Escape') { if (st.crop) cancelCrop(); else if (st.selected) select(null); else invoke('close_editor'); return; }
  if ((e.key === 'Delete' || e.key === 'Backspace') && st.selected) {
    e.preventDefault(); history.run(removeCmd(st.doc, st.selected)); select(null); return;
  }
  if (e.key.startsWith('Arrow') && st.selected) {
    e.preventDefault();
    const step = e.shiftKey ? 10 : 1, d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    const o = st.selected, before = SHAPES[o.type].translate(o, 0, 0), after = SHAPES[o.type].translate(o, d[0], d[1]);
    history.run(setCmd(o, before, after));
    return;
  }
  if (byKey[k]) setTool(byKey[k]);
});
addEventListener('keyup', e => {
  if (e.key === ' ') { st.space = false; setTool(st.tool); }
});

/** Forget the current image (window is hidden but kept warm): frees the bitmap and canvases. */
function reset() {
  endText(false); cancelCrop(); hideLoupe();
  st.selected = null; st.live = null; st.pointer = null;
  history.reset();
  st.bitmap?.close?.(); st.bitmap = null;
  renderer.invalidate();
  st.doc = null; ui.setInfo('');
}

// Rust calls these on a warm (hidden) window: __open loads the new image, __reset releases it.
window.__open = () => { reset(); return load().catch(err => ui.flash(String(err), true)); };
window.__reset = reset;

new ResizeObserver(() => renderer.resize()).observe($('viewport'));

// Frameless window: the toolbar is the drag handle; the page loads its own image.
load().catch(err => ui.flash(String(err), true));
