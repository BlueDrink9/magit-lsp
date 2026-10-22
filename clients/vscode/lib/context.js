'use strict';

/**
 * The static keymap scope of a view descriptor.
 *
 * Menu views carry `menu.<name>` so the server can keep their model separate,
 * but every menu shares the generated `menu` bindings.
 *
 * @param {string|undefined} scope
 * @returns {string}
 */
function staticScope(scope) {
  if (typeof scope !== 'string') {
    return '';
  }
  return scope.startsWith('menu.') ? 'menu' : scope;
}

/**
 * Derive the VSCode context keys for a focused magit view.
 *
 * Static clients never receive `view/keymap`, so the descriptor and the mode
 * from `view/render` are the only sources for these keys.
 *
 * @param {object|undefined} descriptor a `ViewDescriptor` from `view/open` or a view effect
 * @param {string|undefined} mode the current `ViewRender.mode`
 * @returns {{'magit.active': boolean, 'magit.scope': string, 'magit.textMode': boolean, 'magit.capture': boolean}}
 */
function deriveContext(descriptor, mode) {
  const textMode = mode === 'text';
  return {
    'magit.active': Boolean(descriptor && descriptor.id),
    'magit.scope': staticScope(descriptor && descriptor.scope),
    'magit.textMode': textMode,
    // Text mode narrows the server keymap to the toggle, so capture is off.
    'magit.capture': Boolean(descriptor && descriptor.capture) && !textMode,
  };
}

/**
 * The context keys to set while no magit view is focused.
 *
 * @returns {{'magit.active': boolean, 'magit.scope': string, 'magit.textMode': boolean, 'magit.capture': boolean}}
 */
function inactiveContext() {
  return {
    'magit.active': false,
    'magit.scope': '',
    'magit.textMode': false,
    'magit.capture': false,
  };
}

module.exports = { deriveContext, inactiveContext, staticScope };
