// SPDX-License-Identifier: AGPL-3.0-or-later
// The window's own drawn icons and marks, as svg markup (styled by app.css).

export const ICONS = {
  logo: '<svg class="logo" viewBox="0 0 512 512" aria-hidden="true" data-tauri-drag-region><defs><mask id="baguetteBite" maskUnits="userSpaceOnUse" x="-100" y="-100" width="712" height="712"><rect x="-100" y="-100" width="712" height="712" fill="#fff"/><path d="M150 214l30 84M256 214l30 84" stroke="#000" stroke-width="26" stroke-linecap="round"/><g fill="#000" transform="translate(405 224) rotate(45) scale(5.6)"><circle cx="-6" cy="-1.7" r="5.5"/><circle cx="0" cy="0" r="5.5"/><circle cx="6" cy="-1.7" r="5.5"/><rect x="-11.5" y="-20" width="23" height="18"/></g></mask></defs><g transform="rotate(-40 256 256)"><rect x="41" y="181" width="430" height="150" rx="75" fill="var(--accent)" mask="url(#baguetteBite)"/></g></svg>',
  bite: '<svg class="bite" viewBox="0 0 12 12" aria-hidden="true"><circle cx="6" cy="6" r="5.5" fill="var(--accent)"/><circle cx="11" cy="2" r="2.6" fill="var(--surface)"/></svg>',
  biteBig:
    '<svg class="bite big" viewBox="0 0 12 12" aria-hidden="true"><circle cx="6" cy="6" r="5.5" fill="var(--accent)"/><circle cx="11" cy="2" r="2.6" fill="var(--surface)"/></svg>',
  settings:
    '<svg class="gear" viewBox="0 0 16 16" aria-hidden="true"><mask id="gearHole"><rect width="16" height="16" fill="#fff"/><circle cx="8" cy="8" r="2.1" fill="#000"/></mask><g mask="url(#gearHole)"><circle cx="8" cy="8" r="4.9"/><circle cx="13.3" cy="8" r="1.75"/><circle cx="11.75" cy="11.75" r="1.75"/><circle cx="8" cy="13.3" r="1.75"/><circle cx="4.25" cy="11.75" r="1.75"/><circle cx="2.7" cy="8" r="1.75"/><circle cx="4.25" cy="4.25" r="1.75"/><circle cx="8" cy="2.7" r="1.75"/><circle cx="11.75" cy="4.25" r="1.75"/></g></svg>',
  minimize: '<svg viewBox="0 0 12 12"><path d="M2 6h8"/></svg>',
  maximize: '<svg viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" rx="1.5"/></svg>',
  close: '<svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6"/></svg>',
  upload: '<svg viewBox="0 0 16 16"><path d="M8 13V3M4 7l4-4 4 4"/></svg>',
  download: '<svg viewBox="0 0 16 16"><path d="M8 3v10M4 9l4 4 4-4"/></svg>',
  refresh: '<svg viewBox="0 0 16 16"><path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5V5h-2.5"/></svg>',
  up: '<svg viewBox="0 0 16 16"><path d="M8 13V4M4 8l4-4 4 4"/></svg>',
  lock: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="7" width="9" height="6.5" rx="1.8"/><path d="M5.5 7V5.2a2.5 2.5 0 0 1 5 0V7"/></svg>',
  folder:
    '<svg class="ico dir" viewBox="0 0 16 16"><path d="M1.5 4c0-.8.7-1.5 1.5-1.5h3.2l1.5 1.5H13c.8 0 1.5.7 1.5 1.5v6.5c0 .8-.7 1.5-1.5 1.5H3c-.8 0-1.5-.7-1.5-1.5z"/></svg>',
  file: '<svg class="ico file" viewBox="0 0 16 16"><path d="M4 1.5h5L12.5 5v9c0 .3-.2.5-.5.5H4c-.3 0-.5-.2-.5-.5V2c0-.3.2-.5.5-.5zM9 1.5V5h3.5"/></svg>',
  drive:
    '<svg class="ico drive" viewBox="0 0 16 16"><path d="M2 9.5h12v3c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1zM2 9.5 4 3.5h8l2 6M11.5 11.5h.01"/></svg>',
  edit: '<svg viewBox="0 0 16 16"><path d="M10.5 2.5l3 3-8 8H2.5v-3z"/></svg>',
  tabClose: '<svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6"/></svg>',
}

export type IconName = keyof typeof ICONS
