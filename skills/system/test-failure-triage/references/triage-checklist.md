# Triage Checklist

Classify the failure first:

- compile or typecheck failure
- assertion mismatch
- fixture or snapshot drift
- filesystem, environment, or path assumption
- async timeout or ordering issue
- missing dependency or unavailable external service

Use the class to pick the smallest next command. Avoid broad checks until the failure has a concrete hypothesis.
