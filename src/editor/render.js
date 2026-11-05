// One viewport-sized canvas. Every frame (at most one per rAF) draws the image and the
// annotations through a zoom/pan transform, so memory and cost follow the *window*, not the
// image: a retina full-screen capture no longer needs two 30 MB canvases plus a scaled
// compositor layer. The image itself is a GPU ImageBitmap drawn with drawImage.

import { SHAPES } from './shapes.js';

export function createRenderer(canvas, getState) {
  const ctx = canvas.getContext('2d');
  let raf = 0;

  function draw() {
    raf = 0;
    const { doc, bitmap, live, selected, view, unit, crop, dpr } = getState();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (!bitmap || !doc) return;
    const z = view.zoom, s = view.scale;
    ctx.setTransform(z, 0, 0, z, view.x * dpr, view.y * dpr); // image px → device px

    // image (pixelated when zoomed in far enough to see pixels)
    ctx.imageSmoothingEnabled = z < 3;
    ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(bitmap, 0, 0);
    ctx.lineWidth = 1 / z; ctx.strokeStyle = 'rgba(0,0,0,0.35)';
    ctx.strokeRect(-0.5 / z, -0.5 / z, doc.w + 1 / z, doc.h + 1 / z);

    // annotations are clipped to the image
    ctx.save();
    ctx.beginPath(); ctx.rect(0, 0, doc.w, doc.h); ctx.clip();
    const env = { base: bitmap, unit, dpr };
    for (const o of doc.objects) if (!o.hidden) SHAPES[o.type].draw(ctx, o, env);
    if (live) SHAPES[live.type].draw(ctx, live, env);
    ctx.restore();

    if (selected) outline(SHAPES[selected.type].bounds(selected), s);
    if (crop) cropOverlay(doc, crop, s);
  }

  // Dim everything outside the crop box; the box itself keeps a crisp outline and thirds.
  function cropOverlay(doc, r, s) {
    ctx.save();
    ctx.fillStyle = 'rgba(8, 6, 16, 0.55)';
    ctx.beginPath(); ctx.rect(0, 0, doc.w, doc.h); ctx.rect(r.x, r.y, r.w, r.h); ctx.fill('evenodd');
    ctx.lineWidth = 1.5 / s; ctx.strokeStyle = '#ffffff'; ctx.strokeRect(r.x, r.y, r.w, r.h);
    ctx.lineWidth = 1 / s; ctx.strokeStyle = '#ffffff66';
    ctx.beginPath();
    for (const f of [1 / 3, 2 / 3]) {
      ctx.moveTo(r.x + r.w * f, r.y); ctx.lineTo(r.x + r.w * f, r.y + r.h);
      ctx.moveTo(r.x, r.y + r.h * f); ctx.lineTo(r.x + r.w, r.y + r.h * f);
    }
    ctx.stroke(); ctx.restore();
  }

  // Selection box with constant on-screen thickness, whatever the zoom.
  function outline(b, s) {
    const pad = 4 / s;
    ctx.save();
    ctx.lineWidth = 1.5 / s;
    ctx.setLineDash([5 / s, 4 / s]);
    ctx.strokeStyle = '#ffffff'; ctx.strokeRect(b.x - pad, b.y - pad, b.w + pad * 2, b.h + pad * 2);
    ctx.lineDashOffset = 4.5 / s;
    ctx.strokeStyle = '#7c5cff'; ctx.strokeRect(b.x - pad, b.y - pad, b.w + pad * 2, b.h + pad * 2);
    ctx.restore();
  }

  return {
    /** Match the backing store to the viewport (device pixels). */
    resize() {
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, Math.round(canvas.clientWidth * dpr)), h = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      this.invalidate();
    },
    invalidate() { if (!raf) raf = requestAnimationFrame(draw); },
  };
}
