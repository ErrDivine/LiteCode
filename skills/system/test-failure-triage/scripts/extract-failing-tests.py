#!/usr/bin/env python3
"""Extract common failing test markers from stdin."""

import re
import sys


PATTERNS = [
    re.compile(r"FAILED|failures:|test result:"),
    re.compile(r"thread '.+' panicked at"),
    re.compile(r"^\s*\d+\)\s+"),
    re.compile(r"^\s*---- .+ stdout ----"),
]


def main() -> int:
    for line in sys.stdin:
        if any(pattern.search(line) for pattern in PATTERNS):
            print(line.rstrip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
