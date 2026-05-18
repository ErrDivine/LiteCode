#!/usr/bin/env python3
"""Count file extensions from paths provided on stdin."""

import collections
import pathlib
import sys


def main() -> int:
    counts = collections.Counter()
    for raw in sys.stdin:
        path = pathlib.Path(raw.strip())
        if not path.name:
            continue
        suffix = path.suffix or "<none>"
        counts[suffix] += 1
    for suffix, count in counts.most_common():
        print(f"{suffix}\t{count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
