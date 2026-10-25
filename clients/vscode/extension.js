'use strict';

const session = require('./lib/session');

module.exports = { activate: session.activate, deactivate: session.deactivate };
