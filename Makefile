.PHONY: build test test-rust test-lua test-file color-spike

build:
	cargo build

test: test-rust test-lua

test-rust:
	cargo test

test-lua: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()"

test-file: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')"

color-spike: build
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua
