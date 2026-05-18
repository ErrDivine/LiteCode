---
name: repo-explainer
description: Explain repository structure and code relationships without editing.
metadata:
  short-description: Repository structure and code relationship explanation
---

# Repo Explainer

Use this skill when the user asks how the project is organized, how modules interact, or where a behavior lives.

## Workflow

1. Start from `find_files`, `list_directory`, `search_files`, and `list_symbols`.
2. Read the specific files that establish the relationship being explained.
3. Prefer concrete file and type/function references over generic architecture language.
4. Avoid edits, shell commands with side effects, and speculative conclusions.

## Package Resources

- `references/repo-map.md` provides a repeatable repository mapping flow.
- `scripts/file-type-summary.py` summarizes file extensions from a file list or stdin.
