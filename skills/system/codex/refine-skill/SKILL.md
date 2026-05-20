---
name: refine-skill
description: Improve existing Codex skills without rewriting them from scratch. Use when Codex needs to modify or extend an existing skill's `SKILL.md`, YAML frontmatter, `agents/openai.yaml`, or bundled `scripts/`, `references/`, and `assets/` while preserving trigger quality, keeping instructions lean, validating the result, and forward-testing substantial revisions.
---

# Refine Skill

## Overview

Update existing skills surgically. Preserve what already works, widen scope only when the new requests justify it, and keep the final skill easier to trigger, read, and maintain than before the edit.

Borrow the same core standards used for creating strong skills: be concise, match the degree of freedom to the task's fragility, use progressive disclosure, and protect validation integrity.

## Edit Workflow

1. Read the current skill before proposing structure changes.
2. Identify the exact user-facing delta with 2-3 concrete requests the revised skill should handle.
3. Decide the smallest change surface that can support the new behavior.
4. Update resources before instructions when the instructions depend on those resources.
5. Rewrite `SKILL.md` surgically, with special care for the frontmatter description.
6. Reconcile `agents/openai.yaml` if the UI metadata drifted.
7. Validate the skill and forward-test substantial revisions.

Treat each step as required unless there is a clear reason to skip it.

## Inspect Before Editing

Read these files first when they exist:

- `SKILL.md`
- `agents/openai.yaml`
- any bundled resources touched by the requested change

While reading, determine:

- what already triggers well and should stay intact
- what is stale, duplicated, misleading, or too bulky
- whether the request changes the skill's trigger surface, execution workflow, or bundled resources

Do not start from a blank page unless the existing skill is fundamentally unsalvageable.

## Choose the Minimal Change Surface

Map the request to the narrowest set of edits that can solve it:

- change the frontmatter description when the supported tasks or trigger phrases changed
- change the body when the workflow, decision rules, or guidance changed
- add or update `scripts/` when a fragile or repeated procedure needs deterministic help
- add or update `references/` when detailed guidance would otherwise bloat `SKILL.md`
- add or update `assets/` only when the skill needs files to use in output
- update `agents/openai.yaml` when the skill's UI-facing name, summary, or default prompt no longer matches the skill

If the requested scope no longer fits the skill cleanly, prefer creating a sibling skill over stretching the current one into a vague catch-all.

## Editing Rules

### Preserve What Works

- keep the existing skill name and folder path unless the scope truly changed
- retain proven examples, references, and structure when they are still accurate
- prefer patching sections over rewriting them for style alone
- remove stale TODOs, placeholders, and contradictory guidance

### Tighten Triggering First

- treat the frontmatter description as the primary trigger mechanism
- include what the skill does and when to use it in the description
- do not hide trigger guidance only in the body
- only widen triggering when the body and resources actually support the new cases

### Keep Context Lean

- challenge every added paragraph
- move long variant-specific details into `references/`
- keep detailed information in one place instead of duplicating it
- keep `SKILL.md` focused on workflow, decision rules, and resource navigation

### Match the Degree of Freedom

- use high-freedom prose when judgment should stay with Codex
- use lower-freedom checklists, scripts, or concrete procedures when failure is easy or expensive
- add guardrails only where they materially reduce mistakes

### Protect Validation Integrity

- when forward-testing, pass the revised skill and a realistic task, not your diagnosis
- use fresh context for each pass when possible
- prefer raw prompts, outputs, diffs, logs, or artifacts over narrated expectations
- if the skill only works when the test leaks the answer, tighten the skill before trusting it

## File Guidance

### `SKILL.md`

- keep only `name` and `description` in the YAML frontmatter
- write the body in imperative or infinitive form
- keep the body lean and move optional detail into references
- link reference files directly from `SKILL.md`; avoid deep reference chains

### `agents/openai.yaml`

- keep `display_name`, `short_description`, and `default_prompt` aligned with the current skill
- regenerate or update it when the skill meaning changes
- only add optional UI fields if they were explicitly provided

### `scripts/`

- add scripts when the same code is being rewritten repeatedly or reliability matters
- run changed scripts, or at least a representative sample, after editing them
- delete placeholder or stale scripts that no longer support the skill

### `references/`

- store detailed guidance, large examples, schemas, or variant-specific material here
- keep reference files one level away from `SKILL.md`
- add a table of contents near the top when a reference file grows past 100 lines

### `assets/`

- use only for files meant to be consumed in the final output
- do not use assets as a dumping ground for documentation

## Common Edit Patterns

Use [references/edit-patterns.md](references/edit-patterns.md) when you need concrete update patterns, anti-patterns, or a final audit checklist.

Typical cases:

- add a new supported task without disturbing the rest of the skill
- split a bloated `SKILL.md` into lean core guidance plus references
- refresh stale UI metadata after meaningful skill changes
- retire outdated resources and remove every reference to them
- decide whether the request belongs in this skill or should become a new one

## Validation

Run the validator after the edit:

```bash
/Users/errdivine/.codex/skills/.system/skill-creator/scripts/quick_validate.py <path/to/skill-folder>
```

If `agents/openai.yaml` drifted, regenerate or update it so the UI metadata matches the revised skill.

Forward-test when the revision is substantial, touches tricky workflows, or changes triggering behavior. Use realistic requests, review the outputs critically, and iterate until the skill works without hidden context.
