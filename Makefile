.PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour color-spike

build:
	cargo build

test: test-rust test-lua

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
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run tests/ui/tour.scn --markdown build/ui/tour.md

color-spike: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua
