.PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour color-spike vscode-test

build:
	cargo build

test: test-rust test-lua vscode-test

vscode-test:
	npm --prefix clients/vscode test
	node clients/vscode/scripts/gen-keybindings.mjs --check

test-rust:
	cargo test

test-lua: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()"

test-file: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')"

ui-unit:
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py' -v

ui-test:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run tests/ui/scenarios/*.scn tests/ui/smoke.scn

ui-tour:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md

color-spike: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua
