// Own glyphs (24px grid, stroke-based) in the same style as the tray/Preferences icons.
const p = d => `<path d="${d}"/>`;
export const ICONS = {
  select: p('M5 3l13 7-6 1.8L9.6 18z'),
  pen: p('M4 20l1-4L16.5 4.5a2.1 2.1 0 013 3L8 19z') + p('M14 7l3 3'),
  arrow: p('M5 19L19 5') + p('M9 5h10v10'),
  line: p('M5 19L19 5'),
  rect: '<rect x="4" y="6" width="16" height="12" rx="3"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="8" ry="6"/>',
  highlighter: p('M4 20h8') + p('M13.5 4.5l6 6-8 8-4-.5-.5-4z'),
  text: p('M5 6V4h14v2') + p('M12 4v16') + p('M9 20h6'),
  marker: '<circle cx="12" cy="12" r="8"/>' + p('M10.5 10l1.8-1.4V15'),
  blur: '<rect x="4" y="4" width="16" height="16" rx="3"/>' + p('M4 12h16M12 4v16'),
  ruler: p('M3 16L16 3l5 5L8 21z') + p('M7 12l2 2M10 9l2 2M13 6l2 2'),
  picker: p('M14 4l6 6') + p('M17 7l-9 9-1 4 4-1 9-9a2.1 2.1 0 00-3-3z') + p('M9 15l2 2'),
  crop: p('M7 3v14h14') + p('M3 7h14v14'),
  undo: p('M9 8L4 12l5 4') + p('M4 12h10a5 5 0 010 10h-3'),
  redo: p('M15 8l5 4-5 4') + p('M20 12H10a5 5 0 000 10h3'),
  copy: '<rect x="8" y="8" width="12" height="12" rx="2.5"/>' + p('M16 8V6.5A2.5 2.5 0 0013.5 4h-7A2.5 2.5 0 004 6.5v7A2.5 2.5 0 006.5 16H8'),
  save: p('M5 4h11l3 3v13H5z') + p('M8 4v5h7V4M8 20v-6h8v6'),
  close: p('M6 6l12 12M18 6L6 18'),
};
export const icon = (name, cls = '') =>
  `<svg class="ic ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name]}</svg>`;
