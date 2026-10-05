# Project rules for AI agents

Project: God of War II (SCUS-97481, NTSC-U v1.01) reverse engineering and Rust rewrite.
Start with `README.md` and `docs/research-status.md`. Goal and order of work: `docs/project-goal.md`.

## Testing and acceptance
- AI never runs gameplay tests or claims tests passed. Humans own all playtesting and acceptance.
- Automated checks (`cargo test`, Python oracle comparisons) run only when the user asks, and the
  full output is reported unfiltered. A passing check is evidence, never acceptance.

## Ask first
- Ask before installing tools, compiling or recompiling, publishing, or deleting anything.
- Never change system or security settings.

## One source of truth
- The model lives in `analysis/symbols.tsv`, `structs.txt`, `sigs.tsv`, `volatile.tsv`. Keep it
  authoritative; do not create parallel copies of the same facts.
- Never hand-patch generated output (`analysis/exports/`, `analysis/disasm/`, `analysis/core_decomp/`,
  `docs/core-functions.md`, `*.tsv` produced by `tools/`). Fix the generating tool or the model and
  regenerate.

## Evidence labels
- Label every statement as a fact, a hypothesis or an unknown.
- Facts go in `docs/confirmed.md` only when the binary (and disc, where relevant) shows them directly.
  Interpretations go in `docs/hypotheses.md` with a level (HIGH, MEDIUM, LOW, SPECULATIVE) and what
  would confirm or reject them.
- Document the original game faithfully; do not shape the docs around the Rust port.

## Preserve
- Never modify or delete the originals: `God of War II.iso`, `extracted/`, `snapshots/`, saves, RAM dumps.
- Never put `extracted/` or `analysis/exports/` into a repository (copyrighted).
- Never read, print or store credentials or tokens.

## Context tools
- Use RTK and Billion Context, but never treat filtered output or summaries as authoritative evidence.
  Recover the original records and full error details when they matter.
- Never stack duplicate context plugins.

## Working style
- Explain each step in plain language.
- Stop and report if a required integration is unavailable.
- All work stays in one session; no sub-agents unless the user asks.

## Session start
- First inspect available tools read-only and list what is present and missing.
- Do not install, build, test or deploy during this inspection.
