## D. Duplicated runtime tables

Commands: `nl -ba lua/magit-lsp/keys.lua`; `nl -ba lua/magit-lsp/highlights.lua`; `nl -ba lua/magit-lsp/prompt.lua`; `nl -ba lua/magit-lsp/config.lua`; `nl -ba lua/magit-lsp/effects.lua`; `nl -ba clients/vscode/lib/menu.js`; `nl -ba clients/vscode/lib/foldkeys.js`; `nl -ba clients/vscode/lib/semantic.js`; `nl -ba clients/vscode/package.json`; `nl -ba clients/vscode/lib/prompt.js`; `nl -ba clients/vscode/lib/requests.js`; `nl -ba clients/vscode/lib/effects.js`; `nl -ba clients/vscode/lib/session.js`; `nl -ba clients/vscode/scripts/gen-keybindings.mjs`. Excerpts below retain exact source text and line numbers.

### Key translation / delivery

Neovim canonical↔lhs translator is `keys.lua` (320 lines; full public implementation excerpt, including all named-key/modifier translation helpers):

```lua
lua/magit-lsp/keys.lua:1: --- Translate between the server's canonical key notation (TASKS.md §3.8) and
lua/magit-lsp/keys.lua:2: --- Neovim's key notation. Pure functions: no buffer or editor state.
lua/magit-lsp/keys.lua:3: ---
lua/magit-lsp/keys.lua:4: --- The canonical form is `[modifier+]*key` with space-separated chord steps.
lua/magit-lsp/keys.lua:5: --- Neovim's form is a single `lhs` string, e.g. `ctrl+c` → `<C-c>` and
lua/magit-lsp/keys.lua:6: --- `g r` → `gr`.
lua/magit-lsp/keys.lua:7:
lua/magit-lsp/keys.lua:8: local M = {}
lua/magit-lsp/keys.lua:9:
lua/magit-lsp/keys.lua:10: --- Canonical named keys and their Neovim spellings.
lua/magit-lsp/keys.lua:11: local NAMED = {
lua/magit-lsp/keys.lua:12:   Enter = 'CR',
lua/magit-lsp/keys.lua:13:   Escape = 'Esc',
lua/magit-lsp/keys.lua:14:   Tab = 'Tab',
lua/magit-lsp/keys.lua:15:   Backspace = 'BS',
lua/magit-lsp/keys.lua:16:   Delete = 'Del',
lua/magit-lsp/keys.lua:17:   Insert = 'Insert',
lua/magit-lsp/keys.lua:18:   Home = 'Home',
lua/magit-lsp/keys.lua:19:   End = 'End',
lua/magit-lsp/keys.lua:20:   PageUp = 'PageUp',
lua/magit-lsp/keys.lua:21:   PageDown = 'PageDown',
lua/magit-lsp/keys.lua:22:   ArrowUp = 'Up',
lua/magit-lsp/keys.lua:23:   ArrowDown = 'Down',
lua/magit-lsp/keys.lua:24:   ArrowLeft = 'Left',
lua/magit-lsp/keys.lua:25:   ArrowRight = 'Right',
lua/magit-lsp/keys.lua:26: }
lua/magit-lsp/keys.lua:27:
lua/magit-lsp/keys.lua:28: for index = 1, 12 do
lua/magit-lsp/keys.lua:29:   NAMED['F' .. index] = 'F' .. index
lua/magit-lsp/keys.lua:30: end
lua/magit-lsp/keys.lua:31:
lua/magit-lsp/keys.lua:32: local NAMED_FROM_NVIM = {}
lua/magit-lsp/keys.lua:33: for canonical, nvim in pairs(NAMED) do
lua/magit-lsp/keys.lua:34:   NAMED_FROM_NVIM[nvim] = canonical
lua/magit-lsp/keys.lua:35: end
lua/magit-lsp/keys.lua:36:
lua/magit-lsp/keys.lua:37: --- Canonical modifier order (§3.8) and the Neovim letter for each.
lua/magit-lsp/keys.lua:38: local MODIFIER_ORDER = { 'ctrl', 'alt', 'shift', 'meta' }
lua/magit-lsp/keys.lua:39: local MODIFIER_LETTER = { ctrl = 'C', alt = 'A', shift = 'S', meta = 'M' }
lua/magit-lsp/keys.lua:40: local LETTER_MODIFIER = { C = 'ctrl', A = 'alt', S = 'shift', M = 'meta' }
lua/magit-lsp/keys.lua:41: local WORD_MODIFIER = {
lua/magit-lsp/keys.lua:42:   ctrl = 'ctrl',
lua/magit-lsp/keys.lua:43:   control = 'ctrl',
lua/magit-lsp/keys.lua:44:   alt = 'alt',
lua/magit-lsp/keys.lua:45:   shift = 'shift',
lua/magit-lsp/keys.lua:46:   meta = 'meta',
lua/magit-lsp/keys.lua:47: }
lua/magit-lsp/keys.lua:48:
lua/magit-lsp/keys.lua:49: local function fail(message)
lua/magit-lsp/keys.lua:50:   return nil, message
lua/magit-lsp/keys.lua:51: end
lua/magit-lsp/keys.lua:52:
lua/magit-lsp/keys.lua:53: --- The single character of `text`, or nil when it is not one character.
lua/magit-lsp/keys.lua:54: local function one_char(text)
lua/magit-lsp/keys.lua:55:   if vim.fn.strchars(text) ~= 1 then
lua/magit-lsp/keys.lua:56:     return nil
lua/magit-lsp/keys.lua:57:   end
lua/magit-lsp/keys.lua:58:   return vim.fn.strcharpart(text, 0, 1)
lua/magit-lsp/keys.lua:59: end
lua/magit-lsp/keys.lua:60:
lua/magit-lsp/keys.lua:61: local function split_ws(text)
lua/magit-lsp/keys.lua:62:   local out = {}
lua/magit-lsp/keys.lua:63:   for word in text:gmatch('%S+') do
lua/magit-lsp/keys.lua:64:     out[#out + 1] = word
lua/magit-lsp/keys.lua:65:   end
lua/magit-lsp/keys.lua:66:   return out
lua/magit-lsp/keys.lua:67: end
lua/magit-lsp/keys.lua:68:
lua/magit-lsp/keys.lua:69: --- Parse one canonical step into `{ ctrl, alt, shift, meta, name|char }`.
lua/magit-lsp/keys.lua:70: local function parse_step(text)
lua/magit-lsp/keys.lua:71:   if text == '' then
lua/magit-lsp/keys.lua:72:     return fail('empty key')
lua/magit-lsp/keys.lua:73:   end
lua/magit-lsp/keys.lua:74:   if text == 'Plus' then
lua/magit-lsp/keys.lua:75:     return { key = { char = '+' } }
lua/magit-lsp/keys.lua:76:   end
lua/magit-lsp/keys.lua:77:
lua/magit-lsp/keys.lua:78:   local parts = {}
lua/magit-lsp/keys.lua:79:   for part in (text .. '+'):gmatch('([^+]+)') do
lua/magit-lsp/keys.lua:80:     parts[#parts + 1] = part
lua/magit-lsp/keys.lua:81:   end
lua/magit-lsp/keys.lua:82:   if #parts == 0 then
lua/magit-lsp/keys.lua:83:     return fail('empty key')
lua/magit-lsp/keys.lua:84:   end
lua/magit-lsp/keys.lua:85:
lua/magit-lsp/keys.lua:86:   local step = {}
lua/magit-lsp/keys.lua:87:   for index = 1, #parts - 1 do
lua/magit-lsp/keys.lua:88:     local modifier = parts[index]
lua/magit-lsp/keys.lua:89:     if modifier == '' then
lua/magit-lsp/keys.lua:90:       return fail('empty modifier in key')
lua/magit-lsp/keys.lua:91:     end
lua/magit-lsp/keys.lua:92:     local name = WORD_MODIFIER[modifier]
lua/magit-lsp/keys.lua:93:     if not name then
lua/magit-lsp/keys.lua:94:       return fail('unknown modifier: ' .. modifier)
lua/magit-lsp/keys.lua:95:     end
lua/magit-lsp/keys.lua:96:     if step[name] then
lua/magit-lsp/keys.lua:97:       return fail('duplicate modifier: ' .. modifier)
lua/magit-lsp/keys.lua:98:     end
lua/magit-lsp/keys.lua:99:     step[name] = true
lua/magit-lsp/keys.lua:100:   end
lua/magit-lsp/keys.lua:101:
lua/magit-lsp/keys.lua:102:   local key = parts[#parts]
lua/magit-lsp/keys.lua:103:   if key == 'Space' then
lua/magit-lsp/keys.lua:104:     step.key = { char = ' ' }
lua/magit-lsp/keys.lua:105:   elseif key == 'Plus' then
lua/magit-lsp/keys.lua:106:     step.key = { char = '+' }
lua/magit-lsp/keys.lua:107:   elseif NAMED[key] then
lua/magit-lsp/keys.lua:108:     step.key = { name = key }
lua/magit-lsp/keys.lua:109:   else
lua/magit-lsp/keys.lua:110:     local char = one_char(key)
lua/magit-lsp/keys.lua:111:     if not char then
lua/magit-lsp/keys.lua:112:       return fail('unknown key: ' .. key)
lua/magit-lsp/keys.lua:113:     end
lua/magit-lsp/keys.lua:114:     if char == '+' then
lua/magit-lsp/keys.lua:115:       return fail('write the plus key as `Plus`')
lua/magit-lsp/keys.lua:116:     end
lua/magit-lsp/keys.lua:117:     if char:match('%s') then
lua/magit-lsp/keys.lua:118:       return fail('write the space key as `Space`')
lua/magit-lsp/keys.lua:119:     end
lua/magit-lsp/keys.lua:120:     if step.shift and not (step.ctrl or step.alt or step.meta) then
lua/magit-lsp/keys.lua:121:       return fail('`shift+' .. char .. '` is invalid; write the shifted character instead')
lua/magit-lsp/keys.lua:122:     end
lua/magit-lsp/keys.lua:123:     step.key = { char = char }
lua/magit-lsp/keys.lua:124:   end
lua/magit-lsp/keys.lua:125:
lua/magit-lsp/keys.lua:126:   return step
lua/magit-lsp/keys.lua:127: end
lua/magit-lsp/keys.lua:128:
lua/magit-lsp/keys.lua:129: --- The Neovim spelling of one parsed step.
lua/magit-lsp/keys.lua:130: local function step_to_lhs(step)
lua/magit-lsp/keys.lua:131:   local letters = {}
lua/magit-lsp/keys.lua:132:   for _, modifier in ipairs(MODIFIER_ORDER) do
lua/magit-lsp/keys.lua:133:     if step[modifier] then
lua/magit-lsp/keys.lua:134:       letters[#letters + 1] = MODIFIER_LETTER[modifier]
lua/magit-lsp/keys.lua:135:     end
lua/magit-lsp/keys.lua:136:   end
lua/magit-lsp/keys.lua:137:
lua/magit-lsp/keys.lua:138:   local key = step.key
lua/magit-lsp/keys.lua:139:   local name
lua/magit-lsp/keys.lua:140:   if key.name then
lua/magit-lsp/keys.lua:141:     name = NAMED[key.name]
lua/magit-lsp/keys.lua:142:     if not name then
lua/magit-lsp/keys.lua:143:       return fail('unknown key: ' .. key.name)
lua/magit-lsp/keys.lua:144:     end
lua/magit-lsp/keys.lua:145:   elseif key.char == ' ' then
lua/magit-lsp/keys.lua:146:     name = 'Space'
lua/magit-lsp/keys.lua:147:   elseif key.char == '+' then
lua/magit-lsp/keys.lua:148:     name = '+'
lua/magit-lsp/keys.lua:149:   elseif key.char == '<' then
lua/magit-lsp/keys.lua:150:     name = 'lt'
lua/magit-lsp/keys.lua:151:   else
lua/magit-lsp/keys.lua:152:     name = key.char
lua/magit-lsp/keys.lua:153:   end
lua/magit-lsp/keys.lua:154:
lua/magit-lsp/keys.lua:155:   if #letters == 0 then
lua/magit-lsp/keys.lua:156:     if key.name then
lua/magit-lsp/keys.lua:157:       return '<' .. name .. '>'
lua/magit-lsp/keys.lua:158:     end
lua/magit-lsp/keys.lua:159:     if key.char == ' ' then
lua/magit-lsp/keys.lua:160:       return '<Space>'
lua/magit-lsp/keys.lua:161:     end
lua/magit-lsp/keys.lua:162:     if key.char == '<' then
lua/magit-lsp/keys.lua:163:       return '<lt>'
lua/magit-lsp/keys.lua:164:     end
lua/magit-lsp/keys.lua:165:     return key.char
lua/magit-lsp/keys.lua:166:   end
lua/magit-lsp/keys.lua:167:
lua/magit-lsp/keys.lua:168:   return '<' .. table.concat(letters, '-') .. '-' .. name .. '>'
lua/magit-lsp/keys.lua:169: end
lua/magit-lsp/keys.lua:170:
lua/magit-lsp/keys.lua:171: --- Split a canonical chord into its server-provided step strings.
lua/magit-lsp/keys.lua:172: ---@param chord string
lua/magit-lsp/keys.lua:173: ---@return string[]|nil
lua/magit-lsp/keys.lua:174: function M.steps(chord)
lua/magit-lsp/keys.lua:175:   if type(chord) ~= 'string' then
lua/magit-lsp/keys.lua:176:     return nil
lua/magit-lsp/keys.lua:177:   end
lua/magit-lsp/keys.lua:178:   local steps = split_ws(chord)
lua/magit-lsp/keys.lua:179:   if #steps == 0 then
lua/magit-lsp/keys.lua:180:     return nil
lua/magit-lsp/keys.lua:181:   end
lua/magit-lsp/keys.lua:182:   return steps
lua/magit-lsp/keys.lua:183: end
lua/magit-lsp/keys.lua:184:
lua/magit-lsp/keys.lua:185: --- Translate a canonical chord to one Neovim `lhs`.
lua/magit-lsp/keys.lua:186: ---@param chord string
lua/magit-lsp/keys.lua:187: ---@return string|nil
lua/magit-lsp/keys.lua:188: function M.to_lhs(chord)
lua/magit-lsp/keys.lua:189:   local steps, err = M.steps(chord)
lua/magit-lsp/keys.lua:190:   if not steps then
lua/magit-lsp/keys.lua:191:     return nil, err
lua/magit-lsp/keys.lua:192:   end
lua/magit-lsp/keys.lua:193:   local out = {}
lua/magit-lsp/keys.lua:194:   for _, text in ipairs(steps) do
lua/magit-lsp/keys.lua:195:     local step = parse_step(text)
lua/magit-lsp/keys.lua:196:     local lhs, step_err = step_to_lhs(step)
lua/magit-lsp/keys.lua:197:     if not lhs then
lua/magit-lsp/keys.lua:198:       return nil, step_err
lua/magit-lsp/keys.lua:199:     end
lua/magit-lsp/keys.lua:200:     out[#out + 1] = lhs
lua/magit-lsp/keys.lua:201:   end
lua/magit-lsp/keys.lua:202:   return table.concat(out), steps
lua/magit-lsp/keys.lua:203: end
lua/magit-lsp/keys.lua:204:
lua/magit-lsp/keys.lua:205: --- Turn a Neovim special-key body such as `C-S-c` or `CR` into canonical.
lua/magit-lsp/keys.lua:206: local function special_to_canonical(special)
lua/magit-lsp/keys.lua:207:   local parts = {}
lua/magit-lsp/keys.lua:208:   for part in (special .. '-'):gmatch('([^-]*)-') do
lua/magit-lsp/keys.lua:209:     parts[#parts + 1] = part
lua/magit-lsp/keys.lua:210:   end
lua/magit-lsp/keys.lua:211:   if #parts == 0 then
lua/magit-lsp/keys.lua:212:     return fail('empty key')
lua/magit-lsp/keys.lua:213:   end
lua/magit-lsp/keys.lua:214:
lua/magit-lsp/keys.lua:215:   local key = parts[#parts]
lua/magit-lsp/keys.lua:216:   local modifiers = {}
lua/magit-lsp/keys.lua:217:   for index = 1, #parts - 1 do
lua/magit-lsp/keys.lua:218:     local part = parts[index]
lua/magit-lsp/keys.lua:219:     local modifier = LETTER_MODIFIER[part] or WORD_MODIFIER[part:lower()]
lua/magit-lsp/keys.lua:220:     if not modifier then
lua/magit-lsp/keys.lua:221:       return fail('unknown modifier: ' .. part)
lua/magit-lsp/keys.lua:222:     end
lua/magit-lsp/keys.lua:223:     if modifiers[modifier] then
lua/magit-lsp/keys.lua:224:       return fail('duplicate modifier: ' .. part)
lua/magit-lsp/keys.lua:225:     end
lua/magit-lsp/keys.lua:226:     modifiers[modifier] = true
lua/magit-lsp/keys.lua:227:   end
lua/magit-lsp/keys.lua:228:
lua/magit-lsp/keys.lua:229:   local canonical_key
lua/magit-lsp/keys.lua:230:   if key == 'lt' then
lua/magit-lsp/keys.lua:231:     canonical_key = '<'
lua/magit-lsp/keys.lua:232:   elseif key == 'Space' then
lua/magit-lsp/keys.lua:233:     canonical_key = 'Space'
lua/magit-lsp/keys.lua:234:   elseif key == 'Plus' then
lua/magit-lsp/keys.lua:235:     canonical_key = 'Plus'
lua/magit-lsp/keys.lua:236:   elseif NAMED_FROM_NVIM[key] then
lua/magit-lsp/keys.lua:237:     canonical_key = NAMED_FROM_NVIM[key]
lua/magit-lsp/keys.lua:238:   else
lua/magit-lsp/keys.lua:239:     local char = one_char(key)
lua/magit-lsp/keys.lua:240:     if not char then
lua/magit-lsp/keys.lua:241:       return fail('unknown key: ' .. key)
lua/magit-lsp/keys.lua:242:     end
lua/magit-lsp/keys.lua:243:     if char == '+' then
lua/magit-lsp/keys.lua:244:       canonical_key = 'Plus'
lua/magit-lsp/keys.lua:245:     elseif char:match('%s') then
lua/magit-lsp/keys.lua:246:       canonical_key = 'Space'
lua/magit-lsp/keys.lua:247:     else
lua/magit-lsp/keys.lua:248:       canonical_key = char
lua/magit-lsp/keys.lua:249:     end
lua/magit-lsp/keys.lua:250:   end
lua/magit-lsp/keys.lua:251:
lua/magit-lsp/keys.lua:252:   if type(canonical_key) ~= 'string' then
lua/magit-lsp/keys.lua:253:     return nil, canonical_key
lua/magit-lsp/keys.lua:254:   end
lua/magit-lsp/keys.lua:255:
lua/magit-lsp/keys.lua:256:   if canonical_key == '<' or canonical_key == 'Space' or canonical_key == 'Plus' or NAMED[canonical_key] then
lua/magit-lsp/keys.lua:257:     -- Named keys and the two spelled-out characters.
lua/magit-lsp/keys.lua:258:   elseif modifiers.shift and not (modifiers.ctrl or modifiers.alt or modifiers.meta) then
lua/magit-lsp/keys.lua:259:     return fail('shifted character keys have no `shift+` form')
lua/magit-lsp/keys.lua:260:   end
lua/magit-lsp/keys.lua:261:
lua/magit-lsp/keys.lua:262:   local out = {}
lua/magit-lsp/keys.lua:263:   for _, modifier in ipairs(MODIFIER_ORDER) do
lua/magit-lsp/keys.lua:264:     if modifiers[modifier] then
lua/magit-lsp/keys.lua:265:       out[#out + 1] = modifier
lua/magit-lsp/keys.lua:266:     end
lua/magit-lsp/keys.lua:267:   end
lua/magit-lsp/keys.lua:268:   out[#out + 1] = canonical_key
lua/magit-lsp/keys.lua:269:   return table.concat(out, '+')
lua/magit-lsp/keys.lua:270: end
lua/magit-lsp/keys.lua:271:
lua/magit-lsp/keys.lua:272: --- Translate one Neovim `lhs` to a canonical chord.
lua/magit-lsp/keys.lua:273: ---@param lhs string
lua/magit-lsp/keys.lua:274: ---@return string|nil
lua/magit-lsp/keys.lua:275: function M.from_lhs(lhs)
lua/magit-lsp/keys.lua:276:   if type(lhs) ~= 'string' then
lua/magit-lsp/keys.lua:277:     return nil, 'lhs must be a string'
lua/magit-lsp/keys.lua:278:   end
lua/magit-lsp/keys.lua:279:   if lhs == '' then
lua/magit-lsp/keys.lua:280:     return nil, 'empty lhs'
lua/magit-lsp/keys.lua:281:   end
lua/magit-lsp/keys.lua:282:
lua/magit-lsp/keys.lua:283:   local steps = {}
lua/magit-lsp/keys.lua:284:   local index = 1
lua/magit-lsp/keys.lua:285:   while index <= #lhs do
lua/magit-lsp/keys.lua:286:     local char = lhs:sub(index, index)
lua/magit-lsp/keys.lua:287:     if char == '<' then
lua/magit-lsp/keys.lua:288:       local close = lhs:find('>', index + 1, true)
lua/magit-lsp/keys.lua:289:       if not close then
lua/magit-lsp/keys.lua:290:         return nil, 'unterminated `<` in lhs'
lua/magit-lsp/keys.lua:291:       end
lua/magit-lsp/keys.lua:292:       local canonical, err = special_to_canonical(lhs:sub(index + 1, close - 1))
lua/magit-lsp/keys.lua:293:       if not canonical then
lua/magit-lsp/keys.lua:294:         return nil, err
lua/magit-lsp/keys.lua:295:       end
lua/magit-lsp/keys.lua:296:       steps[#steps + 1] = canonical
lua/magit-lsp/keys.lua:297:       index = close + 1
lua/magit-lsp/keys.lua:298:     elseif char == ' ' then
lua/magit-lsp/keys.lua:299:       steps[#steps + 1] = 'Space'
lua/magit-lsp/keys.lua:300:       index = index + 1
lua/magit-lsp/keys.lua:301:     elseif char == '+' then
lua/magit-lsp/keys.lua:302:       steps[#steps + 1] = 'Plus'
lua/magit-lsp/keys.lua:303:       index = index + 1
lua/magit-lsp/keys.lua:304:     else
lua/magit-lsp/keys.lua:305:       steps[#steps + 1] = char
lua/magit-lsp/keys.lua:306:       index = index + 1
lua/magit-lsp/keys.lua:307:     end
lua/magit-lsp/keys.lua:308:   end
lua/magit-lsp/keys.lua:309:
lua/magit-lsp/keys.lua:310:   return table.concat(steps, ' ')
lua/magit-lsp/keys.lua:311: end
lua/magit-lsp/keys.lua:312:
lua/magit-lsp/keys.lua:313: --- Termcodes for `nvim_feedkeys` with `escape_ks = false`.
lua/magit-lsp/keys.lua:314: ---@param lhs string
lua/magit-lsp/keys.lua:315: ---@return string
lua/magit-lsp/keys.lua:316: function M.termcodes(lhs)
lua/magit-lsp/keys.lua:317:   return vim.api.nvim_replace_termcodes(lhs, true, false, true)
lua/magit-lsp/keys.lua:318: end
lua/magit-lsp/keys.lua:319:
lua/magit-lsp/keys.lua:320: return M
```

VSCode typed-menu translation is intentionally narrower; generated static keybindings and native fold-key conversion are separate:

```js
clients/vscode/lib/menu.js:71: const SPECIAL_CHARS = {
clients/vscode/lib/menu.js:72:   ' ': 'Space',
clients/vscode/lib/menu.js:73:   '+': 'Plus',
clients/vscode/lib/menu.js:74: };
clients/vscode/lib/menu.js:75:
clients/vscode/lib/menu.js:76: /**
clients/vscode/lib/menu.js:77:  * The canonical key step for one typed character (§3.8).
clients/vscode/lib/menu.js:78:  *
clients/vscode/lib/menu.js:79:  * @param {string} character
clients/vscode/lib/menu.js:80:  * @returns {string}
clients/vscode/lib/menu.js:81:  */
clients/vscode/lib/menu.js:82: function charToKey(character) {
clients/vscode/lib/menu.js:83:   return SPECIAL_CHARS[character] || character;
clients/vscode/lib/menu.js:84: }
clients/vscode/lib/menu.js:85:
clients/vscode/lib/menu.js:86: /**
clients/vscode/lib/menu.js:87:  * Turn typed QuickPick input into `view/key` steps.
clients/vscode/lib/menu.js:88:  *
clients/vscode/lib/menu.js:89:  * Menu keys are characters (`c`) or two-step prefixes (`-a`, `=A`), so each
clients/vscode/lib/menu.js:90:  * typed character becomes its own key request and the server keeps the pending
clients/vscode/lib/menu.js:91:  * prefix. The QuickPick input is cleared after forwarding.
clients/vscode/lib/menu.js:92:  *
clients/vscode/lib/menu.js:93:  * @param {string|undefined} value
clients/vscode/lib/menu.js:94:  * @returns {string[]}
clients/vscode/lib/menu.js:95:  */
clients/vscode/lib/menu.js:96: function routeTyped(value) {
clients/vscode/lib/menu.js:97:   if (typeof value !== 'string') {
clients/vscode/lib/menu.js:98:     return [];
clients/vscode/lib/menu.js:99:   }
clients/vscode/lib/menu.js:100:   return Array.from(value).map(charToKey);
clients/vscode/lib/menu.js:101: }
clients/vscode/lib/menu.js:102:
clients/vscode/lib/menu.js:103: module.exports = {
clients/vscode/lib/menu.js:104:   quickPickItems,
clients/vscode/lib/menu.js:105:   quickPickItem,
clients/vscode/lib/menu.js:106:   describeItem,
clients/vscode/lib/menu.js:107:   routeTyped,
clients/vscode/lib/menu.js:108:   charToKey,
clients/vscode/lib/menu.js:109: };
clients/vscode/lib/foldkeys.js:24:  * The equivalent magit command for a VSCode fold command, if there is one.
clients/vscode/lib/foldkeys.js:25:  *
clients/vscode/lib/foldkeys.js:26:  * @param {string} command
clients/vscode/lib/foldkeys.js:27:  * @returns {{command: string, args?: object}|undefined}
clients/vscode/lib/foldkeys.js:28:  */
clients/vscode/lib/foldkeys.js:29: function mapFoldCommand(command) {
clients/vscode/lib/foldkeys.js:30:   if (typeof command !== 'string') {
clients/vscode/lib/foldkeys.js:31:     return undefined;
clients/vscode/lib/foldkeys.js:32:   }
clients/vscode/lib/foldkeys.js:33:   if (FOLD_COMMANDS[command]) {
clients/vscode/lib/foldkeys.js:34:     return FOLD_COMMANDS[command];
clients/vscode/lib/foldkeys.js:35:   }
clients/vscode/lib/foldkeys.js:36:   if (command === 'editor.toggleFold') {
clients/vscode/lib/foldkeys.js:37:     return { command: 'common.toggle-section' };
clients/vscode/lib/foldkeys.js:38:   }
clients/vscode/lib/foldkeys.js:39:   if (command.startsWith('editor.unfold')) {
clients/vscode/lib/foldkeys.js:40:     return { command: 'common.show-section' };
clients/vscode/lib/foldkeys.js:41:   }
clients/vscode/lib/foldkeys.js:42:   if (command.startsWith('editor.fold')) {
clients/vscode/lib/foldkeys.js:43:     return { command: 'common.hide-section' };
clients/vscode/lib/foldkeys.js:44:   }
clients/vscode/lib/foldkeys.js:45:   return undefined;
clients/vscode/lib/foldkeys.js:57: function mapFoldKeys(input) {
clients/vscode/lib/foldkeys.js:58:   const entries = typeof input === 'string' ? parseJsonc(input) : input;
clients/vscode/lib/foldkeys.js:59:   if (!Array.isArray(entries)) {
clients/vscode/lib/foldkeys.js:60:     return [];
clients/vscode/lib/foldkeys.js:61:   }
clients/vscode/lib/foldkeys.js:62:
clients/vscode/lib/foldkeys.js:63:   const mapped = [];
clients/vscode/lib/foldkeys.js:64:   for (const entry of entries) {
clients/vscode/lib/foldkeys.js:65:     if (!entry || typeof entry !== 'object') {
clients/vscode/lib/foldkeys.js:66:       continue;
clients/vscode/lib/foldkeys.js:67:     }
clients/vscode/lib/foldkeys.js:68:     const target = mapFoldCommand(entry.command);
clients/vscode/lib/foldkeys.js:69:     if (!target) {
clients/vscode/lib/foldkeys.js:70:       continue;
clients/vscode/lib/foldkeys.js:71:     }
clients/vscode/lib/foldkeys.js:72:
clients/vscode/lib/foldkeys.js:73:     const result = {
clients/vscode/lib/foldkeys.js:74:       key: entry.key,
clients/vscode/lib/foldkeys.js:75:       command: 'magit.invoke',
clients/vscode/lib/foldkeys.js:76:       args: { command: target.command },
clients/vscode/lib/foldkeys.js:77:       when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
clients/vscode/lib/foldkeys.js:78:     };
clients/vscode/lib/foldkeys.js:79:     if (target.args) {
clients/vscode/lib/foldkeys.js:80:       result.args.args = target.args;
clients/vscode/lib/foldkeys.js:81:     }
clients/vscode/lib/foldkeys.js:82:     // Keep platform-specific key overrides as VSCode's own schema expects.
clients/vscode/lib/foldkeys.js:83:     for (const platform of ['mac', 'linux', 'win']) {
clients/vscode/lib/foldkeys.js:84:       if (typeof entry[platform] === 'string') {
clients/vscode/lib/foldkeys.js:85:         result[platform] = entry[platform];
clients/vscode/lib/foldkeys.js:86:       }
clients/vscode/lib/foldkeys.js:87:     }
clients/vscode/lib/foldkeys.js:88:     mapped.push(result);
clients/vscode/lib/foldkeys.js:89:   }
clients/vscode/lib/foldkeys.js:90:   return mapped;
clients/vscode/lib/foldkeys.js:91: }
clients/vscode/lib/foldkeys.js:92:
clients/vscode/lib/foldkeys.js:93: const FOLD_COMMANDS = {
clients/vscode/lib/foldkeys.js:94:   'editor.fold': { command: 'common.hide-section' },
clients/vscode/lib/foldkeys.js:95:   'editor.foldRecursively': { command: 'common.hide-section', args: { recursive: true } },
clients/vscode/lib/foldkeys.js:96:   'editor.foldAll': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:97:   'editor.foldAllBlockComment': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:98:   'editor.foldAllMarkerRegions': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:99:   'editor.foldAllExcept': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:100:   'editor.unfold': { command: 'common.show-section' },
clients/vscode/lib/foldkeys.js:101:   'editor.unfoldRecursively': { command: 'common.show-section', args: { recursive: true } },
clients/vscode/lib/foldkeys.js:102:   'editor.unfoldAll': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:103:   'editor.unfoldAllMarkerRegions': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:104:   'editor.unfoldAllExcept': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:105:   'editor.toggleFold': { command: 'common.toggle-section' },
clients/vscode/lib/foldkeys.js:106: };
clients/vscode/lib/foldkeys.js:107:
clients/vscode/lib/foldkeys.js:108: for (let level = 1; level <= 7; level += 1) {
clients/vscode/lib/foldkeys.js:109:   FOLD_COMMANDS[`editor.foldLevel${level}`] = { command: 'common.less-detail' };
clients/vscode/lib/foldkeys.js:110: }
clients/vscode/lib/foldkeys.js:111:
clients/vscode/lib/foldkeys.js:112: module.exports = {
clients/vscode/lib/foldkeys.js:113:   parseJsonc,
clients/vscode/lib/foldkeys.js:114:   mapFoldCommand,
clients/vscode/lib/foldkeys.js:115:   mapFoldKeys,
clients/vscode/lib/foldkeys.js:116: };
clients/vscode/scripts/gen-keybindings.mjs:32: const server = serverCommand();
clients/vscode/scripts/gen-keybindings.mjs:33: const output = execFileSync(
clients/vscode/scripts/gen-keybindings.mjs:34:   server.command,
clients/vscode/scripts/gen-keybindings.mjs:35:   [...server.args, 'keymap', '--format', 'vscode', '--profile', 'standard'],
clients/vscode/scripts/gen-keybindings.mjs:36:   { cwd: repoRoot, encoding: 'utf8' }
clients/vscode/scripts/gen-keybindings.mjs:37: );
clients/vscode/scripts/gen-keybindings.mjs:38:
clients/vscode/scripts/gen-keybindings.mjs:39: const keybindings = JSON.parse(output);
clients/vscode/scripts/gen-keybindings.mjs:40: const pkg = JSON.parse(readFileSync(manifestPath, 'utf8'));
clients/vscode/scripts/gen-keybindings.mjs:41: pkg.contributes = pkg.contributes || {};
clients/vscode/scripts/gen-keybindings.mjs:42:
clients/vscode/scripts/gen-keybindings.mjs:43: const current = JSON.stringify(pkg.contributes.keybindings || []);
clients/vscode/scripts/gen-keybindings.mjs:44: const next = JSON.stringify(keybindings);
clients/vscode/scripts/gen-keybindings.mjs:45:
clients/vscode/scripts/gen-keybindings.mjs:46: if (checkOnly) {
clients/vscode/scripts/gen-keybindings.mjs:47:   if (current !== next) {
clients/vscode/scripts/gen-keybindings.mjs:48:     console.error('contributes.keybindings is stale; run node clients/vscode/scripts/gen-keybindings.mjs');
clients/vscode/scripts/gen-keybindings.mjs:49:     process.exit(1);
clients/vscode/scripts/gen-keybindings.mjs:50:   }
clients/vscode/scripts/gen-keybindings.mjs:51:   console.log(`ok ${keybindings.length} keybindings`);
clients/vscode/scripts/gen-keybindings.mjs:52:   process.exit(0);
clients/vscode/scripts/gen-keybindings.mjs:53: }
clients/vscode/scripts/gen-keybindings.mjs:54:
clients/vscode/scripts/gen-keybindings.mjs:55: pkg.contributes.keybindings = keybindings;
clients/vscode/scripts/gen-keybindings.mjs:56: writeFileSync(manifestPath, `${JSON.stringify(pkg, null, 2)}\n`);
```

Comparison: Neovim implements canonical chord parse/render (`keys.lua`); VSCode's menu maps only typed space/plus and other characters, static bindings are generated from the server manifest, and fold keybindings preserve VSCode `entry.key`. The matching literal special names `Space`/`Plus` are identical where used; the translator tables/functions are not identical and do not have equivalent scope. No client-wide equivalence asserted.

### Token/theme maps

```lua
lua/magit-lsp/highlights.lua:10: --- Client-only overrides, token type to theme group.
lua/magit-lsp/highlights.lua:11: ---
lua/magit-lsp/highlights.lua:12: --- The server's suggestion wins for every type absent here, so this table is
lua/magit-lsp/highlights.lua:13: --- only for local preferences or a temporarily bad server suggestion.
lua/magit-lsp/highlights.lua:14: M.OVERRIDES = {}
lua/magit-lsp/highlights.lua:15:
lua/magit-lsp/highlights.lua:16: --- Modifier groups are applied above the type, so they should only add
lua/magit-lsp/highlights.lua:17: --- attributes (or a background) and never replace a type's foreground.
lua/magit-lsp/highlights.lua:18: M.MODIFIER_LINKS = {
lua/magit-lsp/highlights.lua:19:   -- keep-for: TASKS §3.15 recorded modifier styling; selected is used by log selection, remaining links support planned/ref presentation (R-5.3).
lua/magit-lsp/highlights.lua:20:   staged = 'Bold',
lua/magit-lsp/highlights.lua:21:   selected = 'Visual',
lua/magit-lsp/highlights.lua:22:   planned = 'Comment',
lua/magit-lsp/highlights.lua:23:   disabled = 'Comment',
lua/magit-lsp/highlights.lua:24:   bold = 'Bold',
lua/magit-lsp/highlights.lua:25: }
lua/magit-lsp/highlights.lua:26:
lua/magit-lsp/highlights.lua:27: --- The legend of the most recently initialized client, kept for `:colorscheme`.
lua/magit-lsp/highlights.lua:28: ---@type table|nil
lua/magit-lsp/highlights.lua:29: M.legend = nil
```

```js
clients/vscode/lib/semantic.js:1: 'use strict';
clients/vscode/lib/semantic.js:2:
clients/vscode/lib/semantic.js:3: /** Register theme-addressable tokens using the server legend and ranges verbatim. */
clients/vscode/lib/semantic.js:4: function register(vscode, client, stateForUri, subscriptions) {
clients/vscode/lib/semantic.js:5:   const serverLegend = client.initializeResult.capabilities.semanticTokensProvider.legend;
clients/vscode/lib/semantic.js:6:   const legend = new vscode.SemanticTokensLegend(
clients/vscode/lib/semantic.js:7:     serverLegend.tokenTypes.map((type) => type.replaceAll('.', '-')),
clients/vscode/lib/semantic.js:8:     serverLegend.tokenModifiers
clients/vscode/lib/semantic.js:9:   );
clients/vscode/lib/semantic.js:10:   const changed = new vscode.EventEmitter();
clients/vscode/lib/semantic.js:11:   subscriptions.push(changed, vscode.languages.registerDocumentSemanticTokensProvider(
clients/vscode/lib/semantic.js:12:     { scheme: 'magit', language: 'magit' },
clients/vscode/lib/semantic.js:13:     {
clients/vscode/lib/semantic.js:14:       onDidChangeSemanticTokens: changed.event,
clients/vscode/lib/semantic.js:15:       async provideDocumentSemanticTokens(document) {
clients/vscode/lib/semantic.js:16:         const state = stateForUri(document.uri);
clients/vscode/lib/semantic.js:17:         if (!state) return undefined;
clients/vscode/lib/semantic.js:18:         const tokens = await client.sendRequest('textDocument/semanticTokens/full', {
clients/vscode/lib/semantic.js:19:           textDocument: { uri: state.descriptor.uri },
clients/vscode/lib/semantic.js:20:         });
clients/vscode/lib/semantic.js:21:         return tokens && new vscode.SemanticTokens(new Uint32Array(tokens.data), tokens.resultId);
clients/vscode/lib/semantic.js:22:       },
clients/vscode/lib/semantic.js:23:     }, legend
clients/vscode/lib/semantic.js:24:   ));
clients/vscode/lib/semantic.js:25:   return changed;
clients/vscode/lib/semantic.js:26: }
clients/vscode/lib/semantic.js:27:
clients/vscode/lib/semantic.js:28: module.exports = { register };
clients/vscode/package.json:74:     "semanticTokenScopes": [
clients/vscode/package.json:75:       {
clients/vscode/package.json:76:         "language": "magit",
clients/vscode/package.json:77:         "scopes": {
clients/vscode/package.json:78:           "magit-section": [
clients/vscode/package.json:79:             "entity.name.section"
clients/vscode/package.json:80:           ],
clients/vscode/package.json:81:           "magit-count": [
clients/vscode/package.json:82:             "constant.numeric"
clients/vscode/package.json:83:           ],
clients/vscode/package.json:84:           "magit-file": [
clients/vscode/package.json:85:             "string"
clients/vscode/package.json:86:           ],
clients/vscode/package.json:87:           "magit-file-modified": [
clients/vscode/package.json:88:             "markup.changed"
clients/vscode/package.json:89:           ],
clients/vscode/package.json:90:           "magit-file-added": [
clients/vscode/package.json:91:             "markup.inserted"
clients/vscode/package.json:92:           ],
clients/vscode/package.json:93:           "magit-file-deleted": [
clients/vscode/package.json:94:             "markup.deleted"
clients/vscode/package.json:95:           ],
clients/vscode/package.json:96:           "magit-file-renamed": [
clients/vscode/package.json:97:             "markup.changed"
clients/vscode/package.json:98:           ],
clients/vscode/package.json:99:           "magit-file-untracked": [
clients/vscode/package.json:100:             "comment"
clients/vscode/package.json:101:           ],
clients/vscode/package.json:102:           "magit-hunk-header": [
clients/vscode/package.json:103:             "meta.diff.range"
clients/vscode/package.json:104:           ],
clients/vscode/package.json:105:           "magit-diff-added": [
clients/vscode/package.json:106:             "markup.inserted"
clients/vscode/package.json:107:           ],
clients/vscode/package.json:108:           "magit-diff-removed": [
clients/vscode/package.json:109:             "markup.deleted"
clients/vscode/package.json:110:           ],
clients/vscode/package.json:111:           "magit-message-error": [
clients/vscode/package.json:112:             "invalid"
clients/vscode/package.json:113:           ],
clients/vscode/package.json:114:           "magit-message-warn": [
clients/vscode/package.json:115:             "markup.warning"
clients/vscode/package.json:116:           ]
```

Neovim client-local modifier links are explicit; token colors derive from the server legend. VSCode transforms dotted token type identifiers to hyphenated identifiers and contributes theme scopes for the listed types; it passes modifiers through. These are structurally different; byte-for-byte equivalence is not claimed. The scope entries shown are the entire `semanticTokenScopes` mapping in the inspected range (with its literal JSON values).

### Prompt kinds/widgets and answers

```lua
lua/magit-lsp/prompt.lua:50: --- Run a text prompt; `on_value` receives the typed string.
lua/magit-lsp/prompt.lua:51: local function ask_text(client_id, params, on_value)
lua/magit-lsp/prompt.lua:52:   if params.secret then
lua/magit-lsp/prompt.lua:53:     -- `vim.ui.input` cannot mask, so secret answers use the built-in helper.
lua/magit-lsp/prompt.lua:54:     local value = vim.fn.inputsecret(params.title)
lua/magit-lsp/prompt.lua:55:     if value == nil or value == '' then
lua/magit-lsp/prompt.lua:56:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:57:     else
lua/magit-lsp/prompt.lua:58:       on_value(value)
lua/magit-lsp/prompt.lua:59:     end
lua/magit-lsp/prompt.lua:60:     return
lua/magit-lsp/prompt.lua:61:   end
lua/magit-lsp/prompt.lua:62:   local default = type(params.default) == 'string' and params.default or nil
lua/magit-lsp/prompt.lua:63:   vim.ui.input({ prompt = params.title, default = default }, function(input)
lua/magit-lsp/prompt.lua:64:     if input == nil then
lua/magit-lsp/prompt.lua:65:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:66:     else
lua/magit-lsp/prompt.lua:67:       on_value(input)
lua/magit-lsp/prompt.lua:68:     end
lua/magit-lsp/prompt.lua:69:   end)
lua/magit-lsp/prompt.lua:70: end
lua/magit-lsp/prompt.lua:71:
lua/magit-lsp/prompt.lua:72: local function ask_confirm(client_id, params)
lua/magit-lsp/prompt.lua:73:   local yes = params.yesLabel or 'Yes'
lua/magit-lsp/prompt.lua:74:   -- A destructive question focuses "No" first.
lua/magit-lsp/prompt.lua:75:   local no = params.noLabel or 'No'
lua/magit-lsp/prompt.lua:76:   local labels = (params.destructive or params.default == false) and { no, yes }
lua/magit-lsp/prompt.lua:77:     or { yes, no }
lua/magit-lsp/prompt.lua:78:   vim.ui.select(labels, { prompt = params.title }, function(choice)
lua/magit-lsp/prompt.lua:79:     if choice == nil then
lua/magit-lsp/prompt.lua:80:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:81:     else
lua/magit-lsp/prompt.lua:82:       finish(client_id, params, false, choice == yes)
lua/magit-lsp/prompt.lua:83:     end
lua/magit-lsp/prompt.lua:84:   end)
lua/magit-lsp/prompt.lua:85: end
lua/magit-lsp/prompt.lua:86:
lua/magit-lsp/prompt.lua:87: local function ask_choice(client_id, params)
lua/magit-lsp/prompt.lua:88:   local items = {}
lua/magit-lsp/prompt.lua:89:   for _, item in ipairs(params.items or {}) do
lua/magit-lsp/prompt.lua:90:     items[#items + 1] = item
lua/magit-lsp/prompt.lua:91:   end
lua/magit-lsp/prompt.lua:92:   if params.allowCustom then
lua/magit-lsp/prompt.lua:93:     items[#items + 1] = { id = '__custom__', label = 'Enter custom value…' }
lua/magit-lsp/prompt.lua:94:   end
lua/magit-lsp/prompt.lua:95:   vim.ui.select(items, {
lua/magit-lsp/prompt.lua:96:     prompt = params.title,
lua/magit-lsp/prompt.lua:97:     format_item = format_item,
lua/magit-lsp/prompt.lua:98:   }, function(item)
lua/magit-lsp/prompt.lua:99:     if item == nil then
lua/magit-lsp/prompt.lua:100:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:101:     elseif item.id == '__custom__' then
lua/magit-lsp/prompt.lua:102:       ask_text(client_id, params, function(text)
lua/magit-lsp/prompt.lua:103:         finish(client_id, params, false, { custom = text })
lua/magit-lsp/prompt.lua:104:       end)
lua/magit-lsp/prompt.lua:105:     else
lua/magit-lsp/prompt.lua:106:       finish(client_id, params, false, { id = item.id })
lua/magit-lsp/prompt.lua:107:     end
lua/magit-lsp/prompt.lua:108:   end)
lua/magit-lsp/prompt.lua:109: end
lua/magit-lsp/prompt.lua:110:
lua/magit-lsp/prompt.lua:111: local function ask_multi(client_id, params)
lua/magit-lsp/prompt.lua:112:   local selected = {}
lua/magit-lsp/prompt.lua:113:   local order = {}
lua/magit-lsp/prompt.lua:114:   for _, id in ipairs(params.preselected or {}) do
lua/magit-lsp/prompt.lua:115:     if not selected[id] then
lua/magit-lsp/prompt.lua:116:       selected[id] = true
lua/magit-lsp/prompt.lua:117:       order[#order + 1] = id
lua/magit-lsp/prompt.lua:118:     end
lua/magit-lsp/prompt.lua:119:   end
lua/magit-lsp/prompt.lua:120:
lua/magit-lsp/prompt.lua:121:   local function step()
lua/magit-lsp/prompt.lua:122:     local items = {}
lua/magit-lsp/prompt.lua:123:     for _, item in ipairs(params.items or {}) do
lua/magit-lsp/prompt.lua:124:       items[#items + 1] = {
lua/magit-lsp/prompt.lua:125:         id = item.id,
lua/magit-lsp/prompt.lua:126:         label = (selected[item.id] and '[x] ' or '[ ] ') .. item.label,
lua/magit-lsp/prompt.lua:127:         description = item.description,
lua/magit-lsp/prompt.lua:128:         detail = item.detail,
lua/magit-lsp/prompt.lua:129:       }
lua/magit-lsp/prompt.lua:130:     end
lua/magit-lsp/prompt.lua:131:     items[#items + 1] = { id = '__done__', label = 'Done' }
lua/magit-lsp/prompt.lua:132:
lua/magit-lsp/prompt.lua:133:     vim.ui.select(items, {
lua/magit-lsp/prompt.lua:134:       prompt = params.title,
lua/magit-lsp/prompt.lua:135:       format_item = format_item,
lua/magit-lsp/prompt.lua:136:     }, function(item)
lua/magit-lsp/prompt.lua:137:       if item == nil then
lua/magit-lsp/prompt.lua:138:         finish(client_id, params, true)
lua/magit-lsp/prompt.lua:139:       elseif item.id == '__done__' then
lua/magit-lsp/prompt.lua:140:         finish(client_id, params, false, { ids = order })
lua/magit-lsp/prompt.lua:141:       else
lua/magit-lsp/prompt.lua:142:         if selected[item.id] then
lua/magit-lsp/prompt.lua:143:           selected[item.id] = nil
lua/magit-lsp/prompt.lua:144:           for index, id in ipairs(order) do
lua/magit-lsp/prompt.lua:145:             if id == item.id then
lua/magit-lsp/prompt.lua:146:               table.remove(order, index)
lua/magit-lsp/prompt.lua:147:               break
lua/magit-lsp/prompt.lua:148:             end
lua/magit-lsp/prompt.lua:149:           end
lua/magit-lsp/prompt.lua:150:         else
lua/magit-lsp/prompt.lua:151:           selected[item.id] = true
lua/magit-lsp/prompt.lua:152:           order[#order + 1] = item.id
lua/magit-lsp/prompt.lua:153:         end
lua/magit-lsp/prompt.lua:154:         step()
lua/magit-lsp/prompt.lua:155:       end
lua/magit-lsp/prompt.lua:156:     end)
lua/magit-lsp/prompt.lua:157:   end
lua/magit-lsp/prompt.lua:158:
lua/magit-lsp/prompt.lua:159:   step()
lua/magit-lsp/prompt.lua:160: end
lua/magit-lsp/prompt.lua:161:
lua/magit-lsp/prompt.lua:162: --- Show one `view/prompt` notification and answer it.
lua/magit-lsp/prompt.lua:163: ---@param params table `view/prompt` params
lua/magit-lsp/prompt.lua:164: ---@param client_id integer|nil originating LSP client
lua/magit-lsp/prompt.lua:165: function M.handle(params, client_id)
lua/magit-lsp/prompt.lua:166:   if type(params) ~= 'table' or not params.promptId then
lua/magit-lsp/prompt.lua:167:     return
lua/magit-lsp/prompt.lua:168:   end
lua/magit-lsp/prompt.lua:169:   hooks.prompt_started()
lua/magit-lsp/prompt.lua:170:   local kind = params.kind
lua/magit-lsp/prompt.lua:171:   if kind == 'confirm' then
lua/magit-lsp/prompt.lua:172:     ask_confirm(client_id, params)
lua/magit-lsp/prompt.lua:173:   elseif kind == 'text' then
lua/magit-lsp/prompt.lua:174:     ask_text(client_id, params, function(text)
lua/magit-lsp/prompt.lua:175:       finish(client_id, params, false, text)
lua/magit-lsp/prompt.lua:176:     end)
lua/magit-lsp/prompt.lua:177:   elseif kind == 'choice' then
lua/magit-lsp/prompt.lua:178:     ask_choice(client_id, params)
lua/magit-lsp/prompt.lua:179:   elseif kind == 'multiChoice' then
lua/magit-lsp/prompt.lua:180:     ask_multi(client_id, params)
lua/magit-lsp/prompt.lua:181:   else
lua/magit-lsp/prompt.lua:182:     -- An unknown kind cannot be repaired client-side; cancel it.
lua/magit-lsp/prompt.lua:183:     finish(client_id, params, true)
lua/magit-lsp/prompt.lua:184:   end
lua/magit-lsp/prompt.lua:185: end
```

```js
clients/vscode/lib/prompt.js:1: 'use strict';
clients/vscode/lib/prompt.js:2:
clients/vscode/lib/prompt.js:3: /**
clients/vscode/lib/prompt.js:4:  * Pure mapping between `view/prompt` parameters and QuickPick items/answers.
clients/vscode/lib/prompt.js:5:  *
clients/vscode/lib/prompt.js:6:  * The extension owns the actual VSCode widgets; this module only shapes data
clients/vscode/lib/prompt.js:7:  * so the mapping can be tested without an editor.
clients/vscode/lib/prompt.js:8:  */
clients/vscode/lib/prompt.js:9:
clients/vscode/lib/prompt.js:10: /** The id of the synthetic "type a value" row of an `allowCustom` choice. */
clients/vscode/lib/prompt.js:11: const CUSTOM_ID = '__custom__';
clients/vscode/lib/prompt.js:12:
clients/vscode/lib/prompt.js:13: /**
clients/vscode/lib/prompt.js:14:  * The rows a prompt offers, in display order.
clients/vscode/lib/prompt.js:15:  *
clients/vscode/lib/prompt.js:16:  * @param {object} params `view/prompt` params
clients/vscode/lib/prompt.js:17:  * @returns {{id: string, label: string, description?: string, detail?: string}[]}
clients/vscode/lib/prompt.js:18:  */
clients/vscode/lib/prompt.js:19: function promptItems(params) {
clients/vscode/lib/prompt.js:20:   if (!params) {
clients/vscode/lib/prompt.js:21:     return [];
clients/vscode/lib/prompt.js:22:   }
clients/vscode/lib/prompt.js:23:   if (params.kind === 'confirm') {
clients/vscode/lib/prompt.js:24:     const yes = { id: 'yes', label: params.yesLabel || 'Yes' };
clients/vscode/lib/prompt.js:25:     const no = { id: 'no', label: params.noLabel || 'No' };
clients/vscode/lib/prompt.js:26:     // A destructive question focuses "No" first.
clients/vscode/lib/prompt.js:27:     return (params.destructive || params.default === false) ? [no, yes] : [yes, no];
clients/vscode/lib/prompt.js:28:   }
clients/vscode/lib/prompt.js:29:
clients/vscode/lib/prompt.js:30:   const items = (params.items || []).map((item) => ({
clients/vscode/lib/prompt.js:31:     id: item.id,
clients/vscode/lib/prompt.js:32:     label: item.label,
clients/vscode/lib/prompt.js:33:     description: item.description,
clients/vscode/lib/prompt.js:34:     detail: item.detail,
clients/vscode/lib/prompt.js:35:   }));
clients/vscode/lib/prompt.js:36:   if (params.kind === 'choice' && params.allowCustom) {
clients/vscode/lib/prompt.js:37:     items.push({ id: CUSTOM_ID, label: 'Enter custom value…' });
clients/vscode/lib/prompt.js:38:   }
clients/vscode/lib/prompt.js:39:   return items;
clients/vscode/lib/prompt.js:40: }
clients/vscode/lib/prompt.js:41:
clients/vscode/lib/prompt.js:42: /**
clients/vscode/lib/prompt.js:43:  * Shape the answer for one picked row (or value).
clients/vscode/lib/prompt.js:44:  *
clients/vscode/lib/prompt.js:45:  * @param {object} params `view/prompt` params
clients/vscode/lib/prompt.js:46:  * @param {unknown} picked one row, a string, an array of rows, or undefined
clients/vscode/lib/prompt.js:47:  * @returns {{cancelled: boolean, value?: unknown, custom?: boolean}}
clients/vscode/lib/prompt.js:48:  */
clients/vscode/lib/prompt.js:49: function answerFor(params, picked) {
clients/vscode/lib/prompt.js:50:   if (picked === undefined || picked === null) {
clients/vscode/lib/prompt.js:51:     return { cancelled: true };
clients/vscode/lib/prompt.js:52:   }
clients/vscode/lib/prompt.js:53:   switch (params.kind) {
clients/vscode/lib/prompt.js:54:     case 'confirm':
clients/vscode/lib/prompt.js:55:       return { cancelled: false, value: picked.id === 'yes' };
clients/vscode/lib/prompt.js:56:     case 'text':
clients/vscode/lib/prompt.js:57:       return { cancelled: false, value: String(picked) };
clients/vscode/lib/prompt.js:58:     case 'choice':
clients/vscode/lib/prompt.js:59:       if (picked.id === CUSTOM_ID) {
clients/vscode/lib/prompt.js:60:         // The caller must run a text prompt and use `customChoice`.
clients/vscode/lib/prompt.js:61:         return { cancelled: false, custom: true };
clients/vscode/lib/prompt.js:62:       }
clients/vscode/lib/prompt.js:63:       return { cancelled: false, value: { id: picked.id } };
clients/vscode/lib/prompt.js:64:     case 'multiChoice': {
clients/vscode/lib/prompt.js:65:       const rows = Array.isArray(picked) ? picked : [picked];
clients/vscode/lib/prompt.js:66:       return { cancelled: false, value: { ids: rows.map((row) => row.id) } };
clients/vscode/lib/prompt.js:67:     }
clients/vscode/lib/prompt.js:68:     default:
clients/vscode/lib/prompt.js:69:       return { cancelled: true };
clients/vscode/lib/prompt.js:70:   }
clients/vscode/lib/prompt.js:71: }
clients/vscode/lib/prompt.js:72:
clients/vscode/lib/prompt.js:73: /**
clients/vscode/lib/prompt.js:74:  * The answer value for a custom choice after the text prompt.
clients/vscode/lib/prompt.js:75:  *
clients/vscode/lib/prompt.js:76:  * @param {string|undefined} text the typed value, or undefined on cancel
clients/vscode/lib/prompt.js:77:  * @returns {{cancelled: boolean, value?: {custom: string}}}
clients/vscode/lib/prompt.js:78:  */
clients/vscode/lib/prompt.js:79: function customChoice(text) {
clients/vscode/lib/prompt.js:80:   if (text === undefined || text === null) {
clients/vscode/lib/prompt.js:81:     return { cancelled: true };
clients/vscode/lib/prompt.js:82:   }
clients/vscode/lib/prompt.js:83:   return { cancelled: false, value: { custom: text } };
clients/vscode/lib/prompt.js:84: }
clients/vscode/lib/prompt.js:85:
clients/vscode/lib/prompt.js:86: module.exports = { CUSTOM_ID, promptItems, answerFor, customChoice };
clients/vscode/lib/requests.js:183: async function promptFor(params) {
clients/vscode/lib/requests.js:184:   if (params.kind === 'text') {
clients/vscode/lib/requests.js:185:     const value = await vscode.window.showInputBox({
clients/vscode/lib/requests.js:186:       title: params.title,
clients/vscode/lib/requests.js:187:       prompt: params.detail,
clients/vscode/lib/requests.js:188:       placeHolder: params.placeholder,
clients/vscode/lib/requests.js:189:       value: typeof params.default === 'string' ? params.default : undefined,
clients/vscode/lib/requests.js:190:       password: Boolean(params.secret),
clients/vscode/lib/requests.js:191:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:192:     });
clients/vscode/lib/requests.js:193:     return prompt.answerFor(params, value);
clients/vscode/lib/requests.js:194:   }
clients/vscode/lib/requests.js:195:
clients/vscode/lib/requests.js:196:   if (params.kind === 'confirm' || params.kind === 'choice') {
clients/vscode/lib/requests.js:197:     const picked = await vscode.window.showQuickPick(prompt.promptItems(params), {
clients/vscode/lib/requests.js:198:       title: params.title,
clients/vscode/lib/requests.js:199:       placeHolder: params.detail || params.title,
clients/vscode/lib/requests.js:200:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:201:     });
clients/vscode/lib/requests.js:202:     const answer = prompt.answerFor(params, picked);
clients/vscode/lib/requests.js:203:     if (!answer.custom) {
clients/vscode/lib/requests.js:204:       return answer;
clients/vscode/lib/requests.js:205:     }
clients/vscode/lib/requests.js:206:     const text = await vscode.window.showInputBox({
clients/vscode/lib/requests.js:207:       title: params.title,
clients/vscode/lib/requests.js:208:       placeHolder: params.placeholder || 'Value',
clients/vscode/lib/requests.js:209:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:210:     });
clients/vscode/lib/requests.js:211:     return prompt.customChoice(text);
clients/vscode/lib/requests.js:212:   }
clients/vscode/lib/requests.js:213:
clients/vscode/lib/requests.js:214:   if (params.kind === 'multiChoice') {
clients/vscode/lib/requests.js:215:     const preselected = new Set(params.preselected || []);
clients/vscode/lib/requests.js:216:     const items = prompt.promptItems(params).map((row) => ({
clients/vscode/lib/requests.js:217:       ...row,
clients/vscode/lib/requests.js:218:       picked: preselected.has(row.id),
clients/vscode/lib/requests.js:219:     }));
clients/vscode/lib/requests.js:220:     const picked = await vscode.window.showQuickPick(items, {
clients/vscode/lib/requests.js:221:       title: params.title,
clients/vscode/lib/requests.js:222:       placeHolder: params.detail || params.title,
clients/vscode/lib/requests.js:223:       canPickMany: true,
clients/vscode/lib/requests.js:224:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:225:     });
clients/vscode/lib/requests.js:226:     return prompt.answerFor(params, picked);
clients/vscode/lib/requests.js:227:   }
clients/vscode/lib/requests.js:228:
clients/vscode/lib/requests.js:229:   // An unknown kind cannot be repaired client-side; cancel it.
clients/vscode/lib/requests.js:230:   return { cancelled: true };
clients/vscode/lib/requests.js:231: }
```

Both recognize confirm/text/choice/multiChoice and cancel unknown kinds; widget code and answer flows differ (Neovim `vim.ui`/`inputsecret`; VSCode InputBox/QuickPick). This is semantic correspondence, not identical tables.

### Effect and placement names/branches

```lua
lua/magit-lsp/config.lua:9: --- The status placements this client implements (R-UI-7).
lua/magit-lsp/config.lua:10: M.PLACEMENTS = { 'replace', 'vsplit', 'split', 'tab' }
lua/magit-lsp/config.lua:11:
lua/magit-lsp/config.lua:97: function M.client_capabilities()
lua/magit-lsp/config.lua:98:   local capabilities = vim.lsp.protocol.make_client_capabilities()
lua/magit-lsp/config.lua:99:   capabilities.experimental = capabilities.experimental or {}
lua/magit-lsp/config.lua:100:   capabilities.experimental.magit = {
lua/magit-lsp/config.lua:101:     v = 1,
lua/magit-lsp/config.lua:102:     keys = 'dynamic',
lua/magit-lsp/config.lua:103:     modifiers = M.key_modifiers(),
lua/magit-lsp/config.lua:104:     render = 'edits',
lua/magit-lsp/config.lua:105:     layout = { 'main', 'side', 'bottom', 'float' },
lua/magit-lsp/config.lua:106:     effects = { 'visitFile', 'editDocument', 'clipboard' },
lua/magit-lsp/config.lua:107:     prompts = { 'confirm', 'text', 'choice', 'multiChoice', 'secret' },
lua/magit-lsp/config.lua:108:     folds = 'server',
lua/magit-lsp/config.lua:109:   }
lua/magit-lsp/config.lua:110:   capabilities.prompts = true
lua/magit-lsp/config.lua:111:   capabilities.placements = { 'main', 'side', 'bottom', 'float' }
lua/magit-lsp/config.lua:112:   capabilities.editorBridge = false
lua/magit-lsp/config.lua:113:   return capabilities
lua/magit-lsp/init.lua:264:   -- implementations, not server descriptor placements.
lua/magit-lsp/init.lua:265:   view.register_placement('replace', { place = status_placement(nil) })
lua/magit-lsp/init.lua:266:   view.register_placement('vsplit', { place = status_placement('vsplit') })
lua/magit-lsp/init.lua:267:   view.register_placement('split', { place = status_placement('split') })
lua/magit-lsp/init.lua:268:   view.register_placement('tab', { place = status_placement('tabnew') })
lua/magit-lsp/effects.lua:14: local LEVELS = {
lua/magit-lsp/effects.lua:15:   info = vim.log.levels.INFO,
lua/magit-lsp/effects.lua:16:   warn = vim.log.levels.WARN,
lua/magit-lsp/effects.lua:17:   error = vim.log.levels.ERROR,
lua/magit-lsp/effects.lua:18: }
lua/magit-lsp/effects.lua:25: function M.notify(effect)
lua/magit-lsp/effects.lua:26:   M.last_error_detail = effect.detail
lua/magit-lsp/effects.lua:27:   local text = effect.text or ''
lua/magit-lsp/effects.lua:28:   if effect.hint then
lua/magit-lsp/effects.lua:29:     text = text .. ' - ' .. effect.hint
lua/magit-lsp/effects.lua:30:   end
lua/magit-lsp/effects.lua:31:   text = text:gsub('%s*\n%s*', ' ')
lua/magit-lsp/effects.lua:32:   -- A notification wider than the screen wraps and opens the hit-enter prompt,
lua/magit-lsp/effects.lua:33:   -- so keep a margin below `columns`.
lua/magit-lsp/effects.lua:34:   local limit = math.max(24, vim.o.columns - 12)
lua/magit-lsp/effects.lua:35:   if #text > limit then
lua/magit-lsp/effects.lua:36:     text = text:sub(1, limit - 3) .. '...'
lua/magit-lsp/effects.lua:37:   end
lua/magit-lsp/effects.lua:38:   vim.notify(text, LEVELS[effect.level] or vim.log.levels.INFO)
lua/magit-lsp/effects.lua:39: end
lua/magit-lsp/effects.lua:46: function M.copy(text)
lua/magit-lsp/effects.lua:47:   local ok = pcall(vim.fn.setreg, '+', text)
lua/magit-lsp/effects.lua:48:   pcall(vim.fn.setreg, '"', text)
lua/magit-lsp/effects.lua:49:   local preview = text:gsub('\n', ' ⏎ ')
lua/magit-lsp/effects.lua:50:   if #preview > 60 then
lua/magit-lsp/effects.lua:51:     preview = preview:sub(1, 57) .. '...'
lua/magit-lsp/effects.lua:52:   end
lua/magit-lsp/effects.lua:53:   if ok then
lua/magit-lsp/effects.lua:54:     vim.notify('Copied: ' .. preview, vim.log.levels.INFO)
lua/magit-lsp/effects.lua:55:   else
lua/magit-lsp/effects.lua:56:     vim.notify('Copied: ' .. preview .. ' (clipboard unavailable)', vim.log.levels.WARN)
lua/magit-lsp/effects.lua:57:   end
lua/magit-lsp/effects.lua:58: end
lua/magit-lsp/effects.lua:59:
lua/magit-lsp/effects.lua:60: --- Replay a server `passThrough` key natively, without re-triggering our maps.
lua/magit-lsp/effects.lua:61: ---@param key string canonical chord, e.g. `Escape` or `g Q`
lua/magit-lsp/effects.lua:62: function M.replay(key)
lua/magit-lsp/effects.lua:63:   local lhs = keys.to_lhs(key)
lua/magit-lsp/effects.lua:64:   if not lhs then
lua/magit-lsp/effects.lua:65:     return
lua/magit-lsp/effects.lua:66:   end
lua/magit-lsp/effects.lua:67:   -- `n` means "do not remap", so the client's own buffer-local map is skipped.
lua/magit-lsp/effects.lua:68:   vim.api.nvim_feedkeys(keys.termcodes(lhs), 'n', false)
lua/magit-lsp/effects.lua:101: function M.visit_file(effect)
lua/magit-lsp/effects.lua:102:   if type(effect) ~= 'table' or not effect.uri then
lua/magit-lsp/effects.lua:103:     return
lua/magit-lsp/effects.lua:104:   end
lua/magit-lsp/effects.lua:105:   local filename = vim.uri_to_fname(effect.uri)
lua/magit-lsp/effects.lua:106:   local target = visit_window()
lua/magit-lsp/effects.lua:107:   if vim.api.nvim_get_current_win() ~= target then
lua/magit-lsp/effects.lua:108:     vim.api.nvim_set_current_win(target)
lua/magit-lsp/effects.lua:109:   end
lua/magit-lsp/effects.lua:110:   vim.cmd('edit ' .. vim.fn.fnameescape(filename))
lua/magit-lsp/effects.lua:111:   local line = math.max((effect.line or 0) + 1, 1)
lua/magit-lsp/effects.lua:112:   local column = math.max(effect.column or 0, 0)
lua/magit-lsp/effects.lua:113:   vim.api.nvim_win_set_cursor(0, { line, column })
lua/magit-lsp/effects.lua:114: end
lua/magit-lsp/effects.lua:115:
```

```js
clients/vscode/lib/session.js:41:         initialize(params, second, third) {
clients/vscode/lib/session.js:42:           params.clientInfo = { name: 'Visual Studio Code', version: vscode.version };
clients/vscode/lib/session.js:43:           params.capabilities = params.capabilities || {};
clients/vscode/lib/session.js:44:           params.capabilities.experimental = params.capabilities.experimental || {};
clients/vscode/lib/session.js:45:           params.capabilities.experimental.magit = {
clients/vscode/lib/session.js:46:             v: 1,
clients/vscode/lib/session.js:47:             keys: 'static',
clients/vscode/lib/session.js:48:             modifiers: 'full',
clients/vscode/lib/session.js:49:             render: 'edits',
clients/vscode/lib/session.js:50:             layout: ['main', 'side', 'bottom', 'float'],
clients/vscode/lib/session.js:51:             effects: ['visitFile', 'editDocument', 'clipboard'],
clients/vscode/lib/session.js:52:             prompts: ['confirm', 'text', 'choice', 'multiChoice', 'secret'],
clients/vscode/lib/session.js:53:             folds: 'server',
clients/vscode/lib/session.js:54:           };
clients/vscode/lib/session.js:55:           const next = typeof second === 'function' ? second : third;
clients/vscode/lib/session.js:56:           return next(params);
clients/vscode/lib/session.js:57:         },
clients/vscode/lib/effects.js:9: const REPLAY_COMMANDS = {
clients/vscode/lib/effects.js:10:   'ctrl+c': 'editor.action.clipboardCopyAction',
clients/vscode/lib/effects.js:11:   'ctrl+x': 'editor.action.clipboardCutAction',
clients/vscode/lib/effects.js:12:   'ctrl+v': 'editor.action.clipboardPasteAction',
clients/vscode/lib/effects.js:13: };
clients/vscode/lib/effects.js:14:
clients/vscode/lib/effects.js:15: /**
clients/vscode/lib/effects.js:16:  * Perform an editor behaviour the server passed through.
clients/vscode/lib/effects.js:17:  *
clients/vscode/lib/effects.js:18:  * A static client cannot translate arbitrary chords into VSCode commands, so
clients/vscode/lib/effects.js:19:  * printable characters and clipboard shortcuts are the supported cases.
clients/vscode/lib/effects.js:20:  *
clients/vscode/lib/effects.js:21:  * @param {string} key
clients/vscode/lib/effects.js:22:  */
clients/vscode/lib/effects.js:23: async function replay(key) {
clients/vscode/lib/effects.js:24:   if (REPLAY_COMMANDS[key]) {
clients/vscode/lib/effects.js:25:     await vscode.commands.executeCommand(REPLAY_COMMANDS[key]);
clients/vscode/lib/effects.js:26:     return;
clients/vscode/lib/effects.js:27:   }
clients/vscode/lib/effects.js:28:   if (key === 'Space') {
clients/vscode/lib/effects.js:29:     await vscode.commands.executeCommand('default:type', { text: ' ' });
clients/vscode/lib/effects.js:30:     return;
clients/vscode/lib/effects.js:31:   }
clients/vscode/lib/effects.js:32:   if (typeof key === 'string' && Array.from(key).length === 1) {
clients/vscode/lib/effects.js:33:     await vscode.commands.executeCommand('default:type', { text: key });
clients/vscode/lib/effects.js:34:     return;
clients/vscode/lib/effects.js:35:   }
clients/vscode/lib/effects.js:36:   store.output.appendLine(`passThrough: cannot replay ${key} from a static binding`);
clients/vscode/lib/effects.js:37: }
clients/vscode/lib/effects.js:44: function showMessage(effect) {
clients/vscode/lib/effects.js:45:   const text = effect.hint ? `${effect.text}\n${effect.hint}` : effect.text || '';
clients/vscode/lib/effects.js:46:   if (effect.detail) {
clients/vscode/lib/effects.js:47:     store.output.appendLine(effect.detail);
clients/vscode/lib/effects.js:48:   }
clients/vscode/lib/effects.js:49:   if (effect.level === 'error') {
clients/vscode/lib/effects.js:50:     vscode.window.showErrorMessage(text);
clients/vscode/lib/effects.js:51:   } else if (effect.level === 'warn') {
clients/vscode/lib/effects.js:52:     vscode.window.showWarningMessage(text);
clients/vscode/lib/effects.js:53:   } else {
clients/vscode/lib/effects.js:54:     vscode.window.showInformationMessage(text);
clients/vscode/lib/effects.js:55:   }
clients/vscode/lib/effects.js:56: }
clients/vscode/lib/effects.js:63: async function applyEffects(effectList, originViewId) {
clients/vscode/lib/effects.js:64:   const origin = originViewId && store.views.get(originViewId);
clients/vscode/lib/effects.js:65:   let rootId = origin && origin.rootId;
clients/vscode/lib/effects.js:66:   for (const effect of effectList || []) {
clients/vscode/lib/effects.js:67:     switch (effect.type) {
clients/vscode/lib/effects.js:68:       case 'message':
clients/vscode/lib/effects.js:69:         showMessage(effect);
clients/vscode/lib/effects.js:70:         break;
clients/vscode/lib/effects.js:71:       case 'clipboard':
clients/vscode/lib/effects.js:72:         await vscode.env.clipboard.writeText(effect.text || '');
clients/vscode/lib/effects.js:73:         vscode.window.showInformationMessage('Copied to clipboard.');
clients/vscode/lib/effects.js:74:         break;
clients/vscode/lib/effects.js:75:       case 'passThrough':
clients/vscode/lib/effects.js:76:         await replay(effect.key);
clients/vscode/lib/effects.js:77:         break;
clients/vscode/lib/effects.js:78:       case 'openView':
clients/vscode/lib/effects.js:79:       case 'pushView':
clients/vscode/lib/effects.js:80:         {
clients/vscode/lib/effects.js:81:           const state = views.ensureView(effect.descriptor);
clients/vscode/lib/effects.js:82:           if (state) state.rootId = rootId || (effect.type === 'openView' ? state.id : undefined);
clients/vscode/lib/effects.js:83:         }
clients/vscode/lib/effects.js:84:         await views.showView(effect.descriptor);
clients/vscode/lib/effects.js:85:         break;
clients/vscode/lib/effects.js:86:       case 'popView':
clients/vscode/lib/effects.js:87:         await views.closeView(effect.viewId, true);
clients/vscode/lib/effects.js:88:         break;
clients/vscode/lib/effects.js:89:       case 'focusView':
clients/vscode/lib/effects.js:90:         await views.focusView(effect.viewId);
clients/vscode/lib/effects.js:91:         break;
clients/vscode/lib/effects.js:92:       case 'quit':
clients/vscode/lib/effects.js:93:         await views.closeRoot(rootId || originViewId);
clients/vscode/lib/effects.js:94:         break;
clients/vscode/lib/effects.js:95:       case 'visitFile': {
clients/vscode/lib/effects.js:96:         const document = await vscode.workspace.openTextDocument(
clients/vscode/lib/effects.js:97:           vscode.Uri.parse(effect.uri)
clients/vscode/lib/effects.js:98:         );
clients/vscode/lib/effects.js:99:         const editor = await vscode.window.showTextDocument(document, {
clients/vscode/lib/effects.js:100:           preview: false,
clients/vscode/lib/effects.js:101:           preserveFocus: false,
clients/vscode/lib/effects.js:102:         });
clients/vscode/lib/effects.js:103:         const position = new vscode.Position(effect.line || 0, effect.column || 0);
clients/vscode/lib/effects.js:104:         editor.selection = new vscode.Selection(position, position);
clients/vscode/lib/effects.js:105:         editor.revealRange(
clients/vscode/lib/effects.js:106:           new vscode.Range(position, position),
clients/vscode/lib/effects.js:107:           vscode.TextEditorRevealType.InCenterIfOutsideViewport
clients/vscode/lib/effects.js:108:         );
clients/vscode/lib/effects.js:109:         break;
clients/vscode/lib/effects.js:110:       }
clients/vscode/lib/effects.js:111:       case 'editDocument':
clients/vscode/lib/effects.js:112:         await drafts.open(effect, rootId || originViewId);
clients/vscode/lib/effects.js:113:         break;
clients/vscode/lib/effects.js:114:       default:
clients/vscode/lib/effects.js:115:         store.output.appendLine(`unknown effect: ${effect.type}`);
clients/vscode/lib/effects.js:116:     }
clients/vscode/lib/effects.js:117:   }
clients/vscode/lib/effects.js:118: }
clients/vscode/lib/effects.js:119:
clients/vscode/lib/effects.js:120: exports.applyEffects = applyEffects;
clients/vscode/lib/effects.js:121: exports.showMessage = showMessage;
clients/vscode/lib/effects.js:122: exports.replay = replay;
```

The advertised effect and prompt array string values in Lua `config.lua` and VSCode `session.js` are byte-identical literal sequences: effects `visitFile, editDocument, clipboard`; prompts `confirm, text, choice, multiChoice, secret`. This does not imply equal implementations. Neovim has status placements `replace/vsplit/split/tab`; no corresponding VSCode placement table appears in inspected package contributions or virtual-document view code. Layout capability values (`main/side/bottom/float`) match but are not status placement values. VSCode effect cases and Neovim routing are shown verbatim above.

## E. Initialize traces

Extraction/search commands: `find build -type f \( -name '*.jsonl' -o -name '*.log' \) | sort`; `grep -n -E 'initialize|didChange|clientInfo|capabilities' build/helix_lsp_probe.jsonl build/test_helix_lsp.jsonl build/test-output/trace-lua.jsonl build/ui/plain-render/with-final-newline/state/nvim/lsp.log build/ui/plain-render/without-final-newline/state/nvim/lsp.log`; `python3` JSONL extraction of Helix `data.params`; and `Path('build/ui/plain-render/with-final-newline/state/nvim/lsp.log').read_text().splitlines()[2]` for the full Neovim `init_params` literal. The latter is inserted verbatim below, not converted to JSON.

### Helix: actual captured JSONL

`build/helix_lsp_probe.jsonl:2` (`received`, actual `initialize`) complete raw record, with only `/home/<user>` redacted:

```json
{"ts": "2026-10-28T05:24:35.809941+00:00", "pid": 3552547, "event": "received", "data": {"jsonrpc": "2.0", "method": "initialize", "params": {"capabilities": {"general": {"positionEncodings": ["utf-8", "utf-32", "utf-16"]}, "textDocument": {"codeAction": {"codeActionLiteralSupport": {"codeActionKind": {"valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"]}}, "dataSupport": true, "disabledSupport": true, "isPreferredSupport": true, "resolveSupport": {"properties": ["edit", "command"]}}, "completion": {"completionItem": {"deprecatedSupport": true, "insertReplaceSupport": true, "resolveSupport": {"properties": ["documentation", "detail", "additionalTextEdits"]}, "snippetSupport": true, "tagSupport": {"valueSet": [1]}}, "completionItemKind": {}}, "formatting": {"dynamicRegistration": false}, "hover": {"contentFormat": ["markdown"]}, "inlayHint": {"dynamicRegistration": false}, "publishDiagnostics": {"tagSupport": {"valueSet": [1, 2]}, "versionSupport": true}, "rename": {"dynamicRegistration": false, "honorsChangeAnnotations": false, "prepareSupport": true}, "signatureHelp": {"signatureInformation": {"activeParameterSupport": true, "documentationFormat": ["markdown"], "parameterInformation": {"labelOffsetSupport": true}}}}, "window": {"workDoneProgress": true}, "workspace": {"applyEdit": true, "configuration": true, "didChangeConfiguration": {"dynamicRegistration": false}, "didChangeWatchedFiles": {"dynamicRegistration": true, "relativePatternSupport": false}, "executeCommand": {"dynamicRegistration": false}, "fileOperations": {"didRename": true, "willRename": true}, "inlayHint": {"refreshSupport": false}, "symbol": {"dynamicRegistration": false}, "workspaceEdit": {"documentChanges": true, "failureHandling": "abort", "normalizesLineEndings": false, "resourceOperations": ["create", "rename", "delete"]}, "workspaceFolders": true}}, "clientInfo": {"name": "helix", "version": "25.07.1"}, "processId": 3552534, "rootPath": "[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe", "rootUri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe", "workspaceFolders": [{"name": "helix-lsp-probe", "uri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe"}]}, "id": 0}}
```

`build/helix_lsp_probe.jsonl:13` (`received`, actual `textDocument/didChange`), same redaction:

```json
{"ts": "2026-10-28T05:25:07.751771+00:00", "pid": 3552547, "event": "received", "data": {"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"contentChanges": [{"range": {"end": {"character": 0, "line": 0}, "start": {"character": 0, "line": 0}}, "text": "i"}], "textDocument": {"uri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe/demo.mgprobe", "version": 1}}}}
```

The initialize record includes the full `params.capabilities`, `clientInfo` (`helix`, `25.07.1`), process/root/workspace values. Independent capture `build/test_helix_lsp.jsonl:3` also has actual initialize and `:7-8` actual didChange. The fixture `tests/fixtures/lsp_traces/reconstructed-plain.jsonl` is explicitly reconstructed, not a trace.

### Plain Neovim `vim.lsp.start`, no plugin: actual native log

`build/ui/plain-render/with-final-newline/state/nvim/lsp.log:3` is the full 5,742-byte actual Neovim `init_params` record. It is copied verbatim (full capabilities and `clientInfo` included); this is Lua `vim.inspect` text, not JSON:

```text
[TRACE][2026-10-31 09:50:32] ...-unwrapped-0.12.6/share/nvim/runtime/lua/vim/lsp/log.lua:150	"LSP[magit-lsp-plain-smoke]"	"init_params"	{ capabilities = { general = { positionEncodings = { "utf-8", "utf-16", "utf-32" } }, textDocument = { callHierarchy = { dynamicRegistration = false }, codeAction = { codeActionLiteralSupport = { codeActionKind = { valueSet = { "", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports" } } }, dataSupport = true, disabledSupport = true, dynamicRegistration = true, honorsChangeAnnotations = true, isPreferredSupport = true, resolveSupport = { properties = { "edit", "command" } } }, codeLens = { dynamicRegistration = false, resolveSupport = { properties = { "command" } } }, colorProvider = { dynamicRegistration = true }, completion = { completionItem = { commitCharactersSupport = false, deprecatedSupport = true, documentationFormat = { "markdown", "plaintext" }, insertReplaceSupport = true, labelDetailsSupport = true, preselectSupport = false, resolveSupport = { properties = { "additionalTextEdits", "command", "documentation" } }, snippetSupport = true, tagSupport = { valueSet = { 1 } } }, completionItemKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25 } }, completionList = { itemDefaults = { "editRange", "insertTextFormat", "insertTextMode", "data" } }, contextSupport = true, dynamicRegistration = false }, declaration = { linkSupport = true }, definition = { dynamicRegistration = true, linkSupport = true }, diagnostic = { dataSupport = true, dynamicRegistration = true, relatedDocumentSupport = true, relatedInformation = true, tagSupport = { valueSet = { 1, 2 } } }, documentHighlight = { dynamicRegistration = false }, documentLink = { dynamicRegistration = false, tooltipSupport = false }, documentSymbol = { dynamicRegistration = false, hierarchicalDocumentSymbolSupport = true, symbolKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26 } }, tagSupport = { valueSet = { 1 } } }, foldingRange = { dynamicRegistration = false, foldingRange = { collapsedText = true }, foldingRangeKind = { valueSet = { "comment", "imports", "region" } }, lineFoldingOnly = true }, formatting = { dynamicRegistration = true }, hover = { contentFormat = { "markdown", "plaintext" }, dynamicRegistration = true }, implementation = { linkSupport = true }, inlayHint = { dynamicRegistration = true, resolveSupport = { properties = { "textEdits", "tooltip", "command", "label.location", "label.tooltip", "label.command" } } }, inlineCompletion = { dynamicRegistration = false }, linkedEditingRange = { dynamicRegistration = false }, onTypeFormatting = { dynamicRegistration = false }, publishDiagnostics = { dataSupport = true, relatedInformation = true, tagSupport = { valueSet = { 1, 2 } } }, rangeFormatting = { dynamicRegistration = true, rangesSupport = true }, references = { dynamicRegistration = false }, rename = { dynamicRegistration = true, honorsChangeAnnotations = true, prepareSupport = true }, selectionRange = { dynamicRegistration = false }, semanticTokens = { augmentsSyntaxTokens = true, dynamicRegistration = false, formats = { "relative" }, multilineTokenSupport = true, overlappingTokenSupport = true, requests = { full = { delta = true }, range = true }, serverCancelSupport = false, tokenModifiers = { "declaration", "definition", "readonly", "static", "deprecated", "abstract", "async", "modification", "documentation", "defaultLibrary" }, tokenTypes = { "namespace", "type", "class", "enum", "interface", "struct", "typeParameter", "parameter", "variable", "property", "enumMember", "event", "function", "method", "macro", "keyword", "modifier", "comment", "string", "number", "regexp", "operator", "decorator" } }, signatureHelp = { dynamicRegistration = false, signatureInformation = { activeParameterSupport = true, documentationFormat = { "markdown", "plaintext" }, noActiveParameterSupport = true, parameterInformation = { labelOffsetSupport = true } } }, synchronization = { didSave = true, dynamicRegistration = false, willSave = true, willSaveWaitUntil = true }, typeDefinition = { linkSupport = true } }, window = { showDocument = { support = true }, showMessage = { messageActionItem = { additionalPropertiesSupport = true } }, workDoneProgress = true }, workspace = { applyEdit = true, codeLens = { refreshSupport = true }, configuration = true, diagnostics = { refreshSupport = true }, didChangeConfiguration = { dynamicRegistration = false }, didChangeWatchedFiles = { dynamicRegistration = false, relativePatternSupport = true }, executeCommand = { dynamicRegistration = false }, fileOperations = { didCreate = false, didDelete = false, didRename = false, dynamicRegistration = false, willCreate = false, willDelete = false, willRename = false }, inlayHint = { refreshSupport = true }, semanticTokens = { refreshSupport = true }, symbol = { dynamicRegistration = false, symbolKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26 } } }, workspaceEdit = { changeAnnotationSupport = { groupsOnLabel = true }, normalizesLineEndings = true, resourceOperations = { "rename", "create", "delete" } }, workspaceFolders = true } }, clientInfo = { name = "Neovim", version = "0.12.6" }, processId = 2591173, rootPath = "/tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", rootUri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", trace = "off", workDoneToken = "1", workspaceFolders = { { name = "/tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", uri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo" } } }
```

Actual client `didChange`, `build/ui/plain-render/with-final-newline/state/nvim/lsp.log:18` (verbatim log record; no home/user path occurs in this record):

```text
[DEBUG][2026-10-31 09:50:33] ...-unwrapped-0.12.6/share/nvim/runtime/lua/vim/lsp/log.lua:150	"rpc.send"	{ jsonrpc = "2.0", method = "textDocument/didChange", params = { contentChanges = { { range = { ["end"] = { character = 27, line = 0 }, start = { character = 0, line = 0 } }, rangeLength = 27, text = "Head: master\n\nStaged changes (0)\n\nUnstaged changes (0)\n\nUntracked files (0)\n\nWorking tree clean" } }, textDocument = { uri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo/.magit", version = 4 } } }
```

`build/ui/plain-render/without-final-newline/state/nvim/lsp.log:3,18` independently records the other EOF variant. No JSON reconstruction is claimed: the observed log uses Lua table syntax; full raw line retained above.

### Bundled clients: NOT FOUND in searched artifacts

No actual bundled Neovim/VSCode initialize or didChange capture was located by the inventory and grep commands above. `build/test-output/trace-lua.jsonl` is 13 lines and has no initialize/didChange/clientInfo/capabilities matches. The two plain-render Neovim logs are plain `vim.lsp.start` runs, not bundled-plugin captures. No params were synthesized from client configuration. Helix is present above; no Helix evidence is inferred from the reconstructed fixture.

## Explicitly unobtained items / reasons

- **Bundled Neovim and bundled VSCode initialize/didChange captures: NOT FOUND**
  in the searched artifacts listed in E. Their configurations are source evidence,
  not substitute wire traces. No requests were synthesized.
- **Plain Neovim raw initialize JSON: NOT FOUND.** The available actual capture
  is a complete Lua `vim.inspect` record, reproduced in E with its full
  capabilities and clientInfo. It is deliberately not labelled raw wire JSON.
- **Real VSCode client→server status-open wire excerpt: NOT FOUND** in the
  searched logs (B/E). Source tracing settings and launcher paths are recorded;
  no fresh VSCode host capture was obtained for this report.
- **Optional xvfb/CDP native Save/Don't Save dialog DOM experiment: NOT RUN.**
  No conclusion about that dialog's DOM accessibility is inferred from source.
- **`docs/plain-support.md`: NOT FOUND.** Planned prompt/select/draft support
  and remaining Plain acceptance must not be inferred from the existing rich
  protocol types. Section C separates implemented behavior from pending work.

Only the report and the mandatory orchestration entry/checkoff in `TASKS.md`
belong to this task. Pre-existing release changes were preserved. No images were
produced or read; image-related paths in this report are text search evidence only.
