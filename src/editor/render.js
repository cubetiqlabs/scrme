// Two-canvas pipeline: the base image is painted once; the scene canvas (objects, live
// shape, selection box) is redrawn at most once per animation frame.

import { SHAPES } from './shapes.js';

export function createRenderer(baseCanvas, sceneCanvas, getState) {
  const sctx = sceneCanvas.getContext('2d');
  let raf = 0;

  function draw() {
    raf = 0;
    const { doc, bitmap, live, selected, screenScale, unit } = getState();
    if (!bitmap) return;
    sctx.clearRect(0, 0, doc.w, doc.h);
    const env = { base: bitmap, unit };
    for (const o of doc.objects) if (!o.hidden) SHAPES[o.type].draw(sctx, o, env);
    if (live) SHAPES[live.type].draw(sctx, live, env);
    if (selected) outline(SHAPES[selected.type].bounds(selected), screenScale);
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
      baseCanvas.getContext('2d').drawImage(bitmap, 0, 0);
    },
    invalidate() { if (!raf) raf = requestAnimationFrame(draw); },
  };
}
