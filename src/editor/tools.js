// Tool registry. A drawing tool has `shape` + `start`/`drag`; add a tool = add an entry.
// `start(p, ctx)` returns a new object (or null); `drag(o, p, ctx)` updates it while the
// pointer moves; `done(o)` returns false to discard (too small).

import { normRect, addStrokePoint } from './geom.js';
import { nextMarker } from './doc.js';

const base = (type, ctx, extra) => ({ id: ctx.nextId(), type, color: ctx.color, size: ctx.size, ...extra });

const boxTool = type => ({
  start: (p, ctx) => ({ ...base(type, ctx, { x: p.x, y: p.y, w: 0, h: 0 }), _a: p }),
  drag(o, p) { Object.assign(o, normRect(o._a, p)); },
  done: o => o.w > 3 && o.h > 3,
});

const lineTool = type => ({
  start: (p, ctx) => base(type, ctx, { x1: p.x, y1: p.y, x2: p.x, y2: p.y }),
  drag(o, p, ctx) {
    let { x, y } = p;
    if (ctx.shift) { // snap to 45° steps
      const dx = x - o.x1, dy = y - o.y1, step = Math.PI / 4;
      const ang = Math.round(Math.atan2(dy, dx) / step) * step, len = Math.hypot(dx, dy);
      x = o.x1 + len * Math.cos(ang); y = o.y1 + len * Math.sin(ang);
    }
    o.x2 = x; o.y2 = y;
  },
  done: o => Math.hypot(o.x2 - o.x1, o.y2 - o.y1) > 4,
});

export const TOOLS = [
  { id: 'select', key: 'v', label: 'Select / move' },
  { id: 'pen', key: 'p', label: 'Pen', start: (p, ctx) => base('pen', ctx, { pts: [p] }),
    drag(o, p, ctx) { addStrokePoint(o.pts, p, Math.max(1.5, ctx.unit * 1.2)); }, done: () => true },
  { id: 'arrow', key: 'a', label: 'Arrow', ...lineTool('arrow') },
  { id: 'line', key: 'l', label: 'Line', ...lineTool('line') },
  { id: 'rect', key: 'r', label: 'Rectangle', ...boxTool('rect') },
  { id: 'ellipse', key: 'o', label: 'Ellipse', ...boxTool('ellipse') },
  { id: 'highlighter', key: 'h', label: 'Highlighter', ...boxTool('highlighter') },
  { id: 'text', key: 't', label: 'Text', text: true },
  { id: 'marker', key: 'n', label: 'Numbered marker',
    start: (p, ctx) => base('marker', ctx, { x: p.x, y: p.y, n: nextMarker(ctx.doc) }), drag() {}, done: () => true },
  { id: 'blur', key: 'b', label: 'Blur / pixelate', ...boxTool('blur') },
  { id: 'ruler', key: 'm', label: 'Ruler (measure)', ...lineTool('ruler') },
  { id: 'picker', key: 'i', label: 'Color picker', picker: true },
  { id: 'crop', key: 'c', label: 'Crop (Enter to apply)', crop: true },
];

export const byId = Object.fromEntries(TOOLS.map(t => [t.id, t]));
export const byKey = Object.fromEntries(TOOLS.map(t => [t.key, t.id]));
