'use strict';

/**
 * A readable message for an unknown error value.
 *
 * @param {unknown} error
 * @returns {string}
 */
function message(error) {
  return error && error.message ? error.message : String(error);
}

module.exports = { message };
