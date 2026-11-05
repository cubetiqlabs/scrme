// Rapid-click combo detector (pure: time is passed in, so it is trivially testable).
// Clicks closer together than `resetMs` build a streak; hitting a tier's count
// fires it once. The top tier starts a cooldown so the big reaction can't be spammed.

export const DEFAULT_TIERS = [
  [10, 'surprise'],
  [6, 'dizzy'],
  [3, 'boop'],
];

export function createCombo({ resetMs = 1400, cooldownMs = 2600, tiers = DEFAULT_TIERS } = {}) {
  let streak = 0;
  let last = -Infinity;
  let coolUntil = -Infinity;
  const top = tiers[0]?.[1];

  /** Register a click at time `now` (ms). Returns 'poke' or a tier name. */
  return function click(now) {
    if (now - last > resetMs) streak = 0;
    last = now;
    streak += 1;
    if (now < coolUntil) return 'poke';
    for (const [count, name] of tiers) {
      if (streak !== count) continue;
      if (name === top) {
        coolUntil = now + cooldownMs;
        streak = 0;
      }
      return name;
    }
    return 'poke';
  };
}
