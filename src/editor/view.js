// Zoom/pan state for the viewport canvas (the renderer applies it). "100%" = 1 image pixel per
// device pixel (so a retina capture shows at its point size, like the status chip says).

export function createView(viewport, onChange = () => {}) {
  const dpr = () => window.devicePixelRatio || 1;
  let z = 1, x = 0, y = 0, iw = 1, ih = 1, cx = 0, cy = 0; // cx,cy,iw,ih = the canvas rect (image + overflow)
  let auto = true; // keep the image fitted until the user zooms or pans
  const scale = () => z / dpr();
  const apply = () => onChange(z);

  const view = {
    get zoom() { return z; },
    get x() { return x; },
    get y() { return y; },
    get scale() { return scale(); },
    /** Load/crop: a new image, always refit. */
    setImage(w, h) { view.setCanvas({ x: 0, y: 0, w, h }, true); },
    /** The canvas grew/shrank (annotations moved outside the image): refit unless the user took over. */
    setCanvas(rect, force = false) {
      cx = rect.x; cy = rect.y; iw = rect.w; ih = rect.h;
      if (force || auto) view.fit();
    },
    fit(pad = 56) {
      auto = true;
      const r = viewport.getBoundingClientRect();
      const s = Math.min(1 / dpr(), (r.width - pad * 2) / iw, (r.height - pad * 2) / ih);
      z = Math.max(0.1, s * dpr());
      x = (r.width - iw * scale()) / 2 - cx * scale();
      y = (r.height - ih * scale()) / 2 - cy * scale();
      apply();
    },
    actual() { view.zoomTo(1); },
    zoomTo(nz, cx, cy) {
      auto = false;
      const r = viewport.getBoundingClientRect();
      cx = cx ?? r.width / 2; cy = cy ?? r.height / 2;
      const before = scale();
      const px = (cx - x) / before, py = (cy - y) / before; // image point under (cx, cy)
      z = Math.min(16, Math.max(0.1, nz));
      x = cx - px * scale(); y = cy - py * scale();
      apply();
    },
    zoomBy(f, clientX, clientY) {
      const r = viewport.getBoundingClientRect();
      view.zoomTo(z * f, clientX === undefined ? undefined : clientX - r.left, clientY === undefined ? undefined : clientY - r.top);
    },
    panBy(dx, dy) { auto = false; x += dx; y += dy; apply(); },
    /** Image pixels → viewport-relative CSS pixels. */
    toViewport(p) { return { x: x + p.x * scale(), y: y + p.y * scale() }; },
    /** Client coordinates → image pixels. */
    toImage(clientX, clientY) {
      const r = viewport.getBoundingClientRect();
      return { x: (clientX - r.left - x) / scale(), y: (clientY - r.top - y) / scale() };
    },
  };

  viewport.addEventListener('wheel', e => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) view.zoomBy(Math.exp(-e.deltaY * 0.01), e.clientX, e.clientY); // pinch / ⌘-scroll
    else view.panBy(-e.deltaX, -e.deltaY);
  }, { passive: false });
  // The window is resized after the page loads (and on every user resize): stay fitted.
  new ResizeObserver(() => { if (auto) view.fit(); }).observe(viewport);
  return view;
}
