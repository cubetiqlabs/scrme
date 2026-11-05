import test from 'node:test';
import assert from 'node:assert/strict';
import { normRect, inRect, distToSegment, boundsOf, arrowBarbs, addStrokePoint, toHex, segLen, clampRect } from './geom.js';
import { createDoc, nextId, nextMarker, addCmd, removeCmd, patchCmd, setCmd, createHistory } from './doc.js';

test('normRect handles any drag direction', () => {
  assert.deepEqual(normRect({ x: 50, y: 40 }, { x: 10, y: 10 }), { x: 10, y: 10, w: 40, h: 30 });
});

test('inRect with tolerance', () => {
  const r = { x: 10, y: 10, w: 20, h: 20 };
  assert.ok(inRect(r, { x: 31, y: 15 }, 2));
  assert.ok(!inRect(r, { x: 40, y: 15 }, 2));
});

test('distToSegment: perpendicular, beyond the end, degenerate', () => {
  const a = { x: 0, y: 0 }, b = { x: 10, y: 0 };
  assert.equal(distToSegment({ x: 5, y: 3 }, a, b), 3);
  assert.equal(distToSegment({ x: 13, y: 4 }, a, b), 5);
  assert.equal(distToSegment({ x: 3, y: 4 }, a, a), 5);
});

test('boundsOf and arrow barbs', () => {
  assert.deepEqual(boundsOf([{ x: 1, y: 2 }, { x: 5, y: 8 }], 1), { x: 0, y: 1, w: 6, h: 8 });
  const [p, q] = arrowBarbs({ x: 0, y: 0 }, { x: 10, y: 0 }, 5, Math.PI / 4);
  assert.ok(p.x < 10 && q.x < 10, 'barbs sit behind the tip');
  assert.ok(Math.abs(p.y + q.y) < 1e-9, 'barbs are symmetric');
});

test('stroke decimation drops points that are too close', () => {
  const pts = [];
  for (const x of [0, 1, 2, 5, 6, 12]) addStrokePoint(pts, { x, y: 0 }, 4);
  assert.deepEqual(pts.map(p => p.x), [0, 5, 12]);
});

test('toHex pads', () => assert.equal(toHex(255, 0, 10), '#ff000a'));

test('history: add, undo, redo, and redo is cleared by a new action', () => {
  const doc = createDoc(100, 100);
  const h = createHistory();
  const a = { id: nextId(doc), type: 'rect' }, b = { id: nextId(doc), type: 'rect' };
  h.run(addCmd(doc, a));
  h.run(addCmd(doc, b));
  assert.equal(doc.objects.length, 2);
  h.undo();
  assert.deepEqual(doc.objects, [a]);
  h.redo();
  assert.deepEqual(doc.objects, [a, b]);
  h.undo();
  h.run(addCmd(doc, { id: 99 }));
  assert.equal(h.canRedo, false);
  assert.equal(doc.objects.length, 2);
});

test('remove restores the original z-order on undo', () => {
  const doc = createDoc(10, 10);
  const h = createHistory();
  const [a, b, c] = [1, 2, 3].map(id => ({ id }));
  doc.objects.push(a, b, c);
  h.run(removeCmd(doc, b));
  assert.deepEqual(doc.objects, [a, c]);
  h.undo();
  assert.deepEqual(doc.objects, [a, b, c]);
});

test('patch undo restores previous values', () => {
  const o = { x: 1, y: 2, color: 'red' };
  const h = createHistory();
  h.run(patchCmd(o, { x: 9, color: 'blue' }));
  assert.deepEqual(o, { x: 9, y: 2, color: 'blue' });
  h.undo();
  assert.deepEqual(o, { x: 1, y: 2, color: 'red' });
});

test('marker numbers continue from the highest', () => {
  const doc = createDoc(10, 10);
  assert.equal(nextMarker(doc), 1);
  doc.objects.push({ type: 'marker', n: 1 }, { type: 'rect' }, { type: 'marker', n: 4 });
  assert.equal(nextMarker(doc), 5);
});

test('setCmd undoes a live-dragged change back to the recorded start', () => {
  const o = { x: 0, y: 0 };
  const h = createHistory();
  Object.assign(o, { x: 30, y: 40 }); // moved live while dragging
  h.run(setCmd(o, { x: 0, y: 0 }, { x: 30, y: 40 }));
  assert.deepEqual(o, { x: 30, y: 40 });
  h.undo();
  assert.deepEqual(o, { x: 0, y: 0 });
  h.redo();
  assert.deepEqual(o, { x: 30, y: 40 });
});

test('segLen and clampRect', () => {
  assert.equal(segLen({ x1: 0, y1: 0, x2: 3, y2: 4 }), 5);
  assert.deepEqual(clampRect({ x: -10, y: 5.4, w: 50, h: 20 }, 100, 100), { x: 0, y: 5, w: 40, h: 20 });
  assert.deepEqual(clampRect({ x: 90, y: 90, w: 50, h: 50 }, 100, 100), { x: 90, y: 90, w: 10, h: 10 });
  assert.equal(clampRect({ x: 200, y: 0, w: 5, h: 5 }, 100, 100), null);
  assert.equal(clampRect({ x: 10, y: 10, w: 0.2, h: 40 }, 100, 100), null);
});
