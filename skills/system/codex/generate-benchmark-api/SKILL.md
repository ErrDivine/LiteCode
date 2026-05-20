---
name: generate-benchmark-api
description: Implement or update LearnwareScaling benchmark dataset classes under scaling/benchmark/dataset so they plug into BenchmarkRegistry, evaluation.py, and BenchmarkOrganizer. Use when Codex needs to add a new benchmark, adapt a Hugging Face dataset into the repo's prompt/answer interface, port scoring or normalization from an official benchmark GitHub repo, fix split loading, or adjust filter_fn, process_docs, scoring, or registry wiring for an existing benchmark class.
---

# Generate Benchmark API

## Overview

Implement repo-specific benchmark adapters for LearnwareScaling. Build concrete dataset classes that produce `KEY_PROMPT`, `KEY_GT`, and optionally `KEY_TEST`, register them with `BenchmarkRegistry`, and match the evaluation contract used by `/Users/errdivine/ErrDivine/SeriousResearches/LearnwareScaling/evaluation.py`.

## Workflow

1. Inspect the nearest existing benchmark family before editing:
   - Generative or multiple-choice benchmarks: `general.py`, `medical.py`, `math.py`, `finance.py`
   - Code-execution benchmarks: `code.py`
   - Shared contract: `base.py`, `config.py`, `filter_fns.py`, `evaluation.py`
2. Check whether the benchmark has an official GitHub repo with evaluation code. If it does, clone or update that repo locally outside LearnwareScaling and inspect the stable evaluator before writing code.
3. Read `references/learnware-benchmark-patterns.md` for the repo map, class family patterns, evaluation invariants, and official-repo porting rules.
4. Place the new class in the existing domain module when it matches the file's grouping. Create a new module only when the benchmark family does not fit an existing one, then update `scaling/benchmark/dataset/__init__.py`.
5. Keep the implementation dataset-specific and direct. Do not add extra framework layers, factories, or abstractions that the current repo does not use.

## Implementation Rules

- Start `__init__` with `super().__init__(**kwargs)`. `BenchmarkRegistry.get()` passes `name=...`, so the base initializer must run.
- Define `dataset_path` as a class attribute unless the path must vary by subset.
- Load real dataset splits in `__init__`. If the source dataset uses nonstandard split names such as `validation`, map them explicitly.
- When an official GitHub evaluator exists, port its stable extraction, normalization, scoring, and test-harness behavior instead of re-inventing them. Refactor that logic into LearnwareScaling's interfaces rather than importing the external repo at runtime.
- Convert raw samples with `dataset.map(...)` into repo columns:
  - `KEY_PROMPT`: final model prompt, including any task instruction or guide prompt
  - `KEY_GT`: normalized reference answer for exact-match style scoring
  - `KEY_TEST`: executable tests or structured scoring payload for code tasks
- After `map`, follow the repo pattern that remaps with `load_from_cache_file=False` if expected columns are missing.
- Implement `get_train_dataset()` and `get_test_dataset()` to return the prepared datasets or raise `NotImplementedError` when the benchmark truly has no train split.
- Implement `filter_fn(predictions, samples)` as a batch transform. Return one parsed answer per prediction.
- Reuse repo helpers before writing custom parsing:
  - `regex_filter_fn` for simple extraction
  - `filter_multi_choice` for option selection
  - `extract_code_from_model` for code blocks
- Match scoring to the benchmark's real metric:
  - Use `super().calculate_exact_match_score(...)` when normalized exact match is correct
  - Override `calculate_score()` for code or test-based tasks
  - Add custom normalization only when the benchmark requires it
- Prefer the official benchmark implementation over local convenience helpers whenever the two would differ in observable behavior.
- Add `get_generation_kwargs()` when the family already uses stop sequences or deterministic decoding settings.
- Register the class with `@BenchmarkRegistry.register("benchmark_name")`.

## Decision Guide

- Multiple-choice benchmark:
  - Follow `medical.py` or `general.py`
  - Provide `doc_to_text()` and usually `doc_to_choice()`
  - In `filter_fn`, extract the last line first, then fall back to `filter_multi_choice`
- Short classification benchmark:
  - Follow `finance.py`
  - Keep prompts light, often just `doc["query"]` plus an answer cue
  - Use a small label regex and case normalization
- Free-form math or QA benchmark:
  - Follow `math.py` or `medical.py`
  - Preserve any benchmark-specific normalization logic instead of forcing generic exact match
- Code generation benchmark:
  - Follow `code.py`
  - Store tests in `KEY_TEST`; `DatasetBase.ref_key` will route evaluation to that column
  - Score with execution helpers instead of string exact match
- Benchmark with an official GitHub evaluator:
  - Clone or update the upstream repo first
  - Identify the stable scoring and answer-processing files
  - Port that behavior into the closest family above without adding a dependency on the upstream package

## Validation

- Check that the class is imported through `scaling/benchmark/dataset/__init__.py` if needed.
- Confirm the registered name appears in `BenchmarkRegistry.list_benchmarks()`.
- When an official repo was used, verify the adapted parser and scorer still match the upstream evaluator's behavior on representative examples.
- Smoke-test the class locally with a short Python snippet or the evaluation path if dependencies are available.
- Verify these invariants before finishing:
  - `get_test_dataset()` returns mapped rows with `prompt` and the expected reference column
  - `filter_fn` returns the same number of items as the input predictions
  - `calculate_score()` or `calculate_exact_match_score()` accepts lists and returns a scalar score
  - The class works with `/Users/errdivine/ErrDivine/SeriousResearches/LearnwareScaling/evaluation.py` without extra glue code

## Pitfalls

- Do not rely on the missing `casual_reasoning.py` example mentioned elsewhere; use the benchmark modules that actually exist in this checkout.
- Do not skip the upstream repo review when the benchmark has an official GitHub evaluator; that is the source of truth for benchmark-specific behavior.
- Do not pass raw dataset columns into evaluation without renaming them to `KEY_PROMPT`, `KEY_GT`, and optionally `KEY_TEST`.
- Do not forget that `evaluation.py` passes mapped sample rows into `filter_fn`; parsing logic can use those samples.
- Do not switch code benchmarks back to `GenerativeDataset`; that breaks `ref_key` handling for test-based evaluation.
