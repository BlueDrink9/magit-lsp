# Plain-key edit-shape traces

JSONL records preserve `textDocument/didChange` payload content and versions; only the machine-specific file URI is redacted. `batched-changes.jsonl` is the captured Plain Neovim native `NATIVE-DIVERGENCE` didChange from `make plain-neovim` (version 7; two ordered edits). All other files are explicitly reconstructed profiles, not editor observations. The retained Helix single-insert capture is documented at `docs/evidence/harness-reassessment-inputs-08.md:1091-1097` and is used as provenance, not relabeled as a new capture.

The profile test replays each fixture's ordered notification records and versions through the plain harness's FakeEditor notification seam; shape classification is test-only and is not a production key classifier.
