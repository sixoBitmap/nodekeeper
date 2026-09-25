#!/usr/bin/env python3
"""Example script (docs/SPEC.md item 6): a daily disk-usage report for
this environment's data directory. Python stdlib only.

Derives the data directory from NKP_COOKIE_PATH (set by Nodekeeper's
script runner -- the cookie file always lives at
"<environment-data-dir>/.cookie", so its parent directory is exactly
what to measure) rather than needing a fifth env var just for this.

Usage: python3 disk_usage_report.py
"""

import os
import shutil
import sys
from datetime import datetime, timezone


def human_bytes(n: float) -> str:
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if n < 1024 or unit == "TB":
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"


def dir_size_bytes(path: str) -> int:
    total = 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(root, name))
            except OSError:
                pass  # a file can vanish mid-walk (log rotation, etc.) -- skip it, don't fail the report
    return total


def main() -> int:
    cookie_path = os.environ.get("NKP_COOKIE_PATH")
    network = os.environ.get("NKP_NETWORK", "unknown")
    if not cookie_path:
        print("error: NKP_COOKIE_PATH is not set -- run this through Nodekeeper's script runner", file=sys.stderr)
        return 1

    data_dir = os.path.dirname(cookie_path)
    if not os.path.isdir(data_dir):
        print(f"error: {data_dir} is not a directory", file=sys.stderr)
        return 1

    used_by_environment = dir_size_bytes(data_dir)
    total, _used_on_volume, free_on_volume = shutil.disk_usage(data_dir)

    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    print(f"Disk usage report [{network}] -- {timestamp}")
    print(f"  Data directory:         {data_dir}")
    print(f"  Used by this environment: {human_bytes(used_by_environment)}")
    print(f"  Free on this volume:      {human_bytes(free_on_volume)}")
    print(f"  Total volume size:        {human_bytes(total)}")

    percent_free = (free_on_volume / total * 100) if total else 0
    if percent_free < 10:
        print(f"WARNING: only {percent_free:.1f}% free on this volume", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
