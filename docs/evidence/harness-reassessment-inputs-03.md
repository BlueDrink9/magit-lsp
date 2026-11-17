## B. VSCode — source evidence (checkout-relative; home/user values redacted only)

Commands used: `find clients/vscode/{lib,test,scripts} -type f | sort`; `wc -l clients/vscode/lib/*.js clients/vscode/test/*.js clients/vscode/test/fake/*.js clients/vscode/test/e2e/* clients/vscode/scripts/*`; `rg -n "test\(|function |exports\.|process\.env|window\.dialogStyle|view/open|wire\.jsonl" clients/vscode build`; `nl -ba` on the sources excerpted below. Line-numbered text is literal source (prefix is index, not source).

### Manifest excerpts

### `clients/vscode/package.json` lines 1-27 of 1838
```text
   1 | {
   2 |   "name": "magit-lsp",
   3 |   "displayName": "magit-lsp",
   4 |   "description": "Magit-style Git views rendered by the magit-lsp language server",
   5 |   "version": "0.1.0",
   6 |   "publisher": "magit-lsp",
   7 |   "private": true,
   8 |   "license": "Apache-2.0",
   9 |   "engines": {
  10 |     "vscode": "^1.85.0"
  11 |   },
  12 |   "categories": [
  13 |     "Other"
  14 |   ],
  15 |   "activationEvents": [
  16 |     "onCommand:magit.open",
  17 |     "onCommand:magit.key",
  18 |     "onCommand:magit.invoke",
  19 |     "onCommand:magit.mirrorFoldShortcuts"
  20 |   ],
  21 |   "main": "./extension.js",
  22 |   "scripts": {
  23 |     "gen:keybindings": "node scripts/gen-keybindings.mjs",
  24 |     "test": "node test/test.js && node test/integration.js",
  25 |     "test:unit": "node test/test.js",
  26 |     "test:integration": "node test/integration.js",
  27 |     "test:e2e": "node test/e2e/run.mjs"
```
`activationEvents` and scripts are verbatim above. Contribution manifests occupy additional package ranges and include keybindings/configuration; source line index from `rg -n '"contributes"|"commands"|"keybindings"|"configuration"' clients/vscode/package.json`: 36:  "contributes": {
120:    "commands": [
179:    "keybindings": [
1813:    "configuration": {

### Full package manifest (contributes, activationEvents and scripts verbatim)
```text
   1 | {
   2 |   "name": "magit-lsp",
   3 |   "displayName": "magit-lsp",
   4 |   "description": "Magit-style Git views rendered by the magit-lsp language server",
   5 |   "version": "0.1.0",
   6 |   "publisher": "magit-lsp",
   7 |   "private": true,
   8 |   "license": "Apache-2.0",
   9 |   "engines": {
  10 |     "vscode": "^1.85.0"
  11 |   },
  12 |   "categories": [
  13 |     "Other"
  14 |   ],
  15 |   "activationEvents": [
  16 |     "onCommand:magit.open",
  17 |     "onCommand:magit.key",
  18 |     "onCommand:magit.invoke",
  19 |     "onCommand:magit.mirrorFoldShortcuts"
  20 |   ],
  21 |   "main": "./extension.js",
  22 |   "scripts": {
  23 |     "gen:keybindings": "node scripts/gen-keybindings.mjs",
  24 |     "test": "node test/test.js && node test/integration.js",
  25 |     "test:unit": "node test/test.js",
  26 |     "test:integration": "node test/integration.js",
  27 |     "test:e2e": "node test/e2e/run.mjs"
  28 |   },
  29 |   "dependencies": {
  30 |     "jsonc-parser": "^3.3.1",
  31 |     "vscode-languageclient": "^9.0.1"
  32 |   },
  33 |   "devDependencies": {
  34 |     "typescript": "^6.0.3"
  35 |   },
  36 |   "contributes": {
  37 |     "languages": [
  38 |       {
  39 |         "id": "magit",
  40 |         "aliases": [
  41 |           "Magit",
  42 |           "magit"
  43 |         ]
  44 |       }
  45 |     ],
  46 |     "configurationDefaults": {
  47 |       "[magit]": {
  48 |         "editor.semanticHighlighting.enabled": true
  49 |       }
  50 |     },
  51 |     "semanticTokenTypes": [
  52 |       {
  53 |         "id": "magit-section",
  54 |         "superType": "namespace",
  55 |         "description": "Status section heading"
  56 |       },
  57 |       {
  58 |         "id": "magit-diff-added",
  59 |         "description": "Added diff line"
  60 |       },
  61 |       {
  62 |         "id": "magit-diff-removed",
  63 |         "description": "Removed diff line"
  64 |       },
  65 |       {
  66 |         "id": "magit-message-error",
  67 |         "description": "Error output from a failed command"
  68 |       },
  69 |       {
  70 |         "id": "magit-message-warn",
  71 |         "description": "Warning output from a successful command"
  72 |       }
  73 |     ],
  74 |     "semanticTokenScopes": [
  75 |       {
  76 |         "language": "magit",
  77 |         "scopes": {
  78 |           "magit-section": [
  79 |             "entity.name.section"
  80 |           ],
  81 |           "magit-count": [
  82 |             "constant.numeric"
  83 |           ],
  84 |           "magit-file": [
  85 |             "string"
  86 |           ],
  87 |           "magit-file-modified": [
  88 |             "markup.changed"
  89 |           ],
  90 |           "magit-file-added": [
  91 |             "markup.inserted"
  92 |           ],
  93 |           "magit-file-deleted": [
  94 |             "markup.deleted"
  95 |           ],
  96 |           "magit-file-renamed": [
  97 |             "markup.changed"
  98 |           ],
  99 |           "magit-file-untracked": [
 100 |             "comment"
 101 |           ],
 102 |           "magit-hunk-header": [
 103 |             "meta.diff.range"
 104 |           ],
 105 |           "magit-diff-added": [
 106 |             "markup.inserted"
 107 |           ],
 108 |           "magit-diff-removed": [
 109 |             "markup.deleted"
 110 |           ],
 111 |           "magit-message-error": [
 112 |             "invalid"
 113 |           ],
 114 |           "magit-message-warn": [
 115 |             "markup.warning"
 116 |           ]
 117 |         }
 118 |       }
 119 |     ],
 120 |     "commands": [
 121 |       {
 122 |         "command": "magit.confirmEdit",
 123 |         "title": "Magit: Confirm Commit Draft",
 124 |         "enablement": "resourceScheme == magit-commit",
 125 |         "icon": "$(check)"
 126 |       },
 127 |       {
 128 |         "command": "magit.cancelEdit",
 129 |         "title": "Magit: Abort Commit Draft",
 130 |         "enablement": "resourceScheme == magit-commit"
 131 |       },
 132 |       {
 133 |         "command": "magit.closeEdit",
 134 |         "title": "Magit: Close Commit Draft (Saved Checkpoint)",
 135 |         "enablement": "resourceScheme == magit-commit",
 136 |         "icon": "$(close)"
 137 |       },
 138 |       {
 139 |         "command": "magit.open",
 140 |         "title": "Open Status",
 141 |         "category": "Magit"
 142 |       },
 143 |       {
 144 |         "command": "magit.key",
 145 |         "title": "Press Key",
 146 |         "category": "Magit",
 147 |         "enablement": "magit.active"
 148 |       },
 149 |       {
 150 |         "command": "magit.invoke",
 151 |         "title": "Invoke Command",
 152 |         "category": "Magit",
 153 |         "enablement": "magit.active"
 154 |       },
 155 |       {
 156 |         "command": "magit.mirrorFoldShortcuts",
 157 |         "title": "Mirror my fold shortcuts",
 158 |         "category": "Magit"
 159 |       }
 160 |     ],
 161 |     "menus": {
 162 |       "editor/title": [
 163 |         {
 164 |           "command": "magit.confirmEdit",
 165 |           "when": "resourceScheme == magit-commit",
 166 |           "group": "navigation@1"
 167 |         },
 168 |         {
 169 |           "command": "magit.closeEdit",
 170 |           "when": "resourceScheme == magit-commit",
 171 |           "group": "navigation@2"
 172 |         },
 173 |         {
 174 |           "command": "magit.cancelEdit",
 175 |           "when": "resourceScheme == magit-commit"
 176 |         }
 177 |       ]
 178 |     },
 179 |     "keybindings": [
 180 |       {
 181 |         "key": "f5",
 182 |         "command": "magit.invoke",
 183 |         "args": {
 184 |           "command": "common.refresh"
 185 |         },
 186 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 187 |       },
 188 |       {
 189 |         "key": "g",
 190 |         "command": "magit.invoke",
 191 |         "args": {
 192 |           "command": "common.refresh"
 193 |         },
 194 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 195 |       },
 196 |       {
 197 |         "key": "tab",
 198 |         "command": "magit.invoke",
 199 |         "args": {
 200 |           "command": "common.toggle-section"
 201 |         },
 202 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 203 |       },
 204 |       {
 205 |         "key": "alt+1",
 206 |         "command": "magit.invoke",
 207 |         "args": {
 208 |           "command": "common.detail-level-1"
 209 |         },
 210 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 211 |       },
 212 |       {
 213 |         "key": "alt+2",
 214 |         "command": "magit.invoke",
 215 |         "args": {
 216 |           "command": "common.detail-level-2"
 217 |         },
 218 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 219 |       },
 220 |       {
 221 |         "key": "alt+3",
 222 |         "command": "magit.invoke",
 223 |         "args": {
 224 |           "command": "common.detail-level-3"
 225 |         },
 226 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 227 |       },
 228 |       {
 229 |         "key": "alt+4",
 230 |         "command": "magit.invoke",
 231 |         "args": {
 232 |           "command": "common.detail-level-4"
 233 |         },
 234 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 235 |       },
 236 |       {
 237 |         "key": "n",
 238 |         "command": "magit.invoke",
 239 |         "args": {
 240 |           "command": "common.next-section"
 241 |         },
 242 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 243 |       },
 244 |       {
 245 |         "key": "p",
 246 |         "command": "magit.invoke",
 247 |         "args": {
 248 |           "command": "common.prev-section"
 249 |         },
 250 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 251 |       },
 252 |       {
 253 |         "key": "escape",
 254 |         "command": "magit.invoke",
 255 |         "args": {
 256 |           "command": "common.cancel"
 257 |         },
 258 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 259 |       },
 260 |       {
 261 |         "key": "ctrl+c",
 262 |         "command": "magit.invoke",
 263 |         "args": {
 264 |           "command": "common.cancel"
 265 |         },
 266 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 267 |       },
 268 |       {
 269 |         "key": "enter",
 270 |         "command": "magit.invoke",
 271 |         "args": {
 272 |           "command": "common.confirm"
 273 |         },
 274 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 275 |       },
 276 |       {
 277 |         "key": "q",
 278 |         "command": "magit.invoke",
 279 |         "args": {
 280 |           "command": "common.quit"
 281 |         },
 282 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 283 |       },
 284 |       {
 285 |         "key": "shift+tab",
 286 |         "command": "magit.invoke",
 287 |         "args": {
 288 |           "command": "common.cycle-visibility"
 289 |         },
 290 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 291 |       },
 292 |       {
 293 |         "key": "alt+down",
 294 |         "command": "magit.invoke",
 295 |         "args": {
 296 |           "command": "common.next-sibling"
 297 |         },
 298 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 299 |       },
 300 |       {
 301 |         "key": "alt+up",
 302 |         "command": "magit.invoke",
 303 |         "args": {
 304 |           "command": "common.prev-sibling"
 305 |         },
 306 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 307 |       },
 308 |       {
 309 |         "key": "alt+left",
 310 |         "command": "magit.invoke",
 311 |         "args": {
 312 |           "command": "common.parent-section"
 313 |         },
 314 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 315 |       },
 316 |       {
 317 |         "key": "ctrl+t",
 318 |         "command": "magit.invoke",
 319 |         "args": {
 320 |           "command": "common.toggle-text-mode"
 321 |         },
 322 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 323 |       },
 324 |       {
 325 |         "key": "ctrl+t",
 326 |         "command": "magit.invoke",
 327 |         "args": {
 328 |           "command": "common.toggle-text-mode"
 329 |         },
 330 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && magit.textMode"
 331 |       },
 332 |       {
 333 |         "key": "ctrl+shift+c",
 334 |         "command": "magit.invoke",
 335 |         "args": {
 336 |           "command": "common.copy-value"
 337 |         },
 338 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 339 |       },
 340 |       {
 341 |         "key": "shift+/",
 342 |         "command": "magit.invoke",
 343 |         "args": {
 344 |           "command": "open.dispatch"
 345 |         },
 346 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 347 |       },
 348 |       {
 349 |         "key": "f5",
 350 |         "command": "magit.invoke",
 351 |         "args": {
 352 |           "command": "common.refresh"
 353 |         },
 354 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 355 |       },
 356 |       {
 357 |         "key": "g",
 358 |         "command": "magit.invoke",
 359 |         "args": {
 360 |           "command": "common.refresh"
 361 |         },
 362 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 363 |       },
 364 |       {
 365 |         "key": "tab",
 366 |         "command": "magit.invoke",
 367 |         "args": {
 368 |           "command": "common.toggle-section"
 369 |         },
 370 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 371 |       },
 372 |       {
 373 |         "key": "alt+1",
 374 |         "command": "magit.invoke",
 375 |         "args": {
 376 |           "command": "common.detail-level-1"
 377 |         },
 378 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 379 |       },
 380 |       {
 381 |         "key": "alt+2",
 382 |         "command": "magit.invoke",
 383 |         "args": {
 384 |           "command": "common.detail-level-2"
 385 |         },
 386 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 387 |       },
 388 |       {
 389 |         "key": "alt+3",
 390 |         "command": "magit.invoke",
 391 |         "args": {
 392 |           "command": "common.detail-level-3"
 393 |         },
 394 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 395 |       },
 396 |       {
 397 |         "key": "alt+4",
 398 |         "command": "magit.invoke",
 399 |         "args": {
 400 |           "command": "common.detail-level-4"
 401 |         },
 402 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 403 |       },
 404 |       {
 405 |         "key": "n",
 406 |         "command": "magit.invoke",
 407 |         "args": {
 408 |           "command": "common.next-section"
 409 |         },
 410 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 411 |       },
 412 |       {
 413 |         "key": "p",
 414 |         "command": "magit.invoke",
 415 |         "args": {
 416 |           "command": "common.prev-section"
 417 |         },
 418 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 419 |       },
 420 |       {
 421 |         "key": "s",
 422 |         "command": "magit.invoke",
 423 |         "args": {
 424 |           "command": "status.stage"
 425 |         },
 426 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 427 |       },
 428 |       {
 429 |         "key": "u",
 430 |         "command": "magit.invoke",
 431 |         "args": {
 432 |           "command": "status.unstage"
 433 |         },
 434 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 435 |       },
 436 |       {
 437 |         "key": "shift+s",
 438 |         "command": "magit.invoke",
 439 |         "args": {
 440 |           "command": "status.stage-all"
 441 |         },
 442 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 443 |       },
 444 |       {
 445 |         "key": "shift+u",
 446 |         "command": "magit.invoke",
 447 |         "args": {
 448 |           "command": "status.unstage-all"
 449 |         },
 450 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 451 |       },
 452 |       {
 453 |         "key": "escape",
 454 |         "command": "magit.invoke",
 455 |         "args": {
 456 |           "command": "common.cancel"
 457 |         },
 458 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 459 |       },
 460 |       {
 461 |         "key": "ctrl+c",
 462 |         "command": "magit.invoke",
 463 |         "args": {
 464 |           "command": "common.cancel"
 465 |         },
 466 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 467 |       },
 468 |       {
 469 |         "key": "q",
 470 |         "command": "magit.invoke",
 471 |         "args": {
 472 |           "command": "common.quit"
 473 |         },
 474 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 475 |       },
 476 |       {
 477 |         "key": "shift+tab",
 478 |         "command": "magit.invoke",
 479 |         "args": {
 480 |           "command": "common.cycle-visibility"
 481 |         },
 482 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 483 |       },
 484 |       {
 485 |         "key": "alt+down",
 486 |         "command": "magit.invoke",
 487 |         "args": {
 488 |           "command": "common.next-sibling"
 489 |         },
 490 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 491 |       },
 492 |       {
 493 |         "key": "alt+up",
 494 |         "command": "magit.invoke",
 495 |         "args": {
 496 |           "command": "common.prev-sibling"
 497 |         },
 498 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 499 |       },
 500 |       {
 501 |         "key": "alt+left",
 502 |         "command": "magit.invoke",
 503 |         "args": {
 504 |           "command": "common.parent-section"
 505 |         },
 506 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 507 |       },
 508 |       {
 509 |         "key": "ctrl+t",
 510 |         "command": "magit.invoke",
 511 |         "args": {
 512 |           "command": "common.toggle-text-mode"
 513 |         },
 514 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 515 |       },
 516 |       {
 517 |         "key": "ctrl+t",
 518 |         "command": "magit.invoke",
 519 |         "args": {
 520 |           "command": "common.toggle-text-mode"
 521 |         },
 522 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && magit.textMode"
 523 |       },
 524 |       {
 525 |         "key": "ctrl+shift+c",
 526 |         "command": "magit.invoke",
 527 |         "args": {
 528 |           "command": "common.copy-value"
 529 |         },
 530 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 531 |       },
 532 |       {
 533 |         "key": "enter",
 534 |         "command": "magit.invoke",
 535 |         "args": {
 536 |           "command": "common.visit"
 537 |         },
 538 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 539 |       },
 540 |       {
 541 |         "key": "delete",
 542 |         "command": "magit.invoke",
 543 |         "args": {
 544 |           "command": "status.discard"
 545 |         },
 546 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 547 |       },
 548 |       {
 549 |         "key": "v",
 550 |         "command": "magit.invoke",
 551 |         "args": {
 552 |           "command": "status.reverse"
 553 |         },
 554 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 555 |       },
 556 |       {
 557 |         "key": "a",
 558 |         "command": "magit.invoke",
 559 |         "args": {
 560 |           "command": "status.apply"
 561 |         },
 562 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 563 |       },
 564 |       {
 565 |         "key": "shift+/",
 566 |         "command": "magit.invoke",
 567 |         "args": {
 568 |           "command": "open.dispatch"
 569 |         },
 570 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 571 |       },
 572 |       {
 573 |         "key": "h",
 574 |         "command": "magit.invoke",
 575 |         "args": {
 576 |           "command": "open.dispatch"
 577 |         },
 578 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 579 |       },
 580 |       {
 581 |         "key": "c",
 582 |         "command": "magit.invoke",
 583 |         "args": {
 584 |           "command": "open.commit"
 585 |         },
 586 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 587 |       },
 588 |       {
 589 |         "key": "b",
 590 |         "command": "magit.invoke",
 591 |         "args": {
 592 |           "command": "open.branch"
 593 |         },
 594 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 595 |       },
 596 |       {
 597 |         "key": "l",
 598 |         "command": "magit.invoke",
 599 |         "args": {
 600 |           "command": "open.log"
 601 |         },
 602 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 603 |       },
 604 |       {
 605 |         "key": "d",
 606 |         "command": "magit.invoke",
 607 |         "args": {
 608 |           "command": "open.diff"
 609 |         },
 610 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 611 |       },
 612 |       {
 613 |         "key": "f",
 614 |         "command": "magit.invoke",
 615 |         "args": {
 616 |           "command": "open.fetch"
 617 |         },
 618 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 619 |       },
 620 |       {
 621 |         "key": "shift+f",
 622 |         "command": "magit.invoke",
 623 |         "args": {
 624 |           "command": "open.pull"
 625 |         },
 626 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 627 |       },
 628 |       {
 629 |         "key": "shift+p",
 630 |         "command": "magit.invoke",
 631 |         "args": {
 632 |           "command": "open.push"
 633 |         },
 634 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 635 |       },
 636 |       {
 637 |         "key": "m",
 638 |         "command": "magit.invoke",
 639 |         "args": {
 640 |           "command": "open.merge"
 641 |         },
 642 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 643 |       },
 644 |       {
 645 |         "key": "r",
 646 |         "command": "magit.invoke",
 647 |         "args": {
 648 |           "command": "open.rebase"
 649 |         },
 650 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 651 |       },
 652 |       {
 653 |         "key": "z",
 654 |         "command": "magit.invoke",
 655 |         "args": {
 656 |           "command": "open.stash"
 657 |         },
 658 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 659 |       },
 660 |       {
 661 |         "key": "t",
 662 |         "command": "magit.invoke",
 663 |         "args": {
 664 |           "command": "open.tag"
 665 |         },
 666 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 667 |       },
 668 |       {
 669 |         "key": "x",
 670 |         "command": "magit.invoke",
 671 |         "args": {
 672 |           "command": "open.reset"
 673 |         },
 674 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 675 |       },
 676 |       {
 677 |         "key": "shift+v",
 678 |         "command": "magit.invoke",
 679 |         "args": {
 680 |           "command": "open.revert"
 681 |         },
 682 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 683 |       },
 684 |       {
 685 |         "key": "shift+a",
 686 |         "command": "magit.invoke",
 687 |         "args": {
 688 |           "command": "open.cherry-pick"
 689 |         },
 690 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 691 |       },
 692 |       {
 693 |         "key": "shift+b",
 694 |         "command": "magit.invoke",
 695 |         "args": {
 696 |           "command": "open.bisect"
 697 |         },
 698 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 699 |       },
 700 |       {
 701 |         "key": "shift+m",
 702 |         "command": "magit.invoke",
 703 |         "args": {
 704 |           "command": "open.remote"
 705 |         },
 706 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 707 |       },
 708 |       {
 709 |         "key": "o",
 710 |         "command": "magit.invoke",
 711 |         "args": {
 712 |           "command": "open.submodule"
 713 |         },
 714 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 715 |       },
 716 |       {
 717 |         "key": "shift+5",
 718 |         "command": "magit.invoke",
 719 |         "args": {
 720 |           "command": "open.worktree"
 721 |         },
 722 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 723 |       },
 724 |       {
 725 |         "key": "i",
 726 |         "command": "magit.invoke",
 727 |         "args": {
 728 |           "command": "open.ignore"
 729 |         },
 730 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 731 |       },
 732 |       {
 733 |         "key": "w",
 734 |         "command": "magit.invoke",
 735 |         "args": {
 736 |           "command": "open.patch-apply"
 737 |         },
 738 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 739 |       },
 740 |       {
 741 |         "key": "shift+w",
 742 |         "command": "magit.invoke",
 743 |         "args": {
 744 |           "command": "open.patch-create"
 745 |         },
 746 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 747 |       },
 748 |       {
 749 |         "key": "y",
 750 |         "command": "magit.invoke",
 751 |         "args": {
 752 |           "command": "open.refs"
 753 |         },
 754 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 755 |       },
 756 |       {
 757 |         "key": "shift+y",
 758 |         "command": "magit.invoke",
 759 |         "args": {
 760 |           "command": "open.cherries"
 761 |         },
 762 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 763 |       },
 764 |       {
 765 |         "key": "shift+1",
 766 |         "command": "magit.invoke",
 767 |         "args": {
 768 |           "command": "open.run"
 769 |         },
 770 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 771 |       },
 772 |       {
 773 |         "key": "shift+4",
 774 |         "command": "magit.invoke",
 775 |         "args": {
 776 |           "command": "open.process-log"
 777 |         },
 778 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 779 |       },
 780 |       {
 781 |         "key": "f5",
 782 |         "command": "magit.invoke",
 783 |         "args": {
 784 |           "command": "common.refresh"
 785 |         },
 786 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 787 |       },
 788 |       {
 789 |         "key": "g",
 790 |         "command": "magit.invoke",
 791 |         "args": {
 792 |           "command": "common.refresh"
 793 |         },
 794 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 795 |       },
 796 |       {
 797 |         "key": "tab",
 798 |         "command": "magit.invoke",
 799 |         "args": {
 800 |           "command": "common.toggle-section"
 801 |         },
 802 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 803 |       },
 804 |       {
 805 |         "key": "alt+1",
 806 |         "command": "magit.invoke",
 807 |         "args": {
 808 |           "command": "common.detail-level-1"
 809 |         },
 810 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 811 |       },
 812 |       {
 813 |         "key": "alt+2",
 814 |         "command": "magit.invoke",
 815 |         "args": {
 816 |           "command": "common.detail-level-2"
 817 |         },
 818 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 819 |       },
 820 |       {
 821 |         "key": "alt+3",
 822 |         "command": "magit.invoke",
 823 |         "args": {
 824 |           "command": "common.detail-level-3"
 825 |         },
 826 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 827 |       },
 828 |       {
 829 |         "key": "alt+4",
 830 |         "command": "magit.invoke",
 831 |         "args": {
 832 |           "command": "common.detail-level-4"
 833 |         },
 834 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 835 |       },
 836 |       {
 837 |         "key": "escape",
 838 |         "command": "magit.invoke",
 839 |         "args": {
 840 |           "command": "common.cancel"
 841 |         },
 842 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 843 |       },
 844 |       {
 845 |         "key": "ctrl+c",
 846 |         "command": "magit.invoke",
 847 |         "args": {
 848 |           "command": "common.cancel"
 849 |         },
 850 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 851 |       },
 852 |       {
 853 |         "key": "enter",
 854 |         "command": "magit.invoke",
 855 |         "args": {
 856 |           "command": "common.confirm"
 857 |         },
 858 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 859 |       },
 860 |       {
 861 |         "key": "q",
 862 |         "command": "magit.invoke",
 863 |         "args": {
 864 |           "command": "common.quit"
 865 |         },
 866 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 867 |       },
 868 |       {
 869 |         "key": "shift+tab",
 870 |         "command": "magit.invoke",
 871 |         "args": {
 872 |           "command": "common.cycle-visibility"
 873 |         },
 874 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 875 |       },
 876 |       {
 877 |         "key": "alt+down",
 878 |         "command": "magit.invoke",
 879 |         "args": {
 880 |           "command": "common.next-sibling"
 881 |         },
 882 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 883 |       },
 884 |       {
 885 |         "key": "alt+up",
 886 |         "command": "magit.invoke",
 887 |         "args": {
 888 |           "command": "common.prev-sibling"
 889 |         },
 890 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 891 |       },
 892 |       {
 893 |         "key": "alt+left",
 894 |         "command": "magit.invoke",
 895 |         "args": {
 896 |           "command": "common.parent-section"
 897 |         },
 898 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 899 |       },
 900 |       {
 901 |         "key": "ctrl+t",
 902 |         "command": "magit.invoke",
 903 |         "args": {
 904 |           "command": "common.toggle-text-mode"
 905 |         },
 906 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 907 |       },
 908 |       {
 909 |         "key": "ctrl+t",
 910 |         "command": "magit.invoke",
 911 |         "args": {
 912 |           "command": "common.toggle-text-mode"
 913 |         },
 914 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && magit.textMode"
 915 |       },
 916 |       {
 917 |         "key": "ctrl+shift+c",
 918 |         "command": "magit.invoke",
 919 |         "args": {
 920 |           "command": "common.copy-value"
 921 |         },
 922 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 923 |       },
 924 |       {
 925 |         "key": "shift+=",
 926 |         "command": "magit.invoke",
 927 |         "args": {
 928 |           "command": "log.load-more"
 929 |         },
 930 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 931 |       },
 932 |       {
 933 |         "key": "f5",
 934 |         "command": "magit.invoke",
 935 |         "args": {
 936 |           "command": "common.refresh"
 937 |         },
 938 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 939 |       },
 940 |       {
 941 |         "key": "g",
 942 |         "command": "magit.invoke",
 943 |         "args": {
 944 |           "command": "common.refresh"
 945 |         },
 946 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 947 |       },
 948 |       {
 949 |         "key": "tab",
 950 |         "command": "magit.invoke",
 951 |         "args": {
 952 |           "command": "common.toggle-section"
 953 |         },
 954 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 955 |       },
 956 |       {
 957 |         "key": "alt+1",
 958 |         "command": "magit.invoke",
 959 |         "args": {
 960 |           "command": "common.detail-level-1"
 961 |         },
 962 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 963 |       },
 964 |       {
 965 |         "key": "alt+2",
 966 |         "command": "magit.invoke",
 967 |         "args": {
 968 |           "command": "common.detail-level-2"
 969 |         },
 970 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 971 |       },
 972 |       {
 973 |         "key": "alt+3",
 974 |         "command": "magit.invoke",
 975 |         "args": {
 976 |           "command": "common.detail-level-3"
 977 |         },
 978 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 979 |       },
 980 |       {
 981 |         "key": "alt+4",
 982 |         "command": "magit.invoke",
 983 |         "args": {
 984 |           "command": "common.detail-level-4"
 985 |         },
 986 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 987 |       },
 988 |       {
 989 |         "key": "escape",
 990 |         "command": "magit.invoke",
 991 |         "args": {
 992 |           "command": "common.cancel"
 993 |         },
 994 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 995 |       },
 996 |       {
 997 |         "key": "ctrl+c",
 998 |         "command": "magit.invoke",
 999 |         "args": {
1000 |           "command": "common.cancel"
1001 |         },
1002 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1003 |       },
1004 |       {
1005 |         "key": "enter",
1006 |         "command": "magit.invoke",
1007 |         "args": {
1008 |           "command": "common.confirm"
1009 |         },
1010 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1011 |       },
1012 |       {
1013 |         "key": "q",
1014 |         "command": "magit.invoke",
1015 |         "args": {
1016 |           "command": "common.quit"
1017 |         },
1018 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1019 |       },
1020 |       {
1021 |         "key": "shift+tab",
1022 |         "command": "magit.invoke",
1023 |         "args": {
1024 |           "command": "common.cycle-visibility"
1025 |         },
1026 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1027 |       },
1028 |       {
1029 |         "key": "alt+down",
1030 |         "command": "magit.invoke",
1031 |         "args": {
1032 |           "command": "common.next-sibling"
1033 |         },
1034 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1035 |       },
1036 |       {
1037 |         "key": "alt+up",
1038 |         "command": "magit.invoke",
1039 |         "args": {
1040 |           "command": "common.prev-sibling"
1041 |         },
1042 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1043 |       },
1044 |       {
1045 |         "key": "alt+left",
1046 |         "command": "magit.invoke",
1047 |         "args": {
1048 |           "command": "common.parent-section"
1049 |         },
1050 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1051 |       },
1052 |       {
1053 |         "key": "ctrl+t",
1054 |         "command": "magit.invoke",
1055 |         "args": {
1056 |           "command": "common.toggle-text-mode"
1057 |         },
1058 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1059 |       },
1060 |       {
1061 |         "key": "ctrl+t",
1062 |         "command": "magit.invoke",
1063 |         "args": {
1064 |           "command": "common.toggle-text-mode"
1065 |         },
1066 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && magit.textMode"
1067 |       },
1068 |       {
1069 |         "key": "ctrl+shift+c",
1070 |         "command": "magit.invoke",
1071 |         "args": {
1072 |           "command": "common.copy-value"
1073 |         },
1074 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1075 |       },
1076 |       {
1077 |         "key": "shift+=",
1078 |         "command": "magit.invoke",
1079 |         "args": {
1080 |           "command": "log.load-more"
1081 |         },
1082 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1083 |       },
1084 |       {
1085 |         "key": "space",
1086 |         "command": "magit.invoke",
1087 |         "args": {
1088 |           "command": "select.toggle-mark"
1089 |         },
1090 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1091 |       },
1092 |       {
1093 |         "key": "i",
1094 |         "command": "magit.invoke",
1095 |         "args": {
1096 |           "command": "select.enter-rev"
1097 |         },
1098 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1099 |       },
1100 |       {
1101 |         "key": "f5",
1102 |         "command": "magit.invoke",
1103 |         "args": {
1104 |           "command": "common.refresh"
1105 |         },
1106 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1107 |       },
1108 |       {
1109 |         "key": "g",
1110 |         "command": "magit.invoke",
1111 |         "args": {
1112 |           "command": "common.refresh"
1113 |         },
1114 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1115 |       },
1116 |       {
1117 |         "key": "tab",
1118 |         "command": "magit.invoke",
1119 |         "args": {
1120 |           "command": "common.toggle-section"
1121 |         },
1122 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1123 |       },
1124 |       {
1125 |         "key": "alt+1",
1126 |         "command": "magit.invoke",
1127 |         "args": {
1128 |           "command": "common.detail-level-1"
1129 |         },
1130 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1131 |       },
1132 |       {
1133 |         "key": "alt+2",
1134 |         "command": "magit.invoke",
1135 |         "args": {
1136 |           "command": "common.detail-level-2"
1137 |         },
1138 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1139 |       },
1140 |       {
1141 |         "key": "alt+3",
1142 |         "command": "magit.invoke",
1143 |         "args": {
1144 |           "command": "common.detail-level-3"
1145 |         },
1146 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1147 |       },
1148 |       {
1149 |         "key": "alt+4",
1150 |         "command": "magit.invoke",
1151 |         "args": {
1152 |           "command": "common.detail-level-4"
1153 |         },
1154 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1155 |       },
1156 |       {
1157 |         "key": "n",
1158 |         "command": "magit.invoke",
1159 |         "args": {
1160 |           "command": "common.next-section"
1161 |         },
1162 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1163 |       },
1164 |       {
1165 |         "key": "p",
1166 |         "command": "magit.invoke",
1167 |         "args": {
1168 |           "command": "common.prev-section"
1169 |         },
1170 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1171 |       },
1172 |       {
1173 |         "key": "escape",
1174 |         "command": "magit.invoke",
1175 |         "args": {
1176 |           "command": "common.cancel"
1177 |         },
1178 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1179 |       },
1180 |       {
1181 |         "key": "ctrl+c",
1182 |         "command": "magit.invoke",
1183 |         "args": {
1184 |           "command": "common.cancel"
1185 |         },
1186 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1187 |       },
1188 |       {
1189 |         "key": "enter",
1190 |         "command": "magit.invoke",
1191 |         "args": {
1192 |           "command": "common.confirm"
1193 |         },
1194 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1195 |       },
1196 |       {
1197 |         "key": "q",
1198 |         "command": "magit.invoke",
1199 |         "args": {
1200 |           "command": "common.quit"
1201 |         },
1202 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1203 |       },
1204 |       {
1205 |         "key": "shift+tab",
1206 |         "command": "magit.invoke",
1207 |         "args": {
1208 |           "command": "common.cycle-visibility"
1209 |         },
1210 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1211 |       },
1212 |       {
1213 |         "key": "alt+down",
1214 |         "command": "magit.invoke",
1215 |         "args": {
1216 |           "command": "common.next-sibling"
1217 |         },
1218 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1219 |       },
1220 |       {
1221 |         "key": "alt+up",
1222 |         "command": "magit.invoke",
1223 |         "args": {
1224 |           "command": "common.prev-sibling"
1225 |         },
1226 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1227 |       },
1228 |       {
1229 |         "key": "alt+left",
1230 |         "command": "magit.invoke",
1231 |         "args": {
1232 |           "command": "common.parent-section"
1233 |         },
1234 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1235 |       },
1236 |       {
1237 |         "key": "ctrl+t",
1238 |         "command": "magit.invoke",
1239 |         "args": {
1240 |           "command": "common.toggle-text-mode"
1241 |         },
1242 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1243 |       },
1244 |       {
1245 |         "key": "ctrl+t",
1246 |         "command": "magit.invoke",
1247 |         "args": {
1248 |           "command": "common.toggle-text-mode"
1249 |         },
1250 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && magit.textMode"
1251 |       },
1252 |       {
1253 |         "key": "ctrl+shift+c",
1254 |         "command": "magit.invoke",
1255 |         "args": {
1256 |           "command": "common.copy-value"
1257 |         },
1258 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1259 |       },
1260 |       {
1261 |         "key": "shift+/",
1262 |         "command": "magit.invoke",
1263 |         "args": {
1264 |           "command": "open.dispatch"
1265 |         },
1266 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1267 |       },
1268 |       {
1269 |         "key": "space",
1270 |         "command": "magit.invoke",
1271 |         "args": {
1272 |           "command": "select.toggle-mark"
1273 |         },
1274 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1275 |       },
1276 |       {
1277 |         "key": "i",
1278 |         "command": "magit.invoke",
1279 |         "args": {
1280 |           "command": "select.enter-rev"
1281 |         },
1282 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1283 |       },
1284 |       {
1285 |         "key": "f5",
1286 |         "command": "magit.invoke",
1287 |         "args": {
1288 |           "command": "common.refresh"
1289 |         },
1290 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1291 |       },
1292 |       {
1293 |         "key": "g",
1294 |         "command": "magit.invoke",
1295 |         "args": {
1296 |           "command": "common.refresh"
1297 |         },
1298 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1299 |       },
1300 |       {
1301 |         "key": "tab",
1302 |         "command": "magit.invoke",
1303 |         "args": {
1304 |           "command": "common.toggle-section"
1305 |         },
1306 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1307 |       },
1308 |       {
1309 |         "key": "alt+1",
1310 |         "command": "magit.invoke",
1311 |         "args": {
1312 |           "command": "common.detail-level-1"
1313 |         },
1314 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1315 |       },
1316 |       {
1317 |         "key": "alt+2",
1318 |         "command": "magit.invoke",
1319 |         "args": {
1320 |           "command": "common.detail-level-2"
1321 |         },
1322 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1323 |       },
1324 |       {
1325 |         "key": "alt+3",
1326 |         "command": "magit.invoke",
1327 |         "args": {
1328 |           "command": "common.detail-level-3"
1329 |         },
1330 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1331 |       },
1332 |       {
1333 |         "key": "alt+4",
1334 |         "command": "magit.invoke",
1335 |         "args": {
1336 |           "command": "common.detail-level-4"
1337 |         },
1338 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1339 |       },
1340 |       {
1341 |         "key": "n",
1342 |         "command": "magit.invoke",
1343 |         "args": {
1344 |           "command": "common.next-section"
1345 |         },
1346 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1347 |       },
1348 |       {
1349 |         "key": "p",
1350 |         "command": "magit.invoke",
1351 |         "args": {
1352 |           "command": "common.prev-section"
1353 |         },
1354 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1355 |       },
1356 |       {
1357 |         "key": "escape",
1358 |         "command": "magit.invoke",
1359 |         "args": {
1360 |           "command": "common.cancel"
1361 |         },
1362 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1363 |       },
1364 |       {
1365 |         "key": "ctrl+c",
1366 |         "command": "magit.invoke",
1367 |         "args": {
1368 |           "command": "common.cancel"
1369 |         },
1370 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1371 |       },
1372 |       {
1373 |         "key": "enter",
1374 |         "command": "magit.invoke",
1375 |         "args": {
1376 |           "command": "common.confirm"
1377 |         },
1378 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1379 |       },
1380 |       {
1381 |         "key": "q",
1382 |         "command": "magit.invoke",
1383 |         "args": {
1384 |           "command": "common.quit"
1385 |         },
1386 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1387 |       },
1388 |       {
1389 |         "key": "shift+tab",
1390 |         "command": "magit.invoke",
1391 |         "args": {
1392 |           "command": "common.cycle-visibility"
1393 |         },
1394 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1395 |       },
1396 |       {
1397 |         "key": "alt+down",
1398 |         "command": "magit.invoke",
1399 |         "args": {
1400 |           "command": "common.next-sibling"
1401 |         },
1402 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1403 |       },
1404 |       {
1405 |         "key": "alt+up",
1406 |         "command": "magit.invoke",
1407 |         "args": {
1408 |           "command": "common.prev-sibling"
1409 |         },
1410 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1411 |       },
1412 |       {
1413 |         "key": "alt+left",
1414 |         "command": "magit.invoke",
1415 |         "args": {
1416 |           "command": "common.parent-section"
1417 |         },
1418 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1419 |       },
1420 |       {
1421 |         "key": "ctrl+t",
1422 |         "command": "magit.invoke",
1423 |         "args": {
1424 |           "command": "common.toggle-text-mode"
1425 |         },
1426 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1427 |       },
1428 |       {
1429 |         "key": "ctrl+t",
1430 |         "command": "magit.invoke",
1431 |         "args": {
1432 |           "command": "common.toggle-text-mode"
1433 |         },
1434 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && magit.textMode"
1435 |       },
1436 |       {
1437 |         "key": "ctrl+shift+c",
1438 |         "command": "magit.invoke",
1439 |         "args": {
1440 |           "command": "common.copy-value"
1441 |         },
1442 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1443 |       },
1444 |       {
1445 |         "key": "shift+/",
1446 |         "command": "magit.invoke",
1447 |         "args": {
1448 |           "command": "open.dispatch"
1449 |         },
1450 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1451 |       },
1452 |       {
1453 |         "key": "ctrl+s",
1454 |         "command": "magit.invoke",
1455 |         "args": {
1456 |           "command": "menu.save-args"
1457 |         },
1458 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1459 |       },
1460 |       {
1461 |         "key": "ctrl+r",
1462 |         "command": "magit.invoke",
1463 |         "args": {
1464 |           "command": "menu.reset-args"
1465 |         },
1466 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1467 |       },
1468 |       {
1469 |         "key": "f5",
1470 |         "command": "magit.invoke",
1471 |         "args": {
1472 |           "command": "common.refresh"
1473 |         },
1474 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1475 |       },
1476 |       {
1477 |         "key": "g",
1478 |         "command": "magit.invoke",
1479 |         "args": {
1480 |           "command": "common.refresh"
1481 |         },
1482 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1483 |       },
1484 |       {
1485 |         "key": "tab",
1486 |         "command": "magit.invoke",
1487 |         "args": {
1488 |           "command": "common.toggle-section"
1489 |         },
1490 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1491 |       },
1492 |       {
1493 |         "key": "alt+1",
1494 |         "command": "magit.invoke",
1495 |         "args": {
1496 |           "command": "common.detail-level-1"
1497 |         },
1498 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1499 |       },
1500 |       {
1501 |         "key": "alt+2",
1502 |         "command": "magit.invoke",
1503 |         "args": {
1504 |           "command": "common.detail-level-2"
1505 |         },
1506 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1507 |       },
1508 |       {
1509 |         "key": "alt+3",
1510 |         "command": "magit.invoke",
1511 |         "args": {
1512 |           "command": "common.detail-level-3"
1513 |         },
1514 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1515 |       },
1516 |       {
1517 |         "key": "alt+4",
1518 |         "command": "magit.invoke",
1519 |         "args": {
1520 |           "command": "common.detail-level-4"
1521 |         },
1522 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1523 |       },
1524 |       {
1525 |         "key": "n",
1526 |         "command": "magit.invoke",
1527 |         "args": {
1528 |           "command": "common.next-section"
1529 |         },
1530 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1531 |       },
1532 |       {
1533 |         "key": "p",
1534 |         "command": "magit.invoke",
1535 |         "args": {
1536 |           "command": "common.prev-section"
1537 |         },
1538 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1539 |       },
1540 |       {
1541 |         "key": "ctrl+shift+x",
1542 |         "command": "magit.invoke",
1543 |         "args": {
1544 |           "command": "common.cancel"
1545 |         },
1546 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1547 |       },
1548 |       {
1549 |         "key": "ctrl+enter",
1550 |         "command": "magit.invoke",
1551 |         "args": {
1552 |           "command": "common.confirm"
1553 |         },
1554 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1555 |       },
1556 |       {
1557 |         "key": "q",
1558 |         "command": "magit.invoke",
1559 |         "args": {
1560 |           "command": "common.quit"
1561 |         },
1562 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1563 |       },
1564 |       {
1565 |         "key": "shift+tab",
1566 |         "command": "magit.invoke",
1567 |         "args": {
1568 |           "command": "common.cycle-visibility"
1569 |         },
1570 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1571 |       },
1572 |       {
1573 |         "key": "alt+left",
1574 |         "command": "magit.invoke",
1575 |         "args": {
1576 |           "command": "common.parent-section"
1577 |         },
1578 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1579 |       },
1580 |       {
1581 |         "key": "ctrl+t",
1582 |         "command": "magit.invoke",
1583 |         "args": {
1584 |           "command": "common.toggle-text-mode"
1585 |         },
1586 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1587 |       },
1588 |       {
1589 |         "key": "ctrl+t",
1590 |         "command": "magit.invoke",
1591 |         "args": {
1592 |           "command": "common.toggle-text-mode"
1593 |         },
1594 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && magit.textMode"
1595 |       },
1596 |       {
1597 |         "key": "ctrl+shift+c",
1598 |         "command": "magit.invoke",
1599 |         "args": {
1600 |           "command": "common.copy-value"
1601 |         },
1602 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1603 |       },
1604 |       {
1605 |         "key": "shift+/",
1606 |         "command": "magit.invoke",
1607 |         "args": {
1608 |           "command": "open.dispatch"
1609 |         },
1610 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1611 |       },
1612 |       {
1613 |         "key": "alt+up",
1614 |         "command": "magit.invoke",
1615 |         "args": {
1616 |           "command": "commit-message.history-prev"
1617 |         },
1618 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1619 |       },
1620 |       {
1621 |         "key": "alt+down",
1622 |         "command": "magit.invoke",
1623 |         "args": {
1624 |           "command": "commit-message.history-next"
1625 |         },
1626 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1627 |       },
1628 |       {
1629 |         "key": "ctrl+alt+s",
1630 |         "command": "magit.invoke",
1631 |         "args": {
1632 |           "command": "commit-message.add-signoff"
1633 |         },
1634 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1635 |       },
1636 |       {
1637 |         "key": "f5",
1638 |         "command": "magit.invoke",
1639 |         "args": {
1640 |           "command": "common.refresh"
1641 |         },
1642 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1643 |       },
1644 |       {
1645 |         "key": "g",
1646 |         "command": "magit.invoke",
1647 |         "args": {
1648 |           "command": "common.refresh"
1649 |         },
1650 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1651 |       },
1652 |       {
1653 |         "key": "tab",
1654 |         "command": "magit.invoke",
1655 |         "args": {
1656 |           "command": "common.toggle-section"
1657 |         },
1658 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1659 |       },
1660 |       {
1661 |         "key": "alt+1",
1662 |         "command": "magit.invoke",
1663 |         "args": {
1664 |           "command": "common.detail-level-1"
1665 |         },
1666 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1667 |       },
1668 |       {
1669 |         "key": "alt+2",
1670 |         "command": "magit.invoke",
1671 |         "args": {
1672 |           "command": "common.detail-level-2"
1673 |         },
1674 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1675 |       },
1676 |       {
1677 |         "key": "alt+3",
1678 |         "command": "magit.invoke",
1679 |         "args": {
1680 |           "command": "common.detail-level-3"
1681 |         },
1682 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1683 |       },
1684 |       {
1685 |         "key": "alt+4",
1686 |         "command": "magit.invoke",
1687 |         "args": {
1688 |           "command": "common.detail-level-4"
1689 |         },
1690 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1691 |       },
1692 |       {
1693 |         "key": "n",
1694 |         "command": "magit.invoke",
1695 |         "args": {
1696 |           "command": "common.next-section"
1697 |         },
1698 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1699 |       },
1700 |       {
1701 |         "key": "p",
1702 |         "command": "magit.invoke",
1703 |         "args": {
1704 |           "command": "common.prev-section"
1705 |         },
1706 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1707 |       },
1708 |       {
1709 |         "key": "escape",
1710 |         "command": "magit.invoke",
1711 |         "args": {
1712 |           "command": "common.cancel"
1713 |         },
1714 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1715 |       },
1716 |       {
1717 |         "key": "ctrl+c",
1718 |         "command": "magit.invoke",
1719 |         "args": {
1720 |           "command": "common.cancel"
1721 |         },
1722 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1723 |       },
1724 |       {
1725 |         "key": "enter",
1726 |         "command": "magit.invoke",
1727 |         "args": {
1728 |           "command": "common.confirm"
1729 |         },
1730 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1731 |       },
1732 |       {
1733 |         "key": "q",
1734 |         "command": "magit.invoke",
1735 |         "args": {
1736 |           "command": "common.quit"
1737 |         },
1738 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1739 |       },
1740 |       {
1741 |         "key": "shift+tab",
1742 |         "command": "magit.invoke",
1743 |         "args": {
1744 |           "command": "common.cycle-visibility"
1745 |         },
1746 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1747 |       },
1748 |       {
1749 |         "key": "alt+down",
1750 |         "command": "magit.invoke",
1751 |         "args": {
1752 |           "command": "common.next-sibling"
1753 |         },
1754 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1755 |       },
1756 |       {
1757 |         "key": "alt+up",
1758 |         "command": "magit.invoke",
1759 |         "args": {
1760 |           "command": "common.prev-sibling"
1761 |         },
1762 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1763 |       },
1764 |       {
1765 |         "key": "alt+left",
1766 |         "command": "magit.invoke",
1767 |         "args": {
1768 |           "command": "common.parent-section"
1769 |         },
1770 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1771 |       },
1772 |       {
1773 |         "key": "ctrl+t",
1774 |         "command": "magit.invoke",
1775 |         "args": {
1776 |           "command": "common.toggle-text-mode"
1777 |         },
1778 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1779 |       },
1780 |       {
1781 |         "key": "ctrl+t",
1782 |         "command": "magit.invoke",
1783 |         "args": {
1784 |           "command": "common.toggle-text-mode"
1785 |         },
1786 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && magit.textMode"
1787 |       },
1788 |       {
1789 |         "key": "ctrl+shift+c",
1790 |         "command": "magit.invoke",
1791 |         "args": {
1792 |           "command": "common.copy-value"
1793 |         },
1794 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1795 |       },
1796 |       {
1797 |         "key": "shift+/",
1798 |         "command": "magit.invoke",
1799 |         "args": {
1800 |           "command": "open.dispatch"
1801 |         },
1802 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1803 |       },
1804 |       {
1805 |         "key": "c",
1806 |         "command": "magit.invoke",
1807 |         "args": {
1808 |           "command": "process-log.clear"
1809 |         },
1810 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1811 |       }
1812 |     ],
1813 |     "configuration": {
1814 |       "title": "magit-lsp",
1815 |       "properties": {
1816 |         "magit-lsp.serverPath": {
1817 |           "type": "string",
1818 |           "default": "magit-lsp",
1819 |           "description": "Path to the magit-lsp executable."
1820 |         },
1821 |         "magit-lsp.profile": {
1822 |           "type": "string",
1823 |           "enum": [
1824 |             "standard",
1825 |             "vim"
1826 |           ],
1827 |           "default": "standard",
1828 |           "description": "Keymap profile the server resolves."
1829 |         },
1830 |         "magit-lsp.keybindingsPath": {
1831 |           "type": "string",
1832 |           "default": "",
1833 |           "description": "Optional path to VSCode's keybindings.json for fold-shortcut mirroring."
1834 |         }
1835 |       }
1836 |     }
1837 |   }
1838 | }
```

