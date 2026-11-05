// Flatten the document at 1:1 image pixels and hand raw RGBA to Rust (never base64/PNG).

import { SHAPES } from './shapes.js';

export function flatten(doc, bitmap, unit) {
  const c = new OffscreenCanvas(doc.w, doc.h);
  const ctx = c.getContext('2d');
  ctx.drawImage(bitmap, 0, 0);
  const env = { base: bitmap, unit };
  for (const o of doc.objects) SHAPES[o.type].draw(ctx, o, env);
  return ctx.getImageData(0, 0, doc.w, doc.h).data.buffer;
}
