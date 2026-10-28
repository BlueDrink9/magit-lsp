# Contributing to magit-lsp

Contributions to magit-lsp are welcome, especially editor integrations, but also server functionality, tests, documentation, bug fixes, and protocol improvements.

For editor integrations, if it is more than documenting a few paragraphs of configuration, consider storing it in a separate repository for you to maintain rather than pushing it upstream.

## Design and code quality

magit-lsp follows a **thin-client, thick-server architecture**. The Rust server owns Git operations, semantic state, rendering, navigation, menus, and command dispatch. Editor clients should remain as small and independent as practical.

Contributions should follow these principles:

- **Prefer standard LSP.** Use standard protocol capabilities wherever practical, particularly for functionality intended to work across editors without plugins.
- **Keep clients thin.** Avoid duplicating Git operations, application state, or business logic in editor integrations.
- **Preserve compatibility.** Editor-specific functionality should not introduce unnecessary requirements for other clients or the standard-LSP Level 0 interface.
- **Keep extensions capability-sensitive.** Custom protocol features should be documented and used only where supported.
- **Prefer simple, incremental changes.** Avoid unnecessary abstractions, unrelated refactoring, and premature generalisation.
- **Test behaviour.** Add or update tests for relevant changes. Verify editor-specific behaviour rather than assuming consistent LSP support across clients.
- **Document significant decisions.** Changes to protocols, architecture, or compatibility guarantees should include their rationale and tradeoffs.

New editor clients are encouraged. Implementations should reuse the existing protocol and server functionality rather than introducing editor-specific alternatives.

## Contribution responsibility

Contributors are responsible for the correctness, quality, and provenance of submitted material, including code produced with AI assistance. **AI-assisted development is welcome but subject to the same standards as manually written contributions.**

Contributors must:

- Understand and review submitted code, including its significant implementation decisions.
- Test changes appropriately and disclose known limitations or unverified behaviour.
- Ensure they have the necessary rights to contribute submitted material under the applicable license.
- Identify substantial third-party code and preserve required attribution and license notices.
- Investigate identifiable similarities to third-party code where provenance or licensing may be uncertain.

Generated output should not be assumed to be correct, original, or free of licensing obligations merely because it was produced by an AI tool.

## Licensing

Different components of magit-lsp use different licenses:

| Component | License |
|---|---|
| Rust language server | GPL-3.0-or-later |
| Editor integrations | MIT |
| Independently reusable protocol libraries | MIT OR Apache-2.0 |

Contributions are submitted under the license applicable to the component being modified, unless explicitly agreed otherwise.

Contributors retain copyright in their contributions where applicable. Copyright assignment and contributor license agreements are not required.

Licensing questions should be resolved before incorporating potentially incompatible material.

## AI assistance disclosure

Substantial AI assistance should be disclosed in pull requests. A brief statement is sufficient, covering:

- **Models used:** Identify the AI models involved, including versions where known.
- **Nature of assistance:** Describe whether AI was used for implementation, refactoring, testing, documentation, design, or review.
- **Verification:** Summarise the human review and testing performed, particularly for substantially generated code.

Routine autocomplete, spelling corrections, and minor syntax assistance do not require disclosure. Prompt histories and full conversation transcripts are not required.

## Third-party dependencies

New dependencies should be justified by their functionality, maintenance status, licensing, security implications, and effect on project complexity.

Dependencies must be compatible with the license of the component using them. Required notices and attribution must be preserved.

In particular, GPL-licensed server implementation code must not be incorporated into permissively licensed client components without appropriate licensing permission.

## Pull requests

**Prefer small, atomic, incremental pull requests.** Each pull request should address a coherent change that can be reviewed and tested independently where practical.

For larger features, use a series of focused, stacked pull requests rather than a single large submission.

Until GitHub provides better support for stacked pull requests:

- Keep stacked pull requests in **draft** while they depend on unmerged changes.
- Clearly identify the pull request or branch each is based on, preferably with a link near the beginning of the description.
- Mark dependent pull requests ready for review once their prerequisites have been merged or the dependencies have otherwise been resolved.

Pull-request descriptions should cover:

- **Purpose:** The problem being addressed.
- **Approach:** The implementation and any significant design decisions.
- **Testing:** What was verified and what remains untested.
- **AI assistance:** Disclosure as described above, where applicable.

Descriptions should be proportionate to the change. Small fixes need little explanation; architectural and protocol changes should document their rationale and compatibility implications.

Significant architectural changes or changes to public protocols should generally be discussed before implementation.
