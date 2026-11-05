// Two-canvas pipeline: the base image is painted once; the scene canvas (objects, live
// shape, selection box) is redrawn at most once per animation frame.

import { SHAPES } from './shapes.js';

export function createRenderer(baseCanvas, sceneCanvas, getState) {
  const sctx = sceneCanvas.getContext('2d');
  let raf = 0;

  function draw() {
    raf = 0;
    const { doc, bitmap, live, selected, screenScale, unit, crop, dpr } = getState();
    if (!bitmap) return;
    sctx.clearRect(0, 0, doc.w, doc.h);
    const env = { base: bitmap, unit, dpr };
    for (const o of doc.objects) if (!o.hidden) SHAPES[o.type].draw(sctx, o, env);
    if (live) SHAPES[live.type].draw(sctx, live, env);
    if (selected) outline(SHAPES[selected.type].bounds(selected), screenScale);
    if (crop) cropOverlay(doc, crop, screenScale);
  }

  // Dim everything outside the crop box; the box itself keeps a crisp outline and thirds.
  function cropOverlay(doc, r, s) {
    sctx.save();
    sctx.fillStyle = 'rgba(8, 6, 16, 0.55)';
    sctx.beginPath(); sctx.rect(0, 0, doc.w, doc.h); sctx.rect(r.x, r.y, r.w, r.h); sctx.fill('evenodd');
    sctx.lineWidth = 1.5 / s; sctx.strokeStyle = '#ffffff'; sctx.strokeRect(r.x, r.y, r.w, r.h);
    sctx.lineWidth = 1 / s; sctx.strokeStyle = '#ffffff66';
    sctx.beginPath();
    for (const f of [1 / 3, 2 / 3]) {
      sctx.moveTo(r.x + r.w * f, r.y); sctx.lineTo(r.x + r.w * f, r.y + r.h);
      sctx.moveTo(r.x, r.y + r.h * f); sctx.lineTo(r.x + r.w, r.y + r.h * f);
    }
    sctx.stroke(); sctx.restore();
  }

  // Selection box with constant on-screen thickness, whatever the zoom.
  function outline(b, s) {
    const pad = 4 / s;
    sctx.save();
    sctx.lineWidth = 1.5 / s;
    sctx.setLineDash([5 / s, 4 / s]);
    sctx.strokeStyle = '#ffffff'; sctx.strokeRect(b.x - pad, b.y - pad, b.w + pad * 2, b.h + pad * 2);
    sctx.lineDashOffset = 4.5 / s;
    sctx.strokeStyle = '#7c5cff'; sctx.strokeRect(b.x - pad, b.y - pad, b.w + pad * 2, b.h + pad * 2);
    sctx.restore();
  }

  return {
    setImage(bitmap) {
      baseCanvas.width = sceneCanvas.width = bitmap.width;
      baseCanvas.height = sceneCanvas.height = bitmap.height;
      baseCanvas.getContext('2d', { willReadFrequently: true }).drawImage(bitmap, 0, 0);
    },
    invalidate() { if (!raf) raf = requestAnimationFrame(draw); },
  };
}
