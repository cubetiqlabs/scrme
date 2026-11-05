// Toolbar, style popover and status chip. All markup here is static/trusted (no user data).

import { TOOLS } from './tools.js';
import { icon } from './icons.js';

export const COLORS = ['#ff3b30', '#ff9500', '#ffcc00', '#34c759', '#0a84ff', '#7c5cff', '#ff6bd6', '#1c1c1e', '#ffffff'];
export const SIZES = ['S', 'M', 'L'];

const button = (attrs, inner) => `<button class="b" ${attrs}>${inner}</button>`;

export function createUI(h) {
  const bar = document.getElementById('bar');
  bar.innerHTML = `
    <button class="x" id="close" title="Close (Esc)" aria-label="Close">${icon('close', 'sm')}</button>
    <div class="grp">
      ${button('id="copy" title="Copy (⌘C)"', icon('copy'))}
      ${button('id="save" title="Save to Pictures/scrme (⌘S)"', icon('save'))}
    </div>
    <i class="sep"></i>
    <div class="grp" id="tools">
      ${TOOLS.map(t => button(`data-tool="${t.id}" title="${t.label} (${t.key.toUpperCase()})"`, icon(t.id))).join('')}
    </div>
    <i class="sep"></i>
    <button class="b style" id="style" title="Color and size">
      <span class="dot" id="dot"></span><span class="lbl" id="sizeLbl">M</span>
    </button>
    <i class="sep"></i>
    <div class="grp">
      ${button('id="undo" title="Undo (⌘Z)"', icon('undo'))}
      ${button('id="redo" title="Redo (⇧⌘Z)"', icon('redo'))}
    </div>
    <span class="grow" data-tauri-drag-region></span>
    <span class="chip" id="hint" hidden></span>
    <span class="chip hex" id="hex" hidden><i id="hexDot"></i><b id="hexVal"></b> ⇥ copy</span>
    <span class="chip" id="flash" hidden></span>
    <span class="chip" id="info"></span>
    <div class="pop" id="pop" hidden>
      <div class="swatches">${COLORS.map(c => `<button class="sw" data-color="${c}" style="--c:${c}" aria-label="${c}"></button>`).join('')}
        <label class="sw custom" title="Custom color"><input type="color" id="custom" value="#ff3b30"></label></div>
      <div class="sizes">${SIZES.map(s => `<button class="sz" data-size="${s}">${s}</button>`).join('')}</div>
    </div>`;

  const $ = id => document.getElementById(id);
  const pop = $('pop');
  let flashTimer;

  bar.addEventListener('click', e => {
    const t = e.target.closest('[data-tool]'), c = e.target.closest('[data-color]'), z = e.target.closest('[data-size]');
    if (t) h.onTool(t.dataset.tool);
    else if (c) h.onColor(c.dataset.color);
    else if (z) h.onSize(z.dataset.size);
    else if (e.target.closest('#style')) {
      pop.hidden = !pop.hidden;
      pop.style.left = `${$('style').getBoundingClientRect().left - bar.getBoundingClientRect().left}px`;
    }
    else if (e.target.closest('#undo')) h.onUndo();
    else if (e.target.closest('#redo')) h.onRedo();
    else if (e.target.closest('#copy')) h.onCopy();
    else if (e.target.closest('#save')) h.onSave();
    else if (e.target.closest('#close')) h.onClose();
  });
  $('custom').addEventListener('input', e => h.onColor(e.target.value));
  document.addEventListener('pointerdown', e => { if (!pop.hidden && !e.target.closest('#pop, #style')) pop.hidden = true; });

  return {
    setTool(id) { for (const b of document.querySelectorAll('[data-tool]')) b.classList.toggle('on', b.dataset.tool === id); },
    setColor(c) {
      $('dot').style.background = c; $('custom').value = c;
      for (const b of document.querySelectorAll('[data-color]')) b.classList.toggle('on', b.dataset.color === c);
    },
    setSize(s) {
      $('sizeLbl').textContent = s;
      for (const b of document.querySelectorAll('[data-size]')) b.classList.toggle('on', b.dataset.size === s);
    },
    setHistory(canUndo, canRedo) { $('undo').disabled = !canUndo; $('redo').disabled = !canRedo; },
    setInfo(text) { $('info').textContent = text; },
    flash(text, bad = false) {
      const f = $('flash');
      f.textContent = text; f.hidden = false; f.classList.toggle('bad', bad);
      clearTimeout(flashTimer); flashTimer = setTimeout(() => { f.hidden = true; }, 1600);
    },
    setHex(hex) {
      const c = $('hex'); c.hidden = !hex;
      if (hex) { $('hexVal').textContent = hex; $('hexDot').style.background = hex; }
    },
    setHint(text) { const c = $('hint'); c.hidden = !text; c.textContent = text ?? ''; },
    closePopover() { pop.hidden = true; },
  };
}
