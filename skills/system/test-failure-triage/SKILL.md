---
name: test-failure-triage
description: Inspect failing behavior and choose the smallest useful verification path.
metadata:
  short-description: Test output triage and narrow verification
---

# Test Failure Triage

Use this skill when a command, test, integration flow, or verification step fails and the next move should be a focused diagnosis rather than broad editing.

## Workflow

1. Preserve the failing command and output as evidence.
2. Identify whether the failure is compile-time, runtime, assertion, fixture, environment, or timeout related.
3. Read only the code and tests needed to explain the failure.
4. Prefer the smallest reproducible command before any broad workspace check.
5. Do not edit unless the routed profile and user approval grant write tools.

## Package Resources

- `references/triage-checklist.md` lists the failure categories to check.
- `scripts/extract-failing-tests.py` extracts common failing test names and panic lines from output.
