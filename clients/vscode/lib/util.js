"use strict";

/**
 * A readable message for an unknown error value.
 *
 * @param {any} error a caught or thrown boundary value
 * @returns {string}
 */
function message(error) {
  return error && error.message ? error.message : String(error);
}

module.exports = { message };
