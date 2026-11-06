'use strict';

/**
 * Pure mapping between `view/pick` parameters and QuickPick items/answers.
 *
 * The extension owns the actual VSCode widget; this module only shapes data
 * so the mapping can be tested without an editor.
 */

/**
 * The rows a pick offers, in display order, with preselection applied.
 *
 * @param {object} params `view/pick` params
 * @returns {{id: string, label: string, description?: string, detail?: string, picked: boolean}[]}
 */
function pickItems(params) {
  if (!params) {
    return [];
  }
  const preselection = params.preselection || [];
  return (params.items || []).map((item) => ({
    id: item.id,
    label: item.label,
    description: item.description,
    detail: item.detail,
    picked: preselection.includes(item.id),
  }));
}

/**
 * Shape the answer for one picked row, an array of rows, or a cancel.
 *
 * @param {object} params `view/pick` params
 * @param {unknown} picked one row, an array of rows, or undefined/null on cancel
 * @returns {{cancelled: boolean, selected: string[]}}
 */
function answerFor(params, picked) {
  if (picked === undefined || picked === null) {
    return { cancelled: true, selected: [] };
  }
  if (Array.isArray(picked)) {
    return { cancelled: false, selected: picked.map((row) => row.id) };
  }
  return { cancelled: false, selected: [picked.id] };
}

module.exports = { pickItems, answerFor };
