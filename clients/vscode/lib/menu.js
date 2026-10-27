'use strict';

/**
 * The one-line description for a QuickPick row.
 *
 * @param {object} item a menu `Item`
 * @param {{kind: string, roadmapId?: string, reason?: string}} status
 * @returns {string}
 */
function describeItem(item, status) {
  const parts = [];
  if (item.flag) {
    parts.push(item.flag);
  }
  if (item.kind === 'switch') {
    parts.push(item.on ? 'on' : 'off');
  } else if (item.kind === 'option' && item.value !== undefined && item.value !== null) {
    parts.push(item.value);
  }
  if (status.status === 'planned') {
    parts.push(status.roadmapId ? `planned ${status.roadmapId}` : 'planned');
  }
  return parts.join(' · ');
}

/**
 * Map one menu item to a QuickPick row.
 *
 * @param {object} item
 * @returns {object}
 */
function quickPickItem(item) {
  const status = item.status;
  return {
    kind: 'item',
    label: `${item.key}  ${item.label}`,
    description: describeItem(item, status),
    key: item.key,
    itemKind: item.kind,
    status: status.status,
    roadmapId: status.roadmapId,
    reason: undefined,
    on: item.kind === 'switch' ? Boolean(item.on) : undefined,
    value: item.kind === 'option' ? item.value : undefined,
    flag: item.flag,
    command: item.command,
    activationSteps: item.activationSteps,
  };
}

/**
 * Map the structured `Menu` model to QuickPick rows.
 *
 * Group titles become separators; every switch, option and action becomes a
 * selectable row whose `key` is the canonical menu key the server expects.
 *
 * @param {object|undefined} menu
 * @returns {object[]}
 */
function quickPickItems(menu) {
  const rows = [];
  for (const group of (menu && menu.groups) || []) {
    rows.push({ kind: 'separator', label: group.title });
    for (const item of group.items || []) {
      rows.push(quickPickItem(item));
    }
  }
  return rows;
}

const SPECIAL_CHARS = {
  ' ': 'Space',
  '+': 'Plus',
};

/**
 * The canonical key step for one typed character (§3.8).
 *
 * @param {string} character
 * @returns {string}
 */
function charToKey(character) {
  return SPECIAL_CHARS[character] || character;
}

/**
 * Turn typed QuickPick input into `view/key` steps.
 *
 * Menu keys are characters (`c`) or two-step prefixes (`-a`, `=A`), so each
 * typed character becomes its own key request and the server keeps the pending
 * prefix. The QuickPick input is cleared after forwarding.
 *
 * @param {string|undefined} value
 * @returns {string[]}
 */
function routeTyped(value) {
  if (typeof value !== 'string') {
    return [];
  }
  return Array.from(value).map(charToKey);
}

module.exports = {
  quickPickItems,
  quickPickItem,
  describeItem,
  routeTyped,
  charToKey,
};
