// Document model + undo/redo (command pattern). No DOM, no canvas.

export function createDoc(w, h) {
  return { w, h, objects: [], seq: 1 };
}

export function nextId(doc) {
  return doc.seq++;
}

/** Next marker number: one more than the highest existing one. */
export function nextMarker(doc) {
  return doc.objects.reduce((n, o) => (o.type === 'marker' ? Math.max(n, o.n) : n), 0) + 1;
}

// Commands: { do(), undo() } — they only mutate the document.
export const addCmd = (doc, obj) => ({
  do: () => { doc.objects.push(obj); },
  undo: () => { doc.objects.splice(doc.objects.indexOf(obj), 1); },
});

export const removeCmd = (doc, obj) => {
  let index = -1;
  return {
    do: () => { index = doc.objects.indexOf(obj); doc.objects.splice(index, 1); },
    undo: () => { doc.objects.splice(index, 0, obj); },
  };
};

/** Patch fields of an object; `before` is captured from the object at creation. */
export const patchCmd = (obj, after) => {
  const before = {};
  for (const k of Object.keys(after)) before[k] = obj[k];
  return { do: () => Object.assign(obj, after), undo: () => Object.assign(obj, before) };
};

/** Set fields from a known `before` to `after` (used when the object was already changed live while dragging). */
export const setCmd = (obj, before, after) => ({
  do: () => Object.assign(obj, after),
  undo: () => Object.assign(obj, before),
});

/**
 * The drawable canvas: the image plus anything annotated outside it. `boundsFn(obj)` gives an
 * object's {x,y,w,h}; the result is rounded outwards to whole pixels and may start at a
 * negative origin. Pure, so it is unit-tested.
 */
export function canvasRect(doc, boundsFn) {
  let x0 = 0, y0 = 0, x1 = doc.w, y1 = doc.h;
  for (const o of doc.objects) {
    if (o.hidden) continue;
    const b = boundsFn(o);
    x0 = Math.min(x0, b.x); y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.w); y1 = Math.max(y1, b.y + b.h);
  }
  const x = Math.floor(x0), y = Math.floor(y0);
  return { x, y, w: Math.ceil(x1) - x, h: Math.ceil(y1) - y };
}

export function createHistory(onChange = () => {}) {
  const undoStack = [], redoStack = [];
  return {
    run(cmd) { cmd.do(); undoStack.push(cmd); redoStack.length = 0; onChange(); },
    undo() { const c = undoStack.pop(); if (!c) return false; c.undo(); redoStack.push(c); onChange(); return true; },
    redo() { const c = redoStack.pop(); if (!c) return false; c.do(); undoStack.push(c); onChange(); return true; },
    reset() { undoStack.length = 0; redoStack.length = 0; onChange(); },
    get canUndo() { return undoStack.length > 0; },
    get canRedo() { return redoStack.length > 0; },
  };
}
