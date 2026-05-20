---
name: transplant-code
description: Transplant existing code from one project into another with a copy-first, stability-preserving workflow. Use when Codex needs to move a feature, module, crate, package, or source file across repositories or directories; decide whether it belongs in an existing crate/package, a new crate/package, or a normal source file; adapt interfaces with the least possible modification; preserve the target's public API and layering rules; and carry over the tests, configuration, and dependencies needed to keep behavior intact.
---

# Transplant Code

## Overview

Transplant code conservatively. Copy the relevant files first, preserve the donor logic, and then decide the smallest integration shape in the target project.

Favor adapting seams over rewriting internals. Interface code usually changes more often than core logic, so keep the core recognizable and put most adaptation pressure on wrappers, translators, and host-side glue.

## Core Workflow

1. Bound the transplant.
2. Copy the relevant files before redesigning them.
3. Decide placement only after the copied slice exists in the target.
4. Adapt the boundary with the least possible modification.
5. Reconcile dependencies, build wiring, and runtime assumptions.
6. Port tests and verify behavior before cleanup.

Treat the sequence as required unless there is a clear reason to skip a step.

## Bound the Transplant

- Identify the exact behavior to preserve, not just the headline file to move.
- Find the minimal working slice: source files, helper modules, types, constants, macros, generated files, manifests, fixtures, templates, migrations, configuration, and tests that define observable behavior.
- Separate core logic from edge adapters early. Expect adapters to change more than the core.
- If the move crosses languages or incompatible runtimes, classify it as a rewrite or port rather than a transplant.

## Copy Before Redesign

- Copy the relevant files into the target project before folding them into local abstractions.
- Preserve names, comments, tests, and internal structure during the first pass unless the target build requires small path, visibility, or namespace fixes.
- Keep the transplanted version easy to diff against the donor. The first target-side snapshot should still resemble the source.
- Do not start by making the code more idiomatic for the target repository. First make it present and runnable.

## Decide Placement After the Copy

- Place code into an existing crate, package, or module when it clearly matches an existing ownership boundary and can fit without creating cycles or widening a stable public API unnecessarily.
- Place code into a new crate or package when it is cohesive, reusable, dependency-heavy, optional, or awkward inside the current package graph.
- Place code into a normal source file when the transplant is small, local to one feature, and not worth a new package boundary.
- In Rust repositories, explicitly compare three options: existing crate, new crate, or plain `src/*.rs` placement. In other languages, map the same decision to the local package and module system.

Use [references/transplant-checklist.md](references/transplant-checklist.md) when the placement decision is unclear or the transplant touches public APIs, heavy dependencies, or build tooling.

## Adapt the Seam, Not the Core

- Prefer wrapper functions, facade modules, trait implementations, newtypes, translators, shims, dependency inversion, and re-exports.
- Keep donor logic recognizable. Change the interface layer first.
- Prefer compatibility adapters over signature churn inside the transplanted core.
- Make the minimum changes required for naming, visibility, lifetimes, ownership, error translation, async integration, and data-shape conversion.
- If a transplant only works after substantial changes to the core logic, stop and reassess whether the task is actually extraction, redesign, or reimplementation.

## Reconcile Integration Requirements

- Bring over direct dependencies before pruning anything.
- Check manifests, lockfiles when relevant, feature flags, proc macros, generated code, build hooks, and environment assumptions.
- Carry over behavior-bearing assets such as templates, schemas, SQL, migrations, lookup tables, prompt files, or embedded resources.
- Align error types, logging, metrics, serialization, and concurrency behavior at the boundary rather than forcing the core to absorb target-specific concerns.
- Preserve license headers, attribution, and provenance requirements when the donor code requires them.
- Avoid leaking new dependencies or donor-specific types across stable public boundaries unless the user explicitly asked for an API change.

## Port Tests With the Code

- Copy donor tests, fixtures, and snapshots when they encode the expected behavior.
- Add host-side integration tests only after the transplanted slice compiles in the target.
- Use the transplanted tests to justify later cleanup and deduplication.
- If tests cannot move directly, recreate the narrowest equivalent checks before further refactoring.

## Clean Up Only After Behavior Matches

- Remove dead compatibility code only after the transplant builds, tests, and fits the target architecture.
- Document intentional deviations from the donor implementation.
- Keep refactors separate from the initial transplant when possible so failures remain attributable.
- Prefer a follow-up cleanup pass over mixing integration and redesign into one change set.

## Default Decision Rules

- Prefer existing ownership boundaries over creating new ones without evidence.
- Prefer a new crate or package over contaminating a stable crate with heavy or optional dependencies.
- Prefer a plain source file over a new crate when the code is small and clearly local.
- Prefer boundary adapters over core rewrites.
- Prefer carrying over too much supporting context initially over silently dropping a hidden dependency.
- Prefer preserving the target's public API over exposing donor shapes directly.

## Avoid

- Do not rewrite the core first just to match local style.
- Do not drop tests, fixtures, or config because the target compiles without them.
- Do not merge donor code deeply into existing abstractions before proving the transplant works.
- Do not accept cyclic dependencies or package graph contortions to avoid creating a clean boundary.
- Do not widen a public API accidentally just because internal wiring became easier.
