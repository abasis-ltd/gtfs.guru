---
name: gtfs-validator-rust-rewrite
description: Full-stack Rust rewrite plan with GTFS rule parity
---

# Plan

Deliver a full-stack Rust rewrite of the GTFS validator with complete rule parity and output compatibility, using the current Java implementation as the behavioral reference. The work proceeds in phases: baseline parity definitions, Rust core implementation, interface layers, and acceptance validation.

## Requirements
- Match all existing GTFS validation checks, notice codes, and severities.
- Preserve report formats and contents for `report.json`, `system_errors.json`, and HTML.
- Support CLI, web service, and desktop GUI workflows used today.
- Keep performance within an acceptable delta or document changes.

## Scope
- In: full GTFS parsing/loading, rule execution, report generation, CLI, web service API, desktop app packaging.
- Out: new features beyond parity unless required to match current behavior.

## Files and entry points
- Reference behavior: `main`, `core`, `model`, `processor`, `cli`, `web/service`, `app/gui`, `output-comparator`.
- Target Rust structure (proposed):
  - `crates/gtfs_validator_core` (parsing, validation engine, rules, notices)
  - `crates/gtfs_model` (schema/types)
  - `crates/gtfs_validator_report` (JSON/HTML output)
  - `crates/gtfs_validator_cli`
  - `crates/gtfs_validator_web`
  - `crates/gtfs_validator_gui`
  - `crates/gtfs_validator_codegen` (if needed for schema/rule generation)

## Data model / API changes
- Define Rust equivalents for GTFS schema, table loaders, and notice metadata.
- Choose approach to replace Java annotation-based codegen (proc-macros vs. build-time generation).
- Lock report schemas and notice naming to the Java baseline for diff-friendly comparison.

## Action items
[ ] Baseline parity: freeze current Java outputs and define the JSON/HTML schemas as reference artifacts.
[ ] Map Java modules to Rust crates and agree on layering boundaries and public APIs.
[ ] Implement core GTFS I/O, CSV parsing, types (time/date/color), and feed container model.
[ ] Build validation framework (validators discovery, execution, notice emission, ordering).
[ ] Port all validation rules with parity tests per rule group.
[ ] Implement report generation (JSON + HTML) to match existing formats.
[ ] Implement CLI with flag compatibility and output directory structure.
[ ] Implement web service endpoints, storage model, and request/response contracts.
[ ] Implement GUI packaging and launch flow (desktop app) with equivalent options.
[ ] Run unit tests, golden-file diffs, and acceptance tests against MobilityDatabase feeds.
[ ] Document migration notes, rollout plan, and fallback strategy to Java if parity gaps exist.

## Testing and validation
- Rust unit tests for parsing, types, and validators.
- Golden-file tests comparing Rust `report.json`/HTML against Java outputs.
- End-to-end tests mirroring CLI and web workflows.
- Acceptance tests using existing output-comparator and MobilityDatabase corpus.
  - Golden workflow helpers: `docs/golden.md`

## Risks and edge cases
- Subtle behavior drift in rules, notice ordering, or schema edge cases.
- Performance regressions on large feeds or memory overhead differences.
- GUI and web service behavior parity (storage, auth, and deployment details).

## Open questions
- Are any Java-only integrations (packaging, JVM tooling) acceptable to keep temporarily during transition?
- What is the required parity bar for HTML rendering (pixel-perfect vs. content parity)?
- Are there platform constraints for the desktop app that require a specific Rust GUI stack?
