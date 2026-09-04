#!/usr/bin/env python3
"""Command-line entry point for the local jobs.v1 contract mirror."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from a2a_contract import (
    ContractError,
    loads_unique,
    parse_event,
    semantic_digest,
    validate_content,
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate-event", "digest-request"))
    parser.add_argument("input", nargs="?", default="-")
    args = parser.parse_args()
    raw = sys.stdin.read() if args.input == "-" else Path(args.input).read_text()
    try:
        if args.command == "validate-event":
            event = parse_event(raw)
            output = {"ok": True, "kind": event["kind"], "event_id": event["id"]}
        else:
            content = loads_unique(raw)
            validate_content(43001, content)
            output = {"ok": True, "scope_digest": semantic_digest(content)}
    except ContractError as error:
        print(json.dumps({"ok": False, "error": str(error)}))
        return 1
    print(json.dumps(output, sort_keys=True))
    return 0
