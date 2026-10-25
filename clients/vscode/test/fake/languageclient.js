'use strict';

/**
 * A minimal `LanguageClient` replacement that speaks LSP over stdio.
 *
 * The node integration test needs the extension to run against the real
 * `magit-lsp` binary without VSCode or `vscode-languageclient`'s editor
 * services. This client implements the JSON-RPC framing, the initialize
 * handshake and the request/notification surface the extension uses. It
 * records every outgoing message so tests can assert what the client sent.
 */

const { spawn } = require('node:child_process');

/** Every client this process constructed, for tests to inspect. */
const instances = [];

/** One outgoing message, recorded for assertions. */
class Recording {
  constructor(method, params) {
    this.method = method;
    this.params = params;
  }
}

/** A JSON-RPC language client over a child process's stdio. */
class LanguageClient {
  /**
   * @param {string} id
   * @param {string} name
   * @param {{command: string, args?: string[]}} serverOptions
   * @param {object} clientOptions
   */
  constructor(id, name, serverOptions, clientOptions) {
    this.id = id;
    this.name = name;
    this.serverOptions = serverOptions;
    this.clientOptions = clientOptions;
    this.notifications = new Map();
    this.pending = new Map();
    this.nextId = 1;
    this.buffer = Buffer.alloc(0);
    this.requests = [];
    this.notes = [];
    this.initializeResult = undefined;
    this.stderr = '';
    this.exited = undefined;
    instances.push(this);
  }

  /**
   * Register a handler for a server notification.
   *
   * @param {string} method
   * @param {(params: unknown) => void} handler
   */
  onNotification(method, handler) {
    this.notifications.set(method, handler);
  }

  /**
   * Start the server, run the LSP initialize handshake and resolve.
   *
   * @returns {Promise<void>}
   */
  async start() {
    this.process = spawn(this.serverOptions.command, this.serverOptions.args || [], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.exited = new Promise((resolve) => {
      this.process.on('exit', (code, signal) => {
        for (const { reject } of this.pending.values()) {
          reject(new Error(`magit-lsp exited (${code ?? signal})`));
        }
        this.pending.clear();
        resolve({ code, signal });
      });
    });
    this.process.stdout.on('data', (chunk) => this.receive(chunk));
    this.process.stderr.on('data', (chunk) => { this.stderr += chunk.toString(); });

    const params = {
      processId: process.pid,
      clientInfo: { name: 'magit-lsp-node-test', version: '0.0.0' },
      rootUri: this.clientOptions.rootUri || null,
      capabilities: {},
      initializationOptions: this.clientOptions.initializationOptions,
    };
    const next = async (requestParams) => this.request('initialize', requestParams);
    const middleware = this.clientOptions.middleware || {};
    if (middleware.initialize) {
      this.initializeResult = await middleware.initialize(params, next, undefined);
    } else {
      this.initializeResult = await next(params);
    }
    this.sendNotification('initialized', {});
  }

  /**
   * Send one request and resolve with its result.
   *
   * @param {string} method
   * @param {unknown} params
   * @returns {Promise<any>}
   */
  sendRequest(method, params) {
    return this.request(method, params);
  }

  /**
   * Send one request, recording it for assertions first.
   *
   * @param {string} method
   * @param {unknown} params
   * @returns {Promise<any>}
   */
  request(method, params) {
    const id = this.nextId++;
    this.requests.push(new Recording(method, params));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ jsonrpc: '2.0', id, method, params });
    });
  }

  /**
   * Send one notification and record it.
   *
   * @param {string} method
   * @param {unknown} params
   */
  sendNotification(method, params) {
    this.notes.push(new Recording(method, params));
    this.write({ jsonrpc: '2.0', method, params });
  }

  /**
   * Stop the server cleanly, then force it down if it lingers.
   *
   * @returns {Promise<void>}
   */
  async stop() {
    if (!this.process || this.process.exitCode !== null) {
      return;
    }
    try {
      await this.request('shutdown', null);
      this.sendNotification('exit', null);
    } catch {
      // The server may already be gone; the kill below is the fallback.
    }
    const timedOut = await Promise.race([
      this.exited.then(() => false),
      new Promise((resolve) => setTimeout(() => resolve(true), 5000)),
    ]);
    if (timedOut) {
      this.process.kill('SIGKILL');
      await this.exited;
    }
  }

  /**
   * Frame and write one JSON-RPC message.
   *
   * @param {object} message
   */
  write(message) {
    const body = Buffer.from(JSON.stringify(message), 'utf8');
    this.process.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.process.stdin.write(body);
  }

  /**
   * Consume an arbitrary stdout chunk, keeping incomplete frames buffered.
   *
   * @param {Buffer} chunk
   */
  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const headerEnd = this.buffer.indexOf('\r\n\r\n');
      if (headerEnd < 0) {
        return;
      }
      const header = this.buffer.subarray(0, headerEnd).toString('ascii');
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) {
        throw new Error(`malformed LSP header: ${header}`);
      }
      const length = Number(match[1]);
      const start = headerEnd + 4;
      if (this.buffer.length < start + length) {
        return;
      }
      const body = this.buffer.subarray(start, start + length).toString('utf8');
      this.buffer = this.buffer.subarray(start + length);
      this.dispatch(JSON.parse(body));
    }
  }

  /**
   * Route one decoded message to a pending request or a notification handler.
   *
   * @param {object} message
   */
  dispatch(message) {
    if (message.id !== undefined && message.method === undefined) {
      const pending = this.pending.get(message.id);
      if (!pending) {
        return;
      }
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(message.error.message || JSON.stringify(message.error)));
      } else {
        pending.resolve(message.result);
      }
      return;
    }
    if (message.method !== undefined && message.id !== undefined) {
      // Server-originated requests are not part of this protocol; answer null.
      this.write({ jsonrpc: '2.0', id: message.id, result: null });
      return;
    }
    if (message.method !== undefined) {
      const handler = this.notifications.get(message.method);
      if (handler) {
        handler(message.params);
      }
    }
  }
}

module.exports = { LanguageClient, instances };
