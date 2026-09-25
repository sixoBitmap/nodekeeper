#!/usr/bin/env python3
"""Example script (docs/SPEC.md item 6): alert if Bitcoin Core hasn't
caught up to the network's own tip. Python stdlib only.

Reads NKP_RPC_URL and NKP_COOKIE_PATH, set by Nodekeeper's script
runner (docs/SPEC.md: "they receive the RPC URL, cookie path, ord
server URL, and NKP_NETWORK"). Uses only `getblockchaininfo`, a purely
local signal (blocks vs headers, initialblockdownload) -- no outside
network call, matching Nodekeeper's own "no telemetry, no outside
calls beyond the pinned few" stance, which a script run through
Nodekeeper should keep to as well.

Usage: python3 alert_node_behind.py [max-blocks-behind]
Exit code 0: caught up. Exit code 1: behind (prints why to stderr) --
suitable for a cron job or Task Scheduler entry that alerts on failure.
"""

import base64
import json
import os
import sys
import urllib.request


def bitcoin_rpc(rpc_url: str, cookie_path: str, method: str, params=None):
    with open(cookie_path, "r", encoding="utf-8") as cookie_file:
        user, password = cookie_file.read().strip().split(":", 1)

    body = json.dumps({"jsonrpc": "1.0", "id": "nodekeeper-script", "method": method, "params": params or []})
    request = urllib.request.Request(rpc_url, data=body.encode("utf-8"), method="POST")
    request.add_header("Content-Type", "application/json")
    credentials = base64.b64encode(f"{user}:{password}".encode("utf-8")).decode("ascii")
    request.add_header("Authorization", f"Basic {credentials}")

    with urllib.request.urlopen(request, timeout=30) as response:
        parsed = json.load(response)
    if parsed.get("error"):
        raise RuntimeError(f"RPC error: {parsed['error']}")
    return parsed["result"]


def main() -> int:
    max_behind = int(sys.argv[1]) if len(sys.argv) > 1 else 2

    rpc_url = os.environ.get("NKP_RPC_URL")
    cookie_path = os.environ.get("NKP_COOKIE_PATH")
    network = os.environ.get("NKP_NETWORK", "unknown")
    if not rpc_url or not cookie_path:
        print("error: NKP_RPC_URL/NKP_COOKIE_PATH not set -- run this through Nodekeeper's script runner", file=sys.stderr)
        return 2

    info = bitcoin_rpc(rpc_url, cookie_path, "getblockchaininfo")
    blocks = info["blocks"]
    headers = info["headers"]
    behind = headers - blocks

    if info.get("initialblockdownload") or behind > max_behind:
        print(
            f"ALERT [{network}]: node is {behind} block(s) behind (blocks={blocks}, headers={headers}, "
            f"initial_block_download={info.get('initialblockdownload')})",
            file=sys.stderr,
        )
        return 1

    print(f"OK [{network}]: caught up at block {blocks}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
