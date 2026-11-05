// The scrme mascot: builds its own DOM once, then reacts to rapid clicks.
// Usage: mountMascot(document.querySelector('.logo'))   (needs shared/mascot.css)
import { createCombo } from './combo.js';

const PARTICLES = 12;
const GLYPHS = ['♥', '✦', '•'];
const KINDS = ['poke', 'boop', 'dizzy', 'surprise'];
const DURATION = { poke: 340, boop: 800, dizzy: 1400, surprise: 1900 }; // ms, matches mascot.css

const FACE = `
<div class="spin"><div class="bob"><div class="hop">
  <svg class="mascot" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="19" fill="#fff4fb"/>
    <g class="eyes"><ellipse cx="14" cy="18" rx="2.4" ry="3.3" fill="#3b2a6b"/><ellipse cx="26" cy="18" rx="2.4" ry="3.3" fill="#3b2a6b"/>
      <circle cx="14.8" cy="16.8" r=".9" fill="#fff"/><circle cx="26.8" cy="16.8" r=".9" fill="#fff"/></g>
    <g class="o-eyes"><circle cx="13.5" cy="17.5" r="4" fill="#3b2a6b"/><circle cx="26.5" cy="17.5" r="4" fill="#3b2a6b"/>
      <circle cx="14.9" cy="16" r="1.3" fill="#fff"/><circle cx="27.9" cy="16" r="1.3" fill="#fff"/></g>
    <g class="cheek"><ellipse cx="9.5" cy="24.5" rx="3.4" ry="2.1" fill="#ffa3c4"/><ellipse cx="30.5" cy="24.5" rx="3.4" ry="2.1" fill="#ffa3c4"/></g>
    <path class="mouth" d="M15.5 24.5q4.5 5 9 0" fill="none" stroke="#3b2a6b" stroke-width="2.4" stroke-linecap="round"/>
    <ellipse class="o-mouth" cx="20" cy="27" rx="3.2" ry="4" fill="#3b2a6b"/>
  </svg>
</div></div></div>
<i class="sp s1">✦</i><i class="sp s2">✦</i>`;

/** Static, trusted markup only (no user data ever reaches innerHTML). */
function html(markup) {
  const t = document.createElement('template');
  t.innerHTML = markup;
  return t.content;
}

function buildFx() {
  const fx = document.createElement('div');
  fx.className = 'fx';
  fx.setAttribute('aria-hidden', 'true');
  fx.append(html('<i class="ring"></i>'));
  for (let i = 0; i < PARTICLES; i++) {
    const a = (i / PARTICLES) * Math.PI * 2 + (i % 2) * 0.2;
    const dist = 34 + (i % 3) * 11;
    const p = document.createElement('i');
    p.className = 'pt';
    p.textContent = GLYPHS[i % GLYPHS.length];
    p.style.setProperty('--i', i);
    p.style.setProperty('--dx', (Math.cos(a) * dist).toFixed(1));
    p.style.setProperty('--dy', (Math.sin(a) * dist - 10).toFixed(1)); // lean upward
    p.style.setProperty('--r', `${(i % 2 ? 1 : -1) * (40 + ((i * 37) % 80))}deg`);
    fx.append(p);
  }
  const orbit = document.createElement('div');
  orbit.className = 'orbit';
  for (let i = 0; i < 3; i++) {
    const b = document.createElement('b');
    b.textContent = '✦';
    b.style.setProperty('--a', `${i * 120}deg`);
    orbit.append(b);
  }
  fx.append(orbit);
  return fx;
}

export function mountMascot(root, { secrets = true } = {}) {
  root.append(html(FACE));
  if (!secrets || matchMedia('(prefers-reduced-motion: reduce)').matches) return;

  const fx = buildFx();
  root.append(fx);
  root.style.cursor = 'pointer';
  const combo = createCombo();
  let timer;

  const react = kind => {
    root.classList.remove(...KINDS.map(k => `is-${k}`));
    fx.classList.remove('go');
    void root.offsetWidth; // restart CSS animations
    root.classList.add(`is-${kind}`);
    if (kind === 'boop' || kind === 'surprise') {
      fx.dataset.kind = kind;
      fx.classList.add('go');
    }
    clearTimeout(timer);
    timer = setTimeout(() => {
      root.classList.remove(`is-${kind}`);
      fx.classList.remove('go');
    }, DURATION[kind]);
  };

  root.addEventListener('click', () => react(combo(performance.now())));
}
