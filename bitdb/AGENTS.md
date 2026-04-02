# AGENTS.md

Purpose: fast rules for coding agents in this repo.
Scope: whole repo at `/home/kottes/worktree/projects/bitdb`.
Priority: user ask > system/dev rules > this file.

## 1) Repo Snapshot
- Language: Rust.
- Crate type now: binary (`src/main.rs`).
- Main manifest: `Cargo.toml`.
- Planning doc: `IMPLEMENTATION_PLAN.md`.
- Spec source file present: `bitcask-intro.pdf`.
- Current code small; expect growth by phases.

## 2) Repo Structure (current + target)
- `Cargo.toml` crate config.
- `src/main.rs` binary entrypoint.
- `IMPLEMENTATION_PLAN.md` phased delivery checklist.
- `bitcask-intro.pdf` reference material.
- Target tree (as implementation grows):
- `src/lib.rs`
- `src/config.rs`
- `src/error.rs`
- `src/record.rs`
- `src/engine.rs`
- `src/recovery.rs`
- `src/merge.rs`
- `src/storage/`
- `src/index/`
- `src/cli.rs`
- `tests/`
- `benches/`

## 3) Build / Lint / Test Commands
- Build debug: `cargo build`
- Build release: `cargo build --release`
- Check only: `cargo check`
- Run app: `cargo run`
- Run with args: `cargo run -- <args>`
- Format check: `cargo fmt --all -- --check`
- Format write: `cargo fmt --all`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Test all: `cargo test --all-targets --all-features`
- Test lib only: `cargo test --lib`
- Test integration only: `cargo test --tests`
- Test single test fn: `cargo test <test_name>`
- Test single integration test file: `cargo test --test <file_stem>`
- Test single exact name: `cargo test <test_name> -- --exact`
- Test w/ output shown: `cargo test <test_name> -- --nocapture`
- Bench (criterion): `cargo bench`
- Doc build: `cargo doc --no-deps`

## 4) TDD (mandatory)
- Always TDD. no skip.
- Red -> Green -> Refactor loop always.
- Start with smallest failing test.
- Implement minimum code to pass.
- Refactor only with green tests.
- New bug => first add failing regression test.
- No feature PR without tests.
- Prefer integration tests for behavior/API.
- Prefer unit tests for codec/storage internals.
- Keep tests deterministic (seed fixed where random).
- For perf work: correctness tests first, benchmark later.

## 5) Coding Style Rules
- Indentation: always 4 spaces. never tabs.
- Keep ASCII unless file already needs Unicode.
- Keep functions small; split when branchy.
- Keep modules cohesive by concern.
- Use `rustfmt` defaults; do not hand-align.
- Max complexity low; prefer readable over clever.
- Add comments only for non-obvious intent.
- Avoid comment noise explaining obvious code.
- No dead code, no unused imports.

## 6) Imports
- Group order: std, external crates, internal modules.
- Keep import lists minimal and used.
- Avoid wildcard imports (`*`) unless test helper local.
- Alias only when conflict or clarity need.
- In tests, import only test-needed items.

## 7) Naming
- Types/traits: `PascalCase`.
- Functions/vars/modules/files: `snake_case`.
- Constants/statics: `SCREAMING_SNAKE_CASE`.
- Test names: behavior-focused, e.g. `put_overwrite_returns_latest`.
- Avoid abbreviations unless domain standard (`crc`, `ttl`).

## 8) Types and APIs
- Prefer explicit domain types over primitive soup.
- Use `struct` wrappers for semantic ids/offsets when useful.
- Return `Result<T, E>` for fallible operations.
- Avoid `unwrap`/`expect` in production code.
- `unwrap` allowed in tests when setup certainty clear.
- Prefer slices/refs over owned clones when possible.
- Minimize allocations in hot paths.

## 9) Error Handling
- Centralize error type in `src/error.rs`.
- Use `thiserror` for error enums.
- Preserve source errors (`#[source]` / transparent).
- Add context at boundaries (file path, offset, key len, file id).
- Distinguish corruption vs IO vs config errors.
- Never silently swallow corruption in core paths.
- For recoverable startup cases, log and continue per policy.

## 10) Bitcask-style Implementation Notes
- Append-only data files.
- Single active writer model first.
- In-memory keydir for latest key location.
- Tombstones for deletes.
- Startup rebuild scans files oldest->newest.
- Merge rewrites live keys only.
- Atomic install for merge outputs.
- Parallelism only after serial correctness baseline.

## 11) Benchmarking Rules
- Keep serial baseline before any parallel change.
- Compare serial vs parallel same dataset/seed.
- Track ops/s + latency + startup + merge time.
- Fail perf claim if no measured win.
- Keep benchmark commands runnable from CLI.

## 12) Agent Workflow
- Read `IMPLEMENTATION_PLAN.md` before coding.
- Implement one phase scope at a time.
- When phase done, mark that phase `[x] Completed` in `IMPLEMENTATION_PLAN.md`.
- If user says `IMPLEMENTATION_FILE.md`, treat as `IMPLEMENTATION_PLAN.md` unless file created later.
- Do not mark phase complete if any listed item missing.
- Keep changes minimal and phase-focused.

## 13) Git / Safety
- Never revert user unrelated local changes.
- Never run destructive git commands.
- Do not commit unless user explicitly asks.
- If committing, keep message concise, reason-focused.
- After a phase is fully completed and all checks pass (`fmt`, `clippy`, tests), commit that phase changes.
- After any huge chunk of code is completed and all checks pass, commit that chunk.
- Never delete files without user confirmation.

## 14) Cursor / Copilot Rules Presence
- `.cursorrules`: not found.
- `.cursor/rules/`: not found.
- `.github/copilot-instructions.md`: not found.
- If added later, merge their rules into this file section.

## 15) Done Criteria for Any Task
- Tests added first (TDD proof in diff/order).
- `cargo fmt --all` clean.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- Relevant `cargo test` scope green.
- Docs/plan updated when behavior/process changed.
