// Pure geometry helpers (no DOM): easy to test, shared by shapes and tools.

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** Rect spanned by two points, any drag direction. */
export function normRect(a, b) {
  const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y);
  return { x, y, w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

export function inRect(r, p, tol = 0) {
  return p.x >= r.x - tol && p.x <= r.x + r.w + tol && p.y >= r.y - tol && p.y <= r.y + r.h + tol;
}

/** Distance from point p to segment a–b. */
export function distToSegment(p, a, b) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / len2, 0, 1);
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** Bounding box of points, grown by `pad`. */
export function boundsOf(points, pad = 0) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of points) {
    x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y);
  }
  return { x: x0 - pad, y: y0 - pad, w: x1 - x0 + 2 * pad, h: y1 - y0 + 2 * pad };
}

/** The two barb end points of an arrow head at `b` (pointing away from `a`). */
export function arrowBarbs(a, b, size, spread = 0.45) {
  const ang = Math.atan2(b.y - a.y, b.x - a.x);
  const barb = s => ({ x: b.x - size * Math.cos(ang + s), y: b.y - size * Math.sin(ang + s) });
  return [barb(-spread), barb(spread)];
}

/** Keep every Nth-or-far-enough point of a freehand stroke (cheap decimation). */
export function addStrokePoint(pts, p, minDist) {
  const last = pts[pts.length - 1];
  if (!last || Math.hypot(p.x - last.x, p.y - last.y) >= minDist) pts.push(p);
  return pts;
}

/** Segment length. */
export const segLen = o => Math.hypot(o.x2 - o.x1, o.y2 - o.y1);

/** Clamp a rect to 0..w × 0..h; null when nothing is left. Coordinates are rounded to whole pixels. */
export function clampRect(r, w, h) {
  const x0 = clamp(Math.round(r.x), 0, w), y0 = clamp(Math.round(r.y), 0, h);
  const x1 = clamp(Math.round(r.x + r.w), 0, w), y1 = clamp(Math.round(r.y + r.h), 0, h);
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
}

export const toHex = (r, g, b) => '#' + [r, g, b].map(v => v.toString(16).padStart(2, '0')).join('');
