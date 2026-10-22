'use strict';

/**
 * Normalise the wire `MenuStatus` into a plain descriptor.
 *
 * The server serialises the status as an internally tagged object, e.g.
 * `{"status":"implemented"}` or `{"status":"planned","roadmapId":"R-4.1"}`.
 *
 * @param {object|string|undefined} status
 * @returns {{kind: string, roadmapId?: string, reason?: string}}
 */
function statusInfo(status) {
  if (typeof status === 'string') {
    return { kind: status };
  }
  if (status && typeof status === 'object') {
    return {
      kind: status.status || 'implemented',
      roadmapId: status.roadmapId,
      reason: status.reason,
    };
  }
  return { kind: 'implemented' };
}

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
  if (status.kind === 'planned') {
    parts.push(status.roadmapId ? `planned ${status.roadmapId}` : 'planned');
  } else if (status.kind === 'unavailable') {
    parts.push(status.reason || 'unavailable');
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
  const status = statusInfo(item.status);
  return {
    kind: 'item',
    label: `${item.key}  ${item.label}`,
    description: describeItem(item, status),
    key: item.key,
    itemKind: item.kind,
    status: status.kind,
    roadmapId: status.roadmapId,
    reason: status.reason,
    on: item.kind === 'switch' ? Boolean(item.on) : undefined,
    value: item.kind === 'option' ? item.value : undefined,
    flag: item.flag,
    command: item.command,
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

/**
 * The newly typed part of a QuickPick value.
 *
 * `onDidChangeValue` can coalesce fast keystrokes; when the previous value is a
 * prefix of the new one only the suffix is new.
 *
 * @param {string} previous
 * @param {string} current
 * @returns {string}
 */
function typedDelta(previous, current) {
  if (!current) {
    return '';
  }
  if (previous && current.startsWith(previous)) {
    return current.slice(previous.length);
  }
  return current;
}

/**
 * Whether a key closes the menu. The server owns this rule; this helper lets
 * the QuickPick stop forwarding after a pop key.
 *
 * @param {string} key
 * @returns {boolean}
 */
function isPopKey(key) {
  return key === 'Escape' || key === 'ctrl+c' || key === 'q';
}

module.exports = {
  quickPickItems,
  quickPickItem,
  describeItem,
  statusInfo,
  routeTyped,
  typedDelta,
  charToKey,
  isPopKey,
};
