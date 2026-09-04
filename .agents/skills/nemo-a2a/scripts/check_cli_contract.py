#!/usr/bin/env python3
"""Validate Nemo's byte-pinned Buzz CLI contract, docs, fixtures, and help."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
CONTRACT_PATH = SKILL_DIR / "references/buzz-cli-contract-v1.json"
COMMANDS_PATH = SKILL_DIR / "references/commands.md"
HELP_PATH = SKILL_DIR / "tests/fixtures/buzz-cli-help-v1.json"
JOBS_FIXTURE_PATH = SKILL_DIR / "tests/fixtures/jobs-v1.json"

EXPECTED_PATHS = {
    "buzz agents capabilities",
    "buzz jobs submit",
    "buzz jobs list",
    "buzz jobs get",
    "buzz jobs accept",
    "buzz jobs progress",
    "buzz jobs complete",
    "buzz jobs fail",
    "buzz jobs cancel",
    "buzz jobs acknowledge-cancel",
    "buzz jobs release",
    "buzz jobs handoff",
}


class ContractError(ValueError):
    """The checked CLI contract is internally inconsistent or has drifted."""


def _unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _load(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=_unique)
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot load {path.name}: {error}") from error
    if not isinstance(value, dict):
        raise ContractError(f"{path.name} root must be an object")
    return value


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, object]:
    return _load(path)


def command_lines(contract: dict[str, object]) -> list[str]:
    commands = contract.get("commands")
    if not isinstance(commands, list):
        raise ContractError("commands must be an array")
    lines: list[str] = []
    seen: set[str] = set()
    for command in commands:
        if not isinstance(command, dict):
            raise ContractError("each command must be an object")
        path = command.get("path")
        if not isinstance(path, str) or not path.startswith("buzz ") or path in seen:
            raise ContractError(f"invalid or duplicate command path: {path!r}")
        seen.add(path)
        required = command.get("required_options", [])
        optional = command.get("options", [])
        if not all(
            isinstance(values, list)
            and all(isinstance(value, str) and value.startswith("--") for value in values)
            for values in (required, optional)
        ):
            raise ContractError(f"malformed options for {path}")
        rendered = [path, *required, *(f"[{value}]" for value in optional)]
        lines.append(" ".join(rendered))
    if seen != EXPECTED_PATHS:
        raise ContractError(
            f"command set drift: missing={sorted(EXPECTED_PATHS-seen)}, "
            f"extra={sorted(seen-EXPECTED_PATHS)}"
        )
    return lines


def _expect_mapping(parent: dict[str, object], key: str) -> dict[str, object]:
    value = parent.get(key)
    if not isinstance(value, dict):
        raise ContractError(f"{key} must be an object")
    return value


def validate_contract(contract: dict[str, object], commands_text: str) -> list[str]:
    if contract.get("schema_version") != "buzz.cli-contract.v1":
        raise ContractError("unexpected CLI contract version")
    if contract.get("job_schema_version") != "buzz.jobs.v1":
        raise ContractError("unexpected job event schema version")
    if contract.get("result_schema_version") != "buzz.cli-result.v1":
        raise ContractError("unexpected CLI result schema version")
    if contract.get("max_job_ttl_seconds") != 604_800:
        raise ContractError("job TTL cap drift")
    if contract.get("terminal_audit_grace_seconds") != 86_400:
        raise ContractError("terminal audit grace drift")
    if contract.get("coordinator_epoch") != {
        "type": "u32",
        "minimum": 1,
        "maximum": 4_294_967_295,
    }:
        raise ContractError("coordinator epoch contract drift")

    lines = command_lines(contract)
    expected_block = "```text\n" + "\n".join(lines) + "\n```"
    if expected_block not in commands_text:
        raise ContractError("commands.md command block differs from the machine contract")
    flattened = json.dumps(contract, sort_keys=True)
    for forbidden in ("--json", "--since", "buzz jobs control", "relay.status"):
        if forbidden in flattened:
            raise ContractError(f"forbidden legacy CLI surface remains: {forbidden}")

    outputs = _expect_mapping(contract, "outputs")
    community = _expect_mapping(outputs, "community")
    if set(community) != {"schema_version", "community_id", "host", "pubkey"}:
        raise ContractError("authenticated community context shape drift")
    if community.get("schema_version") != "buzz.context.v1":
        raise ContractError("authenticated community context version drift")

    write = _expect_mapping(outputs, "write_result")
    if write.get("schema_version") != "buzz.cli-result.v1":
        raise ContractError("write result version drift")
    if write.get("community") != "community" or write.get("lifecycle", 1) is not None:
        raise ContractError("write result must expose trusted community and null lifecycle")
    if write.get("relay") != {
        "state": "stored",
        "event_id": "lowercase hex event ID",
    }:
        raise ContractError("write result relay receipt drift")
    for key in ("replayed", "event", "authority"):
        if key not in write:
            raise ContractError(f"write result omits {key}")
    authority = _expect_mapping(write, "authority")
    if authority.get("repository_scope") != "unverified" or "note" not in authority:
        raise ContractError("repository authority boundary drift")

    projection = _expect_mapping(outputs, "job_projection")
    if projection.get("event_ids") != "lowercase hex event ID[] in reducer order":
        raise ContractError("projection event ID order drift")
    if projection.get("events") != "verified full signed Nostr event[] in the identical order":
        raise ContractError("projection must expose lossless verified signed events")
    lifecycle = _expect_mapping(projection, "lifecycle")
    states = str(lifecycle.get("state", ""))
    for state in (
        "declined",
        "cancel_requested",
        "cancelled",
        "failed",
        "indeterminate",
    ):
        if state not in states:
            raise ContractError(f"projection omits lifecycle state {state}")

    capabilities = _expect_mapping(outputs, "capabilities_result")
    if "agents" not in capabilities or "advertisements" in capabilities:
        raise ContractError("capability output must use agents")
    semantics = _expect_mapping(contract, "semantics")
    for flag in (
        "json_stdout_is_default",
        "relay_stored_is_not_recipient_delivery",
        "relay_stored_is_not_execution_success",
        "relay_stored_is_not_repository_authority",
    ):
        if semantics.get(flag) is not True:
            raise ContractError(f"required CLI semantic is not true: {flag}")
    if semantics.get("receiver_allowlist_key") != [
        "project.address",
        "project.home_channel",
        "repository.canonical",
    ]:
        raise ContractError("receiver repository allowlist key drift")
    if "opaque" not in str(semantics.get("cursor", "")):
        raise ContractError("list cursor must remain opaque")
    return lines


def check_generated_help(binary: Path) -> None:
    if not binary.is_file():
        raise ContractError(f"Buzz CLI binary does not exist: {binary}")
    fixture = _load(HELP_PATH)
    if fixture.get("schema_version") != "buzz.cli-help-fixture.v1":
        raise ContractError("unexpected help fixture version")
    captures = fixture.get("captures")
    if not isinstance(captures, list) or not captures:
        raise ContractError("help fixture captures must be a non-empty array")
    for capture in captures:
        if not isinstance(capture, dict) or not isinstance(capture.get("argv"), list):
            raise ContractError("malformed help capture")
        result = subprocess.run(
            [str(binary), *capture["argv"]],
            text=True,
            capture_output=True,
            timeout=15,
            check=False,
        )
        actual = {
            "exit_code": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
        }
        expected = {key: capture.get(key) for key in actual}
        if actual != expected:
            raise ContractError(
                f"generated help differs for {' '.join(capture['argv'])}: "
                f"expected={expected!r}, actual={actual!r}"
            )


def check_upstream_bytes(buzz_repo: Path) -> None:
    pairs = (
        (CONTRACT_PATH, buzz_repo / "crates/buzz-cli/cli-contract-v1.json"),
        (JOBS_FIXTURE_PATH, buzz_repo / "crates/buzz-core/tests/fixtures/jobs-v1.json"),
    )
    for local, upstream in pairs:
        try:
            identical = local.read_bytes() == upstream.read_bytes()
        except OSError as error:
            raise ContractError(f"cannot compare {upstream}: {error}") from error
        if not identical:
            raise ContractError(f"byte drift between {local.name} and {upstream}")


def check_cli_contract(
    binary: Path | None = None, buzz_repo: Path | None = None
) -> dict[str, object]:
    contract = load_contract()
    lines = validate_contract(contract, COMMANDS_PATH.read_text())
    if binary is not None:
        check_generated_help(binary)
    if buzz_repo is not None:
        check_upstream_bytes(buzz_repo.resolve())
    return {
        "ok": True,
        "schema_version": contract["schema_version"],
        "commands": len(lines),
        "generated_help": binary is not None,
        "upstream_byte_match": buzz_repo is not None,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--buzz-bin", type=Path)
    parser.add_argument("--buzz-repo", type=Path)
    args = parser.parse_args()
    try:
        result = check_cli_contract(args.buzz_bin, args.buzz_repo)
    except ContractError as error:
        raise SystemExit(f"CLI contract check failed: {error}") from error
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
