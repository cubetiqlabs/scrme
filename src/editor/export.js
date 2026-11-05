// Flatten the document at 1:1 image pixels and hand raw RGBA to Rust (never base64/PNG).

import { SHAPES } from './shapes.js';

/** `rect` is the canvas (image + overflow, see doc.canvasRect); the overflow is white. */
export function flatten(doc, bitmap, unit, rect = { x: 0, y: 0, w: doc.w, h: doc.h }) {
  const c = new OffscreenCanvas(rect.w, rect.h);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, rect.w, rect.h);
  ctx.translate(-rect.x, -rect.y);
  ctx.drawImage(bitmap, 0, 0);
  const env = { base: bitmap, unit, dpr: window.devicePixelRatio || 1 };
  for (const o of doc.objects) SHAPES[o.type].draw(ctx, o, env);
  return ctx.getImageData(0, 0, rect.w, rect.h).data.buffer;
}
