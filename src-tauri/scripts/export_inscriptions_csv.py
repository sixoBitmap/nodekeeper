#!/usr/bin/env python3
"""Example script (docs/SPEC.md item 6): export every inscription held
by an address to a CSV file, using only ord's local HTTP JSON API --
no ord CLI, no third-party packages, Python stdlib only.

Reads NKP_ORD_URL, set by Nodekeeper's script runner. Everything else
is a plain command-line argument, since a script has no access to
Nodekeeper's own wallet state -- only the environment it's told to run
against (docs/SPEC.md: "they receive the RPC URL, cookie path, ord
server URL, and NKP_NETWORK").

Usage: python3 export_inscriptions_csv.py <address> [output.csv]
"""

import csv
import json
import os
import sys
import urllib.request


def ord_get(base_url: str, path: str):
    request = urllib.request.Request(
        base_url.rstrip("/") + path,
        headers={"Accept": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: export_inscriptions_csv.py <address> [output.csv]", file=sys.stderr)
        return 2

    address = sys.argv[1]
    output_path = sys.argv[2] if len(sys.argv) > 2 else "inscriptions.csv"

    ord_url = os.environ.get("NKP_ORD_URL")
    if not ord_url:
        print("error: NKP_ORD_URL is not set -- run this through Nodekeeper's script runner", file=sys.stderr)
        return 1

    address_info = ord_get(ord_url, f"/address/{address}")
    inscription_ids = address_info.get("inscriptions", [])
    print(f"{address}: {len(inscription_ids)} inscription(s)")

    rows = []
    for inscription_id in inscription_ids:
        detail = ord_get(ord_url, f"/inscription/{inscription_id}")
        rows.append(
            {
                "id": detail.get("id"),
                "number": detail.get("number"),
                "content_type": detail.get("content_type"),
                "content_length": detail.get("content_length"),
                "sat": detail.get("sat"),
                "satpoint": detail.get("satpoint"),
                "timestamp": detail.get("timestamp"),
            }
        )

    with open(output_path, "w", newline="", encoding="utf-8") as csv_file:
        writer = csv.DictWriter(
            csv_file,
            fieldnames=["id", "number", "content_type", "content_length", "sat", "satpoint", "timestamp"],
        )
        writer.writeheader()
        writer.writerows(rows)

    print(f"wrote {len(rows)} row(s) to {output_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
