'use strict';

/**
 * Compile-time fixture for the generated wire declarations.
 *
 * `make vscode-test` type-checks this file through `jsconfig.json`, so it
 * guards two things at once: valid payloads must satisfy the generated
 * declarations when passed to the production handlers, and concrete malformed
 * payloads must stay rejected. A `@ts-expect-error` here fails the build when
 * the error disappears, e.g. if a required field becomes optional.
 *
 * It is never required by the runtime test suites; only the type check
 * consumes it.
 */

const { applyRender } = require('../lib/render');
const { quickPickItems, quickPickItem } = require('../lib/menu');
const { promptItems, answerFor } = require('../lib/prompt');

/** A full status render as the server sends it. @type {import('../types/protocol').ViewRender.Root} */
const fullRender = {
  viewId: 'status:repo',
  version: 7,
  mode: 'interactive',
  text: ['# status', ''],
  cursor: { line: 0, column: 0 },
};

/** The same render delivered as a line-range update. @type {import('../types/protocol').ViewRender.Root} */
const editRender = {
  viewId: 'status:repo',
  version: 8,
  baseVersion: 7,
  mode: 'interactive',
  edits: [{ startLine: 0, endLine: 1, lines: ['# status'] }],
};

/** The production render handler accepts the generated payload types. */
applyRender([], 7, fullRender);
applyRender(['# status'], 7, editRender);

/** A serialized menu as `view/render` carries it, including `activationSteps`. @type {import('../types/protocol').Menu.Root} */
const menuPayload = {
  id: 'menu:repo',
  title: 'Menu',
  savedArgs: { options: {}, switches: [] },
  groups: [
    {
      title: 'Actions',
      items: [
        {
          kind: 'action',
          key: 'l',
          label: 'Log',
          command: 'log',
          status: { status: 'implemented' },
          activationSteps: [],
        },
      ],
    },
  ],
};

/** The production menu handler consumes the generated menu declaration. */
quickPickItems(menuPayload);

/** A confirm prompt as `view/prompt` sends it. @type {import('../types/protocol').ViewPromptParams.Root} */
const confirmPrompt = {
  promptId: 'prompt:repo',
  kind: 'confirm',
  title: 'Proceed?',
  destructive: false,
};

/** The production prompt handlers consume the generated prompt declaration. */
promptItems(confirmPrompt);
answerFor(confirmPrompt, { id: 'yes', label: 'Yes' });

/**
 * Malformed menu item: the wire requires `activationSteps` on every serialized
 * item beside the switch/option/action combinator.
 */
// @ts-expect-error activationSteps is required on every serialized menu item
quickPickItem({ kind: 'action', key: 'l', label: 'Log', command: 'log', status: { status: 'implemented' } });

/** Malformed render: `mode` is a closed union and `grid` is not a wire value. */
// @ts-expect-error grid is not a ViewMode
applyRender([], 7, { viewId: 'status:repo', version: 8, mode: 'grid', text: [] });
