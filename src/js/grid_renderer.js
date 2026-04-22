// ABOUTME: Experimental DOM-based renderer that displays the alacritty-parsed
// ABOUTME: grid snapshot. Proves the Unicode width bug fix end-to-end.

(function () {
  'use strict';

  const FLAG_BOLD = 1;
  const FLAG_ITALIC = 2;
  const FLAG_UNDERLINE = 4;
  const FLAG_STRIKEOUT = 8;
  const FLAG_INVERSE = 16;
  const FLAG_DIM = 32;

  // Map alacritty named colors to ANSI hex (matches typical dark theme).
  const NAMED_COLORS = {
    Black: '#000000', Red: '#cc6666', Green: '#b5bd68',
    Yellow: '#f0c674', Blue: '#81a2be', Magenta: '#b294bb',
    Cyan: '#8abeb7', White: '#c5c8c6',
    BrightBlack: '#666666', BrightRed: '#d54e53', BrightGreen: '#b9ca4a',
    BrightYellow: '#e7c547', BrightBlue: '#7aa6da', BrightMagenta: '#c397d8',
    BrightCyan: '#70c0b1', BrightWhite: '#eaeaea',
    Foreground: '#c5c8c6', Background: '#1d1f21',
    Cursor: '#c5c8c6',
  };

  function colorToCss(colorStr, fallback) {
    if (!colorStr) return fallback;
    if (colorStr.startsWith('#')) return colorStr;
    if (colorStr.startsWith('named:')) {
      const name = colorStr.slice(6);
      return NAMED_COLORS[name] || fallback;
    }
    if (colorStr.startsWith('idx:')) {
      // 256-color palette index — fall back for now.
      return fallback;
    }
    return fallback;
  }

  /**
   * Create a grid renderer attached to the given DOM container.
   *
   * @param {HTMLElement} container — element to render into
   * @returns object with `render(snapshot)` and `destroy()` methods
   */
  function createGridRenderer(container) {
    container.innerHTML = '';
    container.style.cssText = `
      font-family: 'JetBrains Mono', 'SF Mono', Menlo, monospace;
      font-size: 14px;
      line-height: 1.2;
      background: #1d1f21;
      color: #c5c8c6;
      overflow: auto;
      padding: 4px;
      white-space: pre;
      cursor: text;
      user-select: text;
    `;

    const gridEl = document.createElement('div');
    gridEl.className = 'alacritty-grid';
    container.appendChild(gridEl);

    return {
      render(snapshot) {
        if (!snapshot || !snapshot.grid) return;

        // Build all rows as a single innerHTML pass for performance.
        const rows = [];
        for (let r = 0; r < snapshot.grid.length; r++) {
          const row = snapshot.grid[r];
          const parts = [];
          // Coalesce runs of identical-style cells into single <span>s.
          let runStart = 0;
          while (runStart < row.cells.length) {
            const startCell = row.cells[runStart];
            let runEnd = runStart + 1;
            while (
              runEnd < row.cells.length &&
              row.cells[runEnd].fg === startCell.fg &&
              row.cells[runEnd].bg === startCell.bg &&
              row.cells[runEnd].flags === startCell.flags
            ) {
              runEnd++;
            }
            const text = row.cells.slice(runStart, runEnd)
              .map(c => c.c)
              .join('')
              .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

            const fg = colorToCss(startCell.fg, '#c5c8c6');
            const bg = colorToCss(startCell.bg, '#1d1f21');
            const flags = startCell.flags;
            let style = `color:${fg};background:${bg};`;
            if (flags & FLAG_BOLD) style += 'font-weight:bold;';
            if (flags & FLAG_ITALIC) style += 'font-style:italic;';
            if (flags & FLAG_UNDERLINE) style += 'text-decoration:underline;';
            if (flags & FLAG_STRIKEOUT) style += 'text-decoration:line-through;';
            if (flags & FLAG_DIM) style += 'opacity:0.6;';
            if (flags & FLAG_INVERSE) style += `color:${bg};background:${fg};`;

            // Render cursor cell with inverse styling
            const isCursor = (r === snapshot.cursor_row && runStart <= snapshot.cursor_col && snapshot.cursor_col < runEnd);
            if (isCursor) {
              // Render up to cursor, the cursor cell, then the rest
              const before = row.cells.slice(runStart, snapshot.cursor_col).map(c => c.c).join('')
                .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
              const cursorChar = (row.cells[snapshot.cursor_col].c || ' ')
                .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
              const after = row.cells.slice(snapshot.cursor_col + 1, runEnd).map(c => c.c).join('')
                .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
              if (before) parts.push(`<span style="${style}">${before}</span>`);
              parts.push(`<span style="${style}background:${fg};color:${bg};">${cursorChar}</span>`);
              if (after) parts.push(`<span style="${style}">${after}</span>`);
            } else {
              parts.push(`<span style="${style}">${text}</span>`);
            }
            runStart = runEnd;
          }
          rows.push(`<div class="alacritty-row">${parts.join('')}</div>`);
        }
        gridEl.innerHTML = rows.join('');
      },
      destroy() {
        container.innerHTML = '';
      },
    };
  }

  window.AlacrittyRenderer = { createGridRenderer };
})();
