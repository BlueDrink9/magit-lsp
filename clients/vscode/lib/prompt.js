"use strict";

/**
 * Pure mapping between `view/prompt` parameters and QuickPick items/answers.
 *
 * The extension owns the actual VSCode widgets; this module only shapes data
 * so the mapping can be tested without an editor.
 */

/** The id of the synthetic "type a value" row of an `allowCustom` choice. */
const CUSTOM_ID = "__custom__";

/**
 * The rows a prompt offers, in display order.
 *
 * @param {import('../types/protocol').ViewPromptParams.Root} params `view/prompt` params
 * @returns {{id: string, label: string, description?: string, detail?: string}[]}
 */
function promptItems(params) {
  if (!params) {
    return [];
  }
  if (params.kind === "confirm") {
    const yes = { id: "yes", label: params.yesLabel || "Yes" };
    const no = { id: "no", label: params.noLabel || "No" };
    // A destructive question focuses "No" first.
    return params.destructive || params.default === false ? [no, yes] : [yes, no];
  }

  /** @type {{id: string, label: string, description?: string, detail?: string}[]} */
  const items = (params.items || []).map((item) => ({
    id: item.id,
    label: item.label,
    description: item.description,
    detail: item.detail,
  }));
  if (params.kind === "choice" && params.allowCustom) {
    items.push({ id: CUSTOM_ID, label: "Enter custom value…" });
  }
  return items;
}

/**
 * Shape the answer for one picked row (or value).
 *
 * @param {import('../types/protocol').ViewPromptParams.Root} params `view/prompt` params
 * @param {any} picked one row, a string, an array of rows, or undefined (VSCode widget result boundary)
 * @returns {{cancelled: boolean, value?: import('../types/protocol').ViewPromptRespondParams.PromptValue, custom?: boolean}}
 */
function answerFor(params, picked) {
  if (picked === undefined || picked === null) {
    return { cancelled: true };
  }
  switch (params.kind) {
    case "confirm":
      return { cancelled: false, value: picked.id === "yes" };
    case "text":
      // Neovim's `inputsecret` cannot distinguish Esc from an empty answer,
      // so an empty secret answer is a cancel in both clients.
      if (params.secret && picked === "") {
        return { cancelled: true };
      }
      return { cancelled: false, value: String(picked) };
    case "choice":
      if (picked.id === CUSTOM_ID) {
        // The caller must run a text prompt and use `customChoice`.
        return { cancelled: false, custom: true };
      }
      return { cancelled: false, value: { id: picked.id } };
    case "multiChoice": {
      const rows = Array.isArray(picked) ? picked : [picked];
      return { cancelled: false, value: { ids: rows.map((row) => row.id) } };
    }
    default:
      return { cancelled: true };
  }
}

/**
 * The answer value for a custom choice after the text prompt.
 *
 * @param {string|undefined} text the typed value, or undefined on cancel
 * @returns {{cancelled: boolean, value?: import('../types/protocol').ViewPromptRespondParams.PromptChoiceAnswer}}
 */
function customChoice(text) {
  if (text === undefined || text === null) {
    return { cancelled: true };
  }
  return { cancelled: false, value: { custom: text } };
}

module.exports = { promptItems, answerFor, customChoice };
