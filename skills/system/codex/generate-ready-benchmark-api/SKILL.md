---
name: generate-ready-benchmark-api
description: Implement or update ANCHOR inference-ready benchmark dataset classes under ready_datasets so they subclass InferenceReadyDataset, register with BenchmarkRegistry, map Hugging Face rows into prompt/reference columns, and evaluate single model outputs. Use when Codex needs to add a benchmark, adapt a Hugging Face dataset to ready_datasets/base.py, fix prompt/reference construction, add output filtering or answer scoring, or decide whether helper logic belongs in a subclass file or ready_datasets/common_utils.py.
---

# Generate Ready Benchmark API (from ANCHOR)

## Overview

Implement repo-specific benchmark adapters for `/Users/errdivine/ErrDivine/SeriousResearches/LearnwareAnchorEmpirical/ready_datasets`. Match the contract in `ready_datasets/base.py`: the base class loads splits, maps rows, exposes `prompt` and `reference`, and evaluates one model output at a time.

This skill is for ANCHOR's `InferenceReadyDataset`, not the older LearnwareScaling `DatasetBase` API.

## Workflow

1. Inspect the current contract before editing:
   - Shared base: `ready_datasets/base.py`
   - Shared helpers: `ready_datasets/common_utils.py`
   - Registry exports: `ready_datasets/__init__.py`
   - Tests: `tests/test_ready_datasets.py`
2. Inspect the nearest existing benchmark class under `ready_datasets` when one exists. Follow its prompt style, answer format, imports, and registration pattern.
3. Check whether the benchmark has an official evaluator or clear answer-normalization rules. Port stable filtering and scoring behavior into this repo instead of making up new rules.
4. Add one dataset module per benchmark unless the repo already groups a family differently.
5. Update `ready_datasets/__init__.py` so the new class is imported and included in `__all__`.
6. Keep the class small. If the change needs new framework behavior, reconsider whether `InferenceReadyDataset` itself should change before adding subclass workarounds.

## Base Class Contract

- Subclass `InferenceReadyDataset`.
- Register the class with `@BenchmarkRegistry.register("benchmark_name")`.
- In `__init__`, call `super().__init__(...)` once and pass the dataset identity and split parameters there:

```python
@BenchmarkRegistry.register("example")
class Example(InferenceReadyDataset):
    def __init__(self, name):
        super().__init__(
            name=name,
            dataset_path="org/dataset",
            subset_name=None,
            train_split_name="train",
            test_split_name="test",
        )
```

- Let the base class fill private fields such as `_name`, `_dataset_path`, `_subset_name`, `_train_split_name`, `_test_split_name`, and `_auto_train_test_from_split`.
- Do not manually assign those private fields in subclasses after calling `super().__init__`.
- Do not load Hugging Face datasets in subclasses when the base class can load them from the initializer parameters.
- Implement only the abstract behavior required by the base class:
  - `_build_prompt(self, item: dict)`
  - `_build_ref(self, item: dict)`
  - `_filter_single_output(self, output: str) -> str`
  - `_evaluate_answer_with_reference(self, filtered, reference) -> Union[float, bool]`
- Do not override `get_train_dataset`, `get_test_dataset`, `evaluate_single_output`, `_ready_train_test_dataset`, or `_ready_item` unless the user explicitly asks to change the base contract.
- Use `auto_train_test_from_split` only after confirming `ready_datasets/base.py` supports the needed split behavior. If the current base does not support it, update the base deliberately or use explicit train/test split names.

## Helper Placement

- Put helper logic in the subclass file only when it is specific to that benchmark and mainly makes the prompt, reference, filter, or scoring code easier to read.
- Prefer module-level private helper functions in the subclass file when the helper does not need object state.
- Put helper logic in `ready_datasets/common_utils.py` when several benchmark classes call it or are likely to call it soon.
- Reuse helpers from `common_utils.py` before writing another parser or normalizer.
- Do not move class-specific rules into `common_utils.py`; shared helpers should stay general enough to be reused without benchmark-specific names baked in.

## Implementation Rules

- Build `_build_prompt` from the raw dataset row and return the final inference prompt string.
- Build `_build_ref` from the raw dataset row and return the normalized reference used by evaluation.
- Keep the output columns named by the base class: `prompt` and `reference`. Do not use `KEY_PROMPT`, `KEY_GT`, or `KEY_TEST` in this repo.
- Implement `_filter_single_output` for one raw model output. Return one filtered answer string, not a list.
- Implement `_evaluate_answer_with_reference` for one filtered answer and one reference. Return `True` or `False` for exact checks, or a float when the benchmark metric needs partial credit.
- Normalize answers before comparing when the benchmark expects case-insensitive, punctuation-insensitive, numeric, or option-letter matching.
- Keep prompts plain and predictable. Include choices in a stable order for multiple-choice tasks and make the expected answer format clear.
- Prefer official benchmark normalization and scoring rules over local convenience when they differ.
- Keep imports simple and local to the repo. Do not add a runtime dependency on an official benchmark repository just for scoring.

## Decision Guide

- Multiple-choice benchmark:
  - Store the reference as the answer label such as `A`, `B`, `C`, or `D`.
  - Include labeled options in the prompt.
  - In `_filter_single_output`, accept common formats such as `A`, `(A)`, `Answer: A`, and `The final answer is A.`
- Yes/no or short classification benchmark:
  - Store a small normalized label such as `yes`, `no`, or a dataset-defined class.
  - Filter with a small regex or shared helper, then compare normalized strings.
- Free-form QA benchmark:
  - Preserve official normalization if available.
  - Use exact match only when the official metric is exact match.
- Numeric or math benchmark:
  - Normalize number formatting carefully.
  - Use a float tolerance only if the benchmark rules allow it.
- Benchmark with an official evaluator:
  - Inspect the upstream scoring and answer-processing files first.
  - Port only the stable behavior needed for `_filter_single_output` and `_evaluate_answer_with_reference`.

## Validation

- Confirm the class is imported through `ready_datasets/__init__.py`.
- Confirm the registered name appears in `BenchmarkRegistry.list_benchmarks()`.
- Instantiate through `BenchmarkRegistry.get("benchmark_name")`, not by calling the class directly.
- Smoke-test that `get_test_dataset()[0]` includes `prompt` and `reference`.
- Check `_filter_single_output` on realistic model outputs.
- Check `evaluate_single_output(output, reference)` returns a boolean or float.
- Run focused tests such as:

```bash
pytest tests/test_ready_datasets.py
```

## Pitfalls

- Do not copy the old LearnwareScaling adapter shape into this repo. There is no batch `filter_fn`, `calculate_score`, `process_docs`, `KEY_PROMPT`, `KEY_GT`, or `KEY_TEST` contract here.
- Do not hide split loading inside subclasses when `super().__init__` can describe the dataset path, subset, and splits.
- Do not duplicate the same option parser or answer normalizer across many benchmark files; move it to `common_utils.py`.
- Do not make `common_utils.py` a dumping ground for one-off benchmark rules.
- Do not skip `ready_datasets/__init__.py`; unimported modules will not register their classes.
