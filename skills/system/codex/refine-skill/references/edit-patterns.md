# Skill Edit Patterns

Use this reference when the requested change is large enough that a simple local patch feels ambiguous.

## Table of Contents

- Patch vs. rewrite
- Common update patterns
- Anti-patterns
- Final audit checklist

## Patch vs. Rewrite

Patch by default. Rewrite only when one of these is true:

- the frontmatter description no longer matches the actual skill
- the body structure actively blocks the new workflow
- the skill is dominated by scaffolding, stale content, or contradictions
- the skill tries to cover multiple unrelated jobs and needs to be split

Even when rewriting a section, preserve any examples, scripts, references, or phrases that are already proven and still accurate.

## Common Update Patterns

### Add a Supported Task

1. Expand the frontmatter description so the new task can trigger the skill.
2. Add only the workflow guidance needed for the new task.
3. Add supporting scripts or references if the task cannot be executed reliably from prose alone.
4. Check whether examples, `default_prompt`, or `short_description` should change.

### Reduce Bloat

1. Identify paragraphs that restate what Codex already knows.
2. Move detailed or variant-specific material into `references/`.
3. Replace copied detail in `SKILL.md` with a short routing sentence and a link.
4. Delete duplicate guidance rather than leaving both versions in place.

### Repair Trigger Drift

1. Compare the frontmatter description against the actual body and resources.
2. Remove unsupported trigger phrases.
3. Add missing trigger phrases only when the rest of the skill supports them.
4. Recheck `agents/openai.yaml` for stale UI language.

### Add Determinism

1. Notice repeated or fragile steps that rely on hand-written code each time.
2. Move that logic into `scripts/` if it is likely to recur.
3. Keep `SKILL.md` focused on when and how to use the script, not the entire implementation.
4. Run the changed script after editing it.

### Split Scope

Create a new sibling skill instead of extending the current one when:

- the new task has different triggers and a different workflow
- the combined description becomes too broad to trigger precisely
- the bundled resources have little overlap
- the user is really asking for a distinct toolset, not an incremental extension

## Anti-Patterns

- rewriting the entire skill just to impose a preferred style
- keeping trigger guidance only in a "When to Use" section in the body
- copying long theory from another skill without adapting it to the current task
- duplicating the same guidance in `SKILL.md` and a reference file
- leaving placeholder resources or stale TODOs after the edit
- adding README-like documentation that the runtime never needs
- expanding scope without updating examples, resources, or metadata

## Final Audit Checklist

- Does the frontmatter description accurately describe what the skill does and when to use it?
- Does every new trigger phrase correspond to real instructions or resources?
- Is `SKILL.md` lean enough that optional detail lives in references instead?
- Do all referenced files still exist, and are all stale files removed?
- Does `agents/openai.yaml` still match the skill?
- Were changed scripts actually run?
- Did validation pass?
- If the revision was substantial, was it forward-tested with a realistic task?
