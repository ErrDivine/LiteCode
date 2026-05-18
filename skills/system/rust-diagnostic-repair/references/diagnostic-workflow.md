# Diagnostic Workflow

Start from the real diagnostic text. Identify the primary file, line, error code, and the smallest owner module. Read neighboring code before editing.

Prefer fixes that preserve public APIs unless the diagnostic proves the API is wrong. After patching, run the narrowest command that exercises the changed code and then broaden verification if the modified module is shared.
