#!/usr/bin/env python3
"""Validate the versioned Buzz A2A CLI contract and optional generated help."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
CONTRACT_PATH = SKILL_DIR / "references/cli-contract-v1.json"
COMMANDS_PATH = SKILL_DIR / "references/commands.md"

EXPECTED_PATHS = {
    ("agents", "capabilities"),
    ("jobs", "submit"),
    ("jobs", "list"),
    ("jobs", "get"),
    ("jobs", "accept"),
    ("jobs", "progress"),
    ("jobs", "complete"),
    ("jobs", "fail"),
    ("jobs", "cancel"),
    ("jobs", "release"),
    ("jobs", "handoff"),
}


class ContractError(ValueError):
    """The checked CLI contract is internally inconsistent."""


def _unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, object]:
    try:
        contract = json.loads(path.read_text(), object_pairs_hook=_unique)
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot load CLI contract: {error}") from error
    if not isinstance(contract, dict):
        raise ContractError("CLI contract root must be an object")
    return contract


def command_lines(contract: dict[str, object]) -> list[str]:
    commands = contract.get("commands")
    if not isinstance(commands, list):
        raise ContractError("commands must be an array")
    result: list[str] = []
    seen: set[tuple[str, ...]] = set()
    for command in commands:
        if not isinstance(command, dict):
            raise ContractError("each command must be an object")
        raw_path = command.get("path")
        options = command.get("options")
        if (
            not isinstance(raw_path, list)
            or not raw_path
            or not all(isinstance(part, str) and part for part in raw_path)
            or not isinstance(options, list)
        ):
            raise ContractError("command path/options are malformed")
        path = tuple(raw_path)
        if path in seen:
            raise ContractError(f"duplicate command path: {' '.join(path)}")
        seen.add(path)
        fragments = ["buzz", *path]
        option_names: set[str] = set()
        for option in options:
            if not isinstance(option, dict):
                raise ContractError(f"option for {' '.join(path)} must be an object")
            name = option.get("name")
            value = option.get("value")
            required = option.get("required")
            if (
                not isinstance(name, str)
                or not name.startswith("--")
                or not isinstance(value, str)
                or not value
                or not isinstance(required, bool)
            ):
                raise ContractError(f"malformed option for {' '.join(path)}")
            if name in option_names:
                raise ContractError(f"duplicate option {name} for {' '.join(path)}")
            option_names.add(name)
            rendered = f"{name} {value}"
            fragments.append(rendered if required else f"[{rendered}]")
        result.append(" ".join(fragments))
    if seen != EXPECTED_PATHS:
        raise ContractError(
            f"command set drift: missing={sorted(EXPECTED_PATHS-seen)}, "
            f"extra={sorted(seen-EXPECTED_PATHS)}"
        )
    return result


def validate_contract(contract: dict[str, object], commands_text: str) -> list[str]:
    if contract.get("schema_version") != "buzz.cli-contract.v1":
        raise ContractError("unexpected CLI contract version")
    if contract.get("event_schema_version") != "buzz.jobs.v1":
        raise ContractError("unexpected job event schema version")
    if contract.get("result_schema_version") != "buzz.cli-result.v1":
        raise ContractError("unexpected CLI result schema version")

    lines = command_lines(contract)
    expected_block = "```text\n" + "\n".join(lines) + "\n```"
    if expected_block not in commands_text:
        raise ContractError("commands.md command block differs from the machine contract")

    forbidden = contract.get("forbidden")
    semantics = contract.get("semantics")
    write_result = contract.get("write_result")
    if not isinstance(forbidden, dict) or not isinstance(semantics, dict):
        raise ContractError("forbidden and semantics must be objects")
    if not isinstance(write_result, dict):
        raise ContractError("write_result must be an object")
    if forbidden.get("command_paths") != [["jobs", "control"]]:
        raise ContractError("generic jobs control must remain forbidden")
    if forbidden.get("options") != ["--since"]:
        raise ContractError("--since must remain forbidden")
    if forbidden.get("result_paths") != ["relay.status"]:
        raise ContractError("relay.status must remain forbidden")
    if semantics.get("accept_invocation") != "one_claim_from_input":
        raise ContractError("jobs accept must publish one claim from each input")
    if semantics.get("accept_claim_statuses") != ["processed", "accepted"]:
        raise ContractError("claim status order must be processed then accepted")
    if semantics.get("cursor") != "opaque_relay_history_cursor":
        raise ContractError("jobs list must use an opaque cursor")
    if semantics.get("control_commands") != ["cancel", "release", "handoff"]:
        raise ContractError("control verbs must be separate commands")
    if write_result.get("relay_state") != "stored" or write_result.get("lifecycle", 1) is not None:
        raise ContractError("write receipt must separate relay storage from lifecycle")
    required_paths = write_result.get("required_paths")
    if not isinstance(required_paths, list):
        raise ContractError("write_result.required_paths must be an array")
    if "relay.state" not in required_paths or "relay.status" in required_paths:
        raise ContractError("write receipt field is relay.state")
    community = write_result.get("community_id")
    if community != {
        "source": "authenticated_relay_context",
        "required": True,
        "nullable": False,
    }:
        raise ContractError("community.id must be trusted, required, and non-null")
    return lines


def check_generated_help(binary: Path, contract: dict[str, object]) -> None:
    if not binary.is_file():
        raise ContractError(f"Buzz CLI binary does not exist: {binary}")
    for command, line in zip(contract["commands"], command_lines(contract), strict=True):
        path = command["path"]
        result = subprocess.run(
            [str(binary), *path, "--help"],
            text=True,
            capture_output=True,
            timeout=15,
            check=False,
        )
        if result.returncode != 0:
            raise ContractError(
                f"generated help failed for {' '.join(path)}: {result.stderr.strip()}"
            )
        help_text = result.stdout + result.stderr
        for option in command["options"]:
            if option["name"] not in help_text:
                raise ContractError(
                    f"generated help omits {option['name']} for {' '.join(path)}"
                )
        if "--since" in help_text:
            raise ContractError(f"generated help exposes forbidden --since for {line}")

    forbidden = subprocess.run(
        [str(binary), "jobs", "control", "--help"],
        text=True,
        capture_output=True,
        timeout=15,
        check=False,
    )
    if forbidden.returncode == 0:
        raise ContractError("generated CLI unexpectedly exposes generic jobs control")


def check_cli_contract(binary: Path | None = None) -> dict[str, object]:
    contract = load_contract()
    lines = validate_contract(contract, COMMANDS_PATH.read_text())
    if binary is not None:
        check_generated_help(binary, contract)
    return {
        "ok": True,
        "schema_version": contract["schema_version"],
        "commands": len(lines),
        "generated_help": binary is not None,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--buzz-bin", type=Path)
    args = parser.parse_args()
    try:
        result = check_cli_contract(args.buzz_bin)
    except ContractError as error:
        raise SystemExit(f"CLI contract check failed: {error}") from error
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
