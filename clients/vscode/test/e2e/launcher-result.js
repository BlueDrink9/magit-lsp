"use strict";

/** Convert a suite outcome into text-safe launcher result data. */
function suiteResult(error) {
  return error
    ? {
        status: 1,
        error: { message: String(error.message || error), stack: String(error.stack || "") },
      }
    : { status: 0, error: null };
}

/** Format a launcher failure with its original exception details. */
function failureText(result) {
  if (!result.error) return "";
  return `${result.error.message}\n${result.error.stack}`;
}

module.exports = { suiteResult, failureText };
