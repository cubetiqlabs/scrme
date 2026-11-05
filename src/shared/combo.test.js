import test from 'node:test';
import assert from 'node:assert/strict';
import { createCombo } from './combo.js';

const clicks = (combo, times) => times.map(t => combo(t));

test('tiers fire once at their count', () => {
  const c = createCombo();
  const out = clicks(c, [0, 100, 200, 300, 400, 500, 600, 700, 800, 900]);
  assert.deepEqual(out, ['poke', 'poke', 'boop', 'poke', 'poke', 'dizzy', 'poke', 'poke', 'poke', 'surprise']);
});

test('a pause resets the streak', () => {
  const c = createCombo();
  assert.deepEqual(clicks(c, [0, 100]), ['poke', 'poke']);
  // 2s gap > resetMs: starts over, so this is click #1, not #3
  assert.equal(c(2200), 'poke');
  assert.equal(c(2300), 'poke');
  assert.equal(c(2400), 'boop');
});

test('surprise starts a cooldown and the streak restarts after it', () => {
  const c = createCombo({ cooldownMs: 2000 });
  clicks(c, [0, 50, 100, 150, 200, 250, 300, 350, 400, 450]); // ends in surprise at 450
  // still cooling down: spam only pokes, even when it would hit tiers
  assert.deepEqual(clicks(c, [500, 550, 600, 650, 700, 750]), Array(6).fill('poke'));
  // after the cooldown a fresh streak can trigger again
  const later = [3000, 3050, 3100].map(t => c(t));
  assert.deepEqual(later, ['poke', 'poke', 'boop']);
});
