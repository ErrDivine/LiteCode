# Learnware Benchmark Patterns

## File Map

- `scaling/benchmark/dataset/base.py`: base classes, `BenchmarkRegistry`, `DatasetBase.ref_key`
- `scaling/benchmark/dataset/config.py`: `KEY_PROMPT`, `KEY_GT`, `KEY_TEST`
- `scaling/benchmark/dataset/filter_fns.py`: shared parsing helpers
- `scaling/benchmark/dataset/general.py`: MMLU-style multiple-choice patterns
- `scaling/benchmark/dataset/medical.py`: medical QA and multiple-choice patterns
- `scaling/benchmark/dataset/math.py`: few-shot math and custom normalization patterns
- `scaling/benchmark/dataset/finance.py`: short classification patterns
- `scaling/benchmark/dataset/code.py`: code-generation and executable-test patterns
- `evaluation.py`: the runtime path that instantiates the benchmark and consumes its outputs

## Shared Contract

### Base Classes

- `BenchmarkRegistry.get(name, **kwargs)` instantiates the benchmark class with `name=name`.
- `DatasetBase.prompt_key` always resolves to `KEY_PROMPT`.
- `DatasetBase.ref_key` resolves to `KEY_TEST` when that column exists in the test dataset; otherwise it resolves to `KEY_GT`.
- `GenerativeDataset.calculate_score()` delegates to `calculate_exact_match_score()` unless overridden.

### Evaluation Expectations

`evaluation.py` assumes all benchmark classes provide:

- `get_test_dataset()` with mapped rows
- `prompt_key` and `ref_key`
- `filter_fn(raw_preds, samples)` returning one parsed answer per sample
- `calculate_score([pred], [ref])` or `calculate_exact_match_score(...)` returning a scalar

For code tasks, the reference passed into scoring is the `KEY_TEST` payload, not `KEY_GT`.

## Official Benchmark Repos

When the benchmark publishes an official GitHub repository with evaluation code, treat that repo as the primary source for scoring, normalization, answer extraction, and test-harness behavior.

### Intake Workflow

1. Find the official repo from the benchmark docs, dataset card, or paper.
2. Clone or update it locally outside the LearnwareScaling repo, preferably under `/tmp` or another scratch path.
3. Prefer the most stable implementation in this order:
   - release tag or version used by the benchmark paper or leaderboard
   - pinned commit referenced by the benchmark maintainers
   - default branch only when no stable version is published
4. Inspect the concrete evaluator entrypoints before coding:
   - scoring or metrics modules
   - answer extraction or normalization helpers
   - prompt templates or formatting helpers
   - test runners or execution harnesses for code tasks
5. Port the observable evaluation behavior into LearnwareScaling's dataset class and scoring hooks. Do not vendor the whole external repo or add a runtime dependency on it.

### What To Preserve

- ground-truth normalization rules
- prediction parsing and final-answer extraction
- metric semantics, including partial credit or aggregation details
- execution payload shape and checker behavior for code benchmarks
- any benchmark-specific prompt formatting that changes evaluation correctness

### What To Refactor

- rewrite the logic into `KEY_PROMPT`, `KEY_GT`, and `KEY_TEST`
- adapt scoring to `calculate_score()` or `calculate_exact_match_score()`
- reuse local helpers only when they preserve the official behavior
- keep the final class compatible with `BenchmarkRegistry` and `evaluation.py`

### Grep Shortcuts For Official Repos

Use these searches after cloning the upstream repo:

```bash
rg -n "eval|metric|score|normalize|extract|prompt|judge|grader|checker|test" <official-repo>
rg -n "main\\(|if __name__ == '__main__'|ArgumentParser|click\\.command" <official-repo>
rg -n "exact_match|f1|accuracy|pass@|pass_at|execution|unit test" <official-repo>
```

## Family Patterns

### Multiple Choice

Use `general.py`, `medical.py`, or `math.py::MATHQA` as the model.

Common structure:

1. Build a task instruction string that forces the answer into a final-line format.
2. Implement `doc_to_text()` to render the question and choices.
3. Optionally implement `doc_to_choice()` so `filter_multi_choice()` can match either letters or option text.
4. Map rows into:
   - `KEY_PROMPT`: instruction prompt + rendered question
   - `KEY_GT`: option letter such as `A`, `B`, `C`, `D`
5. In `filter_fn`, first collapse to the last line, then use `filter_multi_choice()` as the fallback extractor.

Typical scoring:

- `super().calculate_exact_match_score(..., ignore_case=True, ignore_punctuation=True)`

### Classification

Use `finance.py` as the model.

Common structure:

1. Keep the prompt compact, often just `doc["query"]` or `doc["query"] + "Answer: "`.
2. Map `KEY_GT` directly to the label column.
3. Parse the prediction with a narrow regex over the allowed labels.
4. Reuse the base exact-match scorer with case and punctuation ignored.

This family is the cleanest fit for sentiment, stance, or binary-label benchmarks.

### Free-Form QA and Math

Use `math.py::GSM8K`, `math.py::MATH`, or `medical.py::PUBMEDQA` as the model.

Common structure:

1. Decide whether the benchmark needs few-shot context.
2. Normalize the ground truth into the exact answer string that the scorer should see.
3. Implement extraction carefully:
   - numeric answer regex for GSM8K
   - boxed-answer or sentence fallback for MATH
   - fixed-label regex for PUBMEDQA
4. Keep custom scoring only when the benchmark uses a metric stricter or looser than plain string match.

Do not flatten these benchmarks into the finance-style label pattern unless the dataset is genuinely classification-only.

### Code Generation

Use `code.py` as the model.

Common structure:

1. Inherit from `CodeDataset` or follow its `DatasetBase` contract.
2. Build prompts that demand a Python solution enclosed in backticks.
3. Store:
   - `KEY_PROMPT`: code-generation prompt
   - `KEY_GT`: optional reference solution or placeholder
   - `KEY_TEST`: serialized tests or execution payload
4. Use `extract_code_from_model()` inside `filter_fn`.
5. Override `calculate_score()` and execute tests with the existing checker helpers.

Critical invariant:

- If the test payload belongs in scoring, it must be stored under `KEY_TEST`, otherwise `evaluation.py` will compare against the wrong column.

## Split Handling Rules

- Load train and test inside `__init__` when the benchmark has both.
- If the source dataset uses a different split naming scheme, translate it explicitly.
- If the benchmark genuinely has no train set, raise `NotImplementedError` from `get_train_dataset()`.
- If the benchmark is train-only or uses one split with an internal marker column, follow the pattern in `math.py::GSM8K`.

## Mapping Rules

After `dataset.map(...)`, check that the expected repo columns exist. Existing modules repeat the map with `load_from_cache_file=False` when cached artifacts are stale.

The mapped dataset should be directly usable by:

```python
sample = bench.get_test_dataset()[0]
sample[KEY_PROMPT]
sample[bench.ref_key]
```

## Grep Shortcuts

Use these searches when orienting quickly:

```bash
rg -n "@BenchmarkRegistry\\.register|class .*\\(" scaling/benchmark/dataset
rg -n "process_docs|filter_fn|calculate_exact_match_score|calculate_score" scaling/benchmark/dataset
rg -n "prompt_key|ref_key|get_mini_test_dataset" scaling/benchmark/dataset/base.py evaluation.py
```

## Smoke Test Snippet

Use a short local check after implementing a benchmark:

```bash
python - <<'PY'
from scaling.benchmark.dataset import BenchmarkRegistry

bench = BenchmarkRegistry.get("your_benchmark_name")
test_ds = bench.get_test_dataset()
print(test_ds.column_names)
print(test_ds[0][bench.prompt_key][:200])
print(test_ds[0][bench.ref_key])
print(bench.filter_fn(["dummy answer"], [test_ds[0]]))
PY
```

If the dataset requires unavailable remote dependencies, at least verify the class wiring, imports, and column names against the source schema before finishing.

## Known Repo Quirk

`scaling/benchmark/dataset/__init__.py` imports `casual_reasoning.FOLIO`, but `casual_reasoning.py` is not present in this checkout. Do not use that missing file as the reference implementation for new work.
