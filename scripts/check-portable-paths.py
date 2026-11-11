import json
import re
from pathlib import Path

root = Path(__file__).resolve().parent.parent
text = (root / "opencode.jsonc").read_text()
data = json.loads(re.sub(r"(?m)^\s*//[^\n]*", "", text))
commands = [data["shell"], data["lsp"]["rust"]["command"][0],
            data["lsp"]["lua-ls"]["command"][0],
            data["lsp"]["typescript-node"]["command"][0]]
for command in commands:
    path = Path(command)
    if path.is_absolute() or not (root / path).is_file():
        raise SystemExit(f"Non-portable or missing executable: {command}")
if "plugins" in data or re.search(r"/home/|/Users/|[A-Z]:\\\\", text):
    raise SystemExit("Private plugin or machine-specific path remains in opencode.jsonc")

spike = (root / "clients/vscode/test/e2e/real-file-spike.mjs").read_text()
if spike.count("process.env.MAGIT_VSCODE_BIN || 'code'") != 2:
    raise SystemExit("Spike must use MAGIT_VSCODE_BIN or PATH-discovered code")
if re.search(r"/home/|/Users/|[A-Z]:\\\\", spike):
    raise SystemExit("Machine-specific path remains in VSCode spike")
print("Portable executable references and spike editor fallback verified")
