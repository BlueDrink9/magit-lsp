'use strict';

/** Register theme-addressable tokens using the server legend and ranges verbatim. */
function register(vscode, client, stateForUri, subscriptions) {
  const serverLegend = client.initializeResult.capabilities.semanticTokensProvider.legend;
  const legend = new vscode.SemanticTokensLegend(
    serverLegend.tokenTypes.map((type) => type.replaceAll('.', '-')),
    serverLegend.tokenModifiers
  );
  const changed = new vscode.EventEmitter();
  subscriptions.push(changed, vscode.languages.registerDocumentSemanticTokensProvider(
    { scheme: 'magit', language: 'magit' },
    {
      onDidChangeSemanticTokens: changed.event,
      async provideDocumentSemanticTokens(document) {
        const state = stateForUri(document.uri);
        if (!state) return undefined;
        const tokens = await client.sendRequest('textDocument/semanticTokens/full', {
          textDocument: { uri: state.descriptor.uri },
        });
        return tokens && new vscode.SemanticTokens(new Uint32Array(tokens.data), tokens.resultId);
      },
    }, legend
  ));
  return changed;
}

module.exports = { register };
