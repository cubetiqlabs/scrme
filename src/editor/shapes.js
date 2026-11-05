// Shape registry: one entry per object type = draw + hit-test + bounds + translate.
// Objects are plain data ({id, type, color, size, ...geometry}); sizes are in image pixels.

import { distToSegment, inRect, boundsOf, arrowBarbs } from './geom.js';

const FONT = '-apple-system, BlinkMacSystemFont, "SF Pro Rounded", "Helvetica Neue", sans-serif';
const measureCtx = typeof document === 'undefined' ? null : document.createElement('canvas').getContext('2d');

export const textMetrics = o => {
  const ctx = measureCtx;
  ctx.font = `700 ${o.size}px ${FONT}`;
  const lines = o.text.split('\n');
  const w = Math.max(...lines.map(l => ctx.measureText(l).width));
  return { lines, w, h: lines.length * o.size * 1.25 };
};

const stroke = (ctx, o) => {
  ctx.strokeStyle = o.color; ctx.lineWidth = o.size;
  ctx.lineCap = 'round'; ctx.lineJoin = 'round';
};
const ptsOfLine = o => [{ x: o.x1, y: o.y1 }, { x: o.x2, y: o.y2 }];
const rectBounds = o => ({ x: o.x, y: o.y, w: o.w, h: o.h });
const moveRect = (o, dx, dy) => ({ x: o.x + dx, y: o.y + dy });
const hitRect = (o, p, tol) => inRect(rectBounds(o), p, tol);

export const SHAPES = {
  rect: {
    draw(ctx, o) {
      stroke(ctx, o);
      const r = Math.min(o.size * 1.5, o.w / 2, o.h / 2);
      ctx.beginPath(); ctx.roundRect(o.x, o.y, o.w, o.h, r); ctx.stroke();
    },
    hit: hitRect, bounds: rectBounds, translate: moveRect,
  },
  ellipse: {
    draw(ctx, o) {
      stroke(ctx, o);
      ctx.beginPath(); ctx.ellipse(o.x + o.w / 2, o.y + o.h / 2, o.w / 2, o.h / 2, 0, 0, Math.PI * 2); ctx.stroke();
    },
    hit: hitRect, bounds: rectBounds, translate: moveRect,
  },
  highlighter: {
    draw(ctx, o) { ctx.save(); ctx.globalAlpha = 0.38; ctx.fillStyle = o.color; ctx.fillRect(o.x, o.y, o.w, o.h); ctx.restore(); },
    hit: hitRect, bounds: rectBounds, translate: moveRect,
  },
  line: {
    draw(ctx, o) { stroke(ctx, o); ctx.beginPath(); ctx.moveTo(o.x1, o.y1); ctx.lineTo(o.x2, o.y2); ctx.stroke(); },
    hit: (o, p, tol) => distToSegment(p, { x: o.x1, y: o.y1 }, { x: o.x2, y: o.y2 }) <= tol + o.size / 2,
    bounds: o => boundsOf(ptsOfLine(o), o.size),
    translate: (o, dx, dy) => ({ x1: o.x1 + dx, y1: o.y1 + dy, x2: o.x2 + dx, y2: o.y2 + dy }),
  },
  arrow: {
    // A tapered look: slimmer shaft that ends inside a generous, softly rounded head.
    draw(ctx, o) {
      const a = { x: o.x1, y: o.y1 }, b = { x: o.x2, y: o.y2 };
      const len = Math.hypot(b.x - a.x, b.y - a.y);
      if (len < 1) return;
      const ang = Math.atan2(b.y - a.y, b.x - a.x);
      const head = Math.min(Math.max(o.size * 4.2, 16), len);      // barb length
      const base = { x: b.x - Math.cos(ang) * head * 0.78, y: b.y - Math.sin(ang) * head * 0.78 };
      ctx.strokeStyle = o.color; ctx.fillStyle = o.color; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
      ctx.lineWidth = o.size * 0.62;
      ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(base.x, base.y); ctx.stroke();
      const [p, q] = arrowBarbs(a, b, head, 0.5);
      ctx.lineWidth = o.size * 0.3; // rounds the three corners of the head
      ctx.beginPath(); ctx.moveTo(b.x, b.y); ctx.lineTo(p.x, p.y); ctx.lineTo(q.x, q.y); ctx.closePath(); ctx.fill(); ctx.stroke();
    },
    hit: (o, p, tol) => distToSegment(p, { x: o.x1, y: o.y1 }, { x: o.x2, y: o.y2 }) <= tol + o.size / 2,
    bounds: o => boundsOf(ptsOfLine(o), o.size * 3),
    translate: (o, dx, dy) => ({ x1: o.x1 + dx, y1: o.y1 + dy, x2: o.x2 + dx, y2: o.y2 + dy }),
  },
  ruler: {
    // A measured line: end caps plus a pill label with the length in points.
    draw(ctx, o, env) {
      stroke(ctx, o);
      const a = { x: o.x1, y: o.y1 }, b = { x: o.x2, y: o.y2 };
      const ang = Math.atan2(b.y - a.y, b.x - a.x), cap = o.size * 2.5;
      const nx = -Math.sin(ang) * cap, ny = Math.cos(ang) * cap;
      ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y);
      ctx.moveTo(a.x - nx, a.y - ny); ctx.lineTo(a.x + nx, a.y + ny);
      ctx.moveTo(b.x - nx, b.y - ny); ctx.lineTo(b.x + nx, b.y + ny); ctx.stroke();
      const label = `${Math.round(Math.hypot(b.x - a.x, b.y - a.y) / (env.dpr || 1))}pt`;
      const fs = o.size * 4.2;
      ctx.font = `700 ${fs}px ${FONT}`;
      const w = ctx.measureText(label).width + fs * 0.9, h = fs * 1.5;
      const cx = (a.x + b.x) / 2, cy = (a.y + b.y) / 2;
      ctx.fillStyle = o.color; ctx.beginPath(); ctx.roundRect(cx - w / 2, cy - h / 2, w, h, h / 2); ctx.fill();
      ctx.fillStyle = '#fff'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(label, cx, cy + fs * 0.05); ctx.textAlign = 'left';
    },
    hit: (o, p, tol) => distToSegment(p, { x: o.x1, y: o.y1 }, { x: o.x2, y: o.y2 }) <= tol + o.size * 2,
    bounds: o => boundsOf(ptsOfLine(o), o.size * 3),
    translate: (o, dx, dy) => ({ x1: o.x1 + dx, y1: o.y1 + dy, x2: o.x2 + dx, y2: o.y2 + dy }),
  },
  pen: {
    draw(ctx, o) {
      stroke(ctx, o);
      const p = o.pts;
      ctx.beginPath(); ctx.moveTo(p[0].x, p[0].y);
      if (p.length === 1) ctx.lineTo(p[0].x + 0.01, p[0].y);
      // quadratic through midpoints = smooth without storing extra points
      for (let i = 1; i < p.length - 1; i++) {
        ctx.quadraticCurveTo(p[i].x, p[i].y, (p[i].x + p[i + 1].x) / 2, (p[i].y + p[i + 1].y) / 2);
      }
      if (p.length > 1) ctx.lineTo(p[p.length - 1].x, p[p.length - 1].y);
      ctx.stroke();
    },
    hit(o, p, tol) {
      for (let i = 1; i < o.pts.length; i++) if (distToSegment(p, o.pts[i - 1], o.pts[i]) <= tol + o.size / 2) return true;
      return o.pts.length === 1 && Math.hypot(p.x - o.pts[0].x, p.y - o.pts[0].y) <= tol + o.size;
    },
    bounds: o => boundsOf(o.pts, o.size),
    translate: (o, dx, dy) => ({ pts: o.pts.map(q => ({ x: q.x + dx, y: q.y + dy })) }),
  },
  text: {
    draw(ctx, o) {
      const { lines } = textMetrics(o);
      ctx.fillStyle = o.color; ctx.font = `700 ${o.size}px ${FONT}`; ctx.textBaseline = 'top';
      lines.forEach((l, i) => ctx.fillText(l, o.x, o.y + i * o.size * 1.25));
    },
    hit: (o, p, tol) => inRect(SHAPES.text.bounds(o), p, tol),
    bounds(o) { const m = textMetrics(o); return { x: o.x, y: o.y, w: m.w, h: m.h }; },
    translate: moveRect,
  },
  marker: {
    draw(ctx, o) {
      const r = o.size * 3.2;
      ctx.fillStyle = o.color; ctx.beginPath(); ctx.arc(o.x, o.y, r, 0, Math.PI * 2); ctx.fill();
      ctx.lineWidth = Math.max(1, o.size / 4); ctx.strokeStyle = '#ffffffcc'; ctx.stroke();
      ctx.fillStyle = '#fff'; ctx.font = `800 ${r * 1.15}px ${FONT}`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(String(o.n), o.x, o.y + r * 0.06); ctx.textAlign = 'left';
    },
    hit: (o, p, tol) => Math.hypot(p.x - o.x, p.y - o.y) <= o.size * 3.2 + tol,
    bounds: o => ({ x: o.x - o.size * 3.2, y: o.y - o.size * 3.2, w: o.size * 6.4, h: o.size * 6.4 }),
    translate: moveRect,
  },
  blur: {
    // Pixelate the region from the *base* image; the small bitmap is cached per object.
    draw(ctx, o, env) {
      if (o.w < 2 || o.h < 2) return;
      const f = Math.max(6, Math.round(env.unit * 8));
      const cw = Math.max(1, Math.ceil(o.w / f)), ch = Math.max(1, Math.ceil(o.h / f));
      const key = `${o.x}|${o.y}|${o.w}|${o.h}`;
      if (!o._cache || o._cache.key !== key) {
        // reuse the small canvas while the box is moved (only the sampled region changes)
        const c = o._cache && o._cache.cw === cw && o._cache.ch === ch ? o._cache : { canvas: new OffscreenCanvas(cw, ch), cw, ch };
        const g = c.canvas.getContext('2d');
        g.clearRect(0, 0, cw, ch);
        g.drawImage(env.base, o.x, o.y, o.w, o.h, 0, 0, cw, ch);
        c.key = key; o._cache = c;
      }
      ctx.save(); ctx.imageSmoothingEnabled = false;
      ctx.drawImage(o._cache.canvas, 0, 0, o._cache.cw, o._cache.ch, o.x, o.y, o.w, o.h);
      ctx.restore();
    },
    hit: hitRect, bounds: rectBounds, translate: moveRect,
  },
};

/** Top-most object under `p` (image px), with a tolerance in image px. */
export function pick(doc, p, tol) {
  for (let i = doc.objects.length - 1; i >= 0; i--) {
    const o = doc.objects[i];
    if (SHAPES[o.type].hit(o, p, tol)) return o;
  }
  return null;
}
