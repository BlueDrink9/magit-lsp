'use strict';

/**
 * Apply one `LineEdit` to a line array. `endLine` is exclusive.
 *
 * @param {string[]} lines
 * @param {{startLine: number, endLine: number, lines: string[]}} edit
 */
function applyEdit(lines, edit) {
  const start = Math.max(0, edit.startLine);
  const end = Math.max(start, edit.endLine);
  lines.splice(start, end - start, ...(edit.lines || []));
}

/**
 * Apply a `view/render` payload to the lines the client currently holds.
 *
 * A full `text` always wins. Otherwise line `edits` are applied only when the
 * render's `baseVersion` is the version the client acknowledged; a stale base
 * means an update was lost, so the result is a `resync` marker and the caller
 * must ask the server with `view/resync` instead of patching stale text.
 *
 * @param {string[]|undefined} lines the current document lines
 * @param {number|undefined} version the last acknowledged version
 * @param {object|undefined} render a `ViewRender`
 * @returns {{lines: string[], version: number|undefined, mode: string|undefined, status: 'full'|'edits'|'resync'|'none'}}
 */
function applyRender(lines, version, render) {
  const current = Array.isArray(lines) ? lines.slice() : [];

  if (!render || typeof render !== 'object') {
    return { lines: current, version, mode: undefined, status: 'none' };
  }

  if (Array.isArray(render.text)) {
    return {
      lines: render.text.slice(),
      version: render.version,
      mode: render.mode,
      status: 'full',
    };
  }

  if (Array.isArray(render.edits)) {
    if (render.baseVersion !== version) {
      return { lines: current, version, mode: render.mode, status: 'resync' };
    }
    // Bottom-up keeps earlier offsets valid while splicing.
    const edits = render.edits
      .slice()
      .sort((left, right) => right.startLine - left.startLine);
    for (const edit of edits) {
      applyEdit(current, edit);
    }
    return {
      lines: current,
      version: render.version,
      mode: render.mode,
      status: 'edits',
    };
  }

  return { lines: current, version, mode: render.mode, status: 'none' };
}

module.exports = { applyRender, applyEdit };
