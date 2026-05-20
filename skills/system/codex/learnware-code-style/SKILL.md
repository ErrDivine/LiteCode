---
name: learnware-code-style
description: Restyle only the user's changed files in the Learnware/Learnware-Private repository before commit. Use when Codex is asked to uniform, normalize, clean up, format, reformat, restyle, lint-fix, or prepare Python/docs changes for commit in this repo, especially after local edits and before staging or committing.
---

# Learnware Code Style

## Purpose

Restyle changed files in the Learnware repo without changing behavior. Treat this as a pre-commit cleanup pass: preserve the user's edits, touch only relevant changed files, and verify style with the repo's own conventions.

## First Checks

1. Confirm the repo and dirty state:

```bash
git status --short
git diff --name-only --diff-filter=ACMR
git diff --cached --name-only --diff-filter=ACMR
git ls-files --others --exclude-standard
```

2. Work only on files the user changed or asks to include. Do not restyle the whole repo unless the user explicitly requests that.
3. If `CODE_STYLE.md`, `.pre-commit-config.yaml`, or `docs/about/dev.rst` exist, read them first. The conventions below are the known Learnware conventions and should still guide the cleanup.

## Required Style Convention

Use these rules exactly for this repo:

- Formatter: Black `23.1.0` style with line length `120`.
- Flake8: run with `--ignore=E203,E501,F841,W503`.
- Pre-commit hooks: Black with `-l 120`, Flake8 with the ignore list above.
- Import sorting: developer docs mention `isort learnware --reverse-relative`; apply this idea to changed package files, but avoid running repo-wide import sorting if it would touch unrelated files.
- Python indentation: 4 spaces.
- Keep one newline at end of file.
- Remove trailing whitespace and random blank lines.
- Prefer Black-compatible trailing commas in multi-line calls, dicts, lists, and function signatures.
- Let Black normalize Python string quotes; most formatted code will use double quotes.
- Do not keep unused imports. Past maintenance commits fixed `F401` many times.
- Do not keep invalid f-strings with no placeholders. Past commits fixed `F541`.

## Import Style

Group imports in this order with one blank line between groups:

1. standard library
2. third-party packages
3. local/package imports

Inside the `learnware` package, prefer relative imports when nearby code uses them. Keep optional heavy imports inside functions when top-level imports would make the package fail without optional dependencies, for example `torch`, `lightgbm`, `sentence_transformers`, or Docker-related tools.

## Naming And API Style

- Use `snake_case` for functions, methods, variables, and module files.
- Use `PascalCase` for classes.
- Use clear uppercase names for true constants.
- Use a leading underscore for private helpers and internal attributes.
- Keep public terms consistent with this repo: `Learnware`, `Specification`, `BaseUserInfo`, `Searcher`, `Organizer`, `Checker`, `Reuser`, and `RKME`.

## Docstring And Comment Style

- Use Numpydoc-style docstrings for public classes, public functions, and important methods.
- Common docstring sections are `Parameters`, `Returns`, and `Raises`.
- Keep comments short. Explain why a non-obvious step is needed, not what each simple line does.
- Remove stale commented-out code unless nearby code already uses it as an intentional note.

## Documentation Style

- For `.rst`, keep Python examples inside `.. code-block:: python` indented consistently with nearby docs. Existing docs commonly use 3 spaces under code-block directives.
- For Markdown, use fenced code blocks with language labels when helpful.
- Do not reflow large documentation sections unless the user asks; fix only nearby style issues caused by the changed lines.

## Cleanup Workflow

1. Build the changed-file list. Include staged, unstaged, and untracked files relevant to the user request.
2. For changed Python files, run Black on only those files when possible:

```bash
python -m black -l 120 <changed-python-files>
```

3. If import order is the only issue and `isort` is available, run it only on changed package files, or sort imports manually using the repo grouping rule:

```bash
isort --reverse-relative <changed-python-files-under-learnware>
python -m black -l 120 <same-files>
```

4. Run Flake8 on changed Python files:

```bash
python -m flake8 <changed-python-files> --ignore=E203,E501,F841,W503
```

5. If Flake8 reports real issues, fix them directly. Do not silence them with broad `# noqa` unless the surrounding code already uses that pattern and there is a concrete reason.
6. For docs/examples, inspect the diff manually after formatting and make small formatting edits only where needed.
7. Run focused tests if behavior could have changed. For pure style changes, say that tests were not run unless the user asked.

## Safety Rules

- Never revert user changes to get a cleaner diff.
- Do not stage or commit unless explicitly asked.
- Do not run full-repo formatters on a dirty worktree unless the user asks for repo-wide style cleanup.
- If formatting tools are missing, either install only with user approval or apply the same rules manually.
- Keep the final response short: list changed files, commands run, and any remaining style/test gaps.
