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

export function createHistory(onChange = () => {}) {
  const undoStack = [], redoStack = [];
  return {
    run(cmd) { cmd.do(); undoStack.push(cmd); redoStack.length = 0; onChange(); },
    undo() { const c = undoStack.pop(); if (!c) return false; c.undo(); redoStack.push(c); onChange(); return true; },
    redo() { const c = redoStack.pop(); if (!c) return false; c.do(); undoStack.push(c); onChange(); return true; },
    get canUndo() { return undoStack.length > 0; },
    get canRedo() { return redoStack.length > 0; },
  };
}
