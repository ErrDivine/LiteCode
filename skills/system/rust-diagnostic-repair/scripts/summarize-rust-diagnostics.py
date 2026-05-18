#!/usr/bin/env python3
"""Extract compact Rust diagnostic lines from stdin."""

import re
import sys


PATTERNS = [
    re.compile(r"error(?:\[[A-Z0-9]+\])?:"),
    re.compile(r"warning:"),
    re.compile(r"\s+-->\s+.+:\d+:\d+"),
    re.compile(r"thread '.+' panicked at"),
    re.compile(r"failures:"),
]


def main() -> int:
    for line in sys.stdin:
        if any(pattern.search(line) for pattern in PATTERNS):
            print(line.rstrip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
