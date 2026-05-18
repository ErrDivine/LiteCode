---
name: rust-diagnostic-repair
description: Repair Rust compiler diagnostics and failing tests with small verified patches.
metadata:
  short-description: Rust diagnostics and test repair
---

# Rust Diagnostic Repair

Use this skill when the task is about Rust compiler diagnostics, failing Rust tests, clippy warnings, borrow-checker errors, or small behavior fixes in Rust code.

## Workflow

1. Read the exact diagnostic or failing output before editing.
2. Inspect the smallest relevant source slice with `read_file`, `read_many_files`, `search_files`, `find_files`, and `list_symbols`.
3. Prefer `apply_patch` for localized edits and keep the patch inside the workspace.
4. Run the narrowest useful check first, then broaden to `cargo check --workspace`, `cargo test --workspace`, or `cargo clippy --workspace -- -D warnings` when the change touches shared runtime code.
5. If the fix is risky, explain the risk and stop before widening permissions.

## Package Resources

- `references/diagnostic-workflow.md` gives the repair checklist.
- `scripts/summarize-rust-diagnostics.py` extracts likely Rust diagnostics from copied command output.
