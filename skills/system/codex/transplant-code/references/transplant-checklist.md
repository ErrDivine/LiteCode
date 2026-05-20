# Transplant Checklist

## Contents

- Placement matrix
- Copy-set audit
- Minimal adaptation patterns
- Validation exit criteria
- Red flags

## Placement Matrix

| Target shape | Use when | Avoid when |
| --- | --- | --- |
| Existing crate or package | Ownership already matches, runtime and dependency model align, and the move does not force unstable API growth | The move creates cycles, drags in heavy optional dependencies, or pollutes a stable boundary |
| New crate or package | The code is cohesive, reusable, dependency-heavy, optional, or easier to test and evolve in isolation | The code is tiny, single-use, or would over-fragment the repository |
| Normal source file | The transplant is local to one feature and does not need package-level identity | The code has multiple consumers, special build wiring, or likely reuse |

## Copy-Set Audit

Check for all behavior-bearing inputs before editing:

- source files and helper modules
- tests, fixtures, snapshots, and sample inputs
- manifests, dependency declarations, and feature flags
- macros, proc macros, generated sources, codegen inputs, and `build.rs`
- schemas, SQL, migrations, templates, assets, and embedded files
- config files, env vars, path assumptions, and runtime selection
- docs, comments, and examples that define observable behavior
- license headers, attribution, and provenance requirements

## Minimal Adaptation Patterns

Prefer these in order:

1. Outer adapter that maps target calls into donor interfaces
2. Facade module that preserves the target's public shape
3. Newtype or translator for incompatible data models
4. Thin trait or interface implementation that plugs donor logic into host abstractions
5. Re-export or namespace shim for stable call sites

If these are not enough, verify that the task is still a transplant rather than a rewrite.

## Validation Exit Criteria

Require all of the following before calling the transplant complete:

- target build passes
- donor tests or equivalent behavior checks pass
- no accidental public API breakage unless explicitly approved
- no new dependency cycles
- runtime, platform, and configuration assumptions are documented
- intentional deviations from donor behavior are called out

## Red Flags

Pause and reassess when you find any of these:

- async runtime mismatch
- hidden globals or singleton state
- platform-specific or `unsafe` code
- serialization or schema mismatch
- file path or working-directory assumptions
- hidden code generation or build steps
- compile-feature interactions
- large performance sensitivity at the integration boundary
- changes to core logic spreading beyond the smallest necessary seam
