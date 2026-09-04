#!/usr/bin/env python3
"""Validate portable adoption, links, schema parity, source bounds, and both local smokes."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True

SKILL_DIR = Path(__file__).resolve().parents[1]
REPO = Path(__file__).resolve().parents[4]
SCRIPTS = SKILL_DIR / "scripts"
sys.path.insert(0, str(SCRIPTS))

from a2a_contract import COMMON_FIELDS, SCHEMA_VERSION, SHAPES  # noqa: E402
from check_cli_contract import ContractError as CliContractError  # noqa: E402
from check_cli_contract import check_cli_contract  # noqa: E402
from two_agent_smoke import run_smoke  # noqa: E402

GOLDEN = """<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->"""

PACKAGE_ENTRYPOINTS = (
    SKILL_DIR / "SKILL.md",
    REPO / ".claude/skills/nemo-a2a/SKILL.md",
)
PACKAGE_REQUIRED = (
    *PACKAGE_ENTRYPOINTS,
    SKILL_DIR / "VERSION",
    SKILL_DIR / "references/adoption.md",
    SKILL_DIR / "references/commands.md",
    SKILL_DIR / "references/cli-contract-v1.json",
    SKILL_DIR / "references/job-envelope.schema.json",
    SKILL_DIR / "references/protocol.md",
    SKILL_DIR / "references/staging-smoke.md",
    SCRIPTS / "a2a_contract.py",
    SCRIPTS / "check_cli_contract.py",
    SCRIPTS / "executor.py",
    SCRIPTS / "two_agent_smoke.py",
    Path(__file__),
    SKILL_DIR / "tests/test_a2a_contract.py",
    SKILL_DIR / "tests/fixtures/jobs-v1.json",
)


def fail(message: str) -> None:
    raise SystemExit(f"package check failed: {message}")


def relative(path: Path) -> str:
    return str(path.relative_to(REPO))


def tracked_paths() -> set[str]:
    result = subprocess.run(
        ["git", "ls-files"], cwd=REPO, check=True, text=True, capture_output=True
    )
    return set(result.stdout.splitlines())


def check_adoption() -> None:
    tracked = tracked_paths()
    for path in PACKAGE_REQUIRED:
        if not path.is_file():
            fail(f"missing required file {relative(path)}")
        if relative(path) not in tracked:
            fail(f"clean clone would omit {relative(path)}; stage it before validation")
    for path in PACKAGE_ENTRYPOINTS:
        text = path.read_text()
        match = re.search(r"<!-- nemo-golden-rules:start -->.*?<!-- nemo-golden-rules:end -->", text, re.S)
        if not match or match.group(0) != GOLDEN:
            fail(f"golden block differs in {relative(path)}")
        first_body_heading = text.find("# ", text.find("---", 3) + 3) if text.startswith("---") else text.find("# ")
        if match.start() > first_body_heading:
            fail(f"golden block is not the first body section in {relative(path)}")
    if (SKILL_DIR / "VERSION").read_text().strip() != "1.0.0":
        fail("unexpected skill VERSION")
    shim = (REPO / ".claude/skills/nemo-a2a/SKILL.md").read_text()
    shim_target = (REPO / ".claude/skills/nemo-a2a" / "../../../.agents/skills/nemo-a2a/SKILL.md").resolve()
    if "../../../.agents/skills/nemo-a2a/SKILL.md" not in shim or shim_target != (SKILL_DIR / "SKILL.md").resolve():
        fail("Claude shim does not resolve to the canonical tracked skill")


def check_integrated_adoption() -> None:
    tracked = tracked_paths()
    root_agents = REPO / "AGENTS.md"
    claude_path = REPO / "CLAUDE.md"
    ignore_path = REPO / ".gitignore"
    for path in (root_agents, claude_path, ignore_path):
        if not path.is_file() or relative(path) not in tracked:
            fail(f"integrated adoption is missing tracked {relative(path)}")
    text = root_agents.read_text()
    match = re.search(r"<!-- nemo-golden-rules:start -->.*?<!-- nemo-golden-rules:end -->", text, re.S)
    if not match or match.group(0) != GOLDEN or match.start() != 0:
        fail("tracked root AGENTS.md must begin with the exact golden block")
    if ".agents/skills/nemo-a2a/SKILL.md" not in text:
        fail("root AGENTS.md does not trigger the canonical skill")
    claude = claude_path.read_text().splitlines()
    expected_import = "@AGENTS.md"
    if not claude or claude[0] != expected_import:
        fail("CLAUDE.md must directly import the tracked canonical skill")
    import_target = (REPO / claude[0][1:]).resolve()
    if import_target != (REPO / "AGENTS.md").resolve() or not import_target.is_file():
        fail("CLAUDE.md import target is missing or not the tracked root entry point")
    shim = (REPO / ".claude/skills/nemo-a2a/SKILL.md").read_text()
    if "../../../.agents/skills/nemo-a2a/SKILL.md" not in shim:
        fail("Claude shim does not point to the canonical tracked skill")
    ignore = ignore_path.read_text()
    if "__pycache__/" not in ignore or "*.py[cod]" not in ignore:
        fail("integrated .gitignore lacks Python bytecode exclusions")


def check_links_and_sources() -> None:
    if not _contains_private_reference(("/" + "Users/" + "ivg/project")):
        fail("private-path detector regression")
    package_files = [path for path in SKILL_DIR.rglob("*") if path.is_file()]
    for path in package_files:
        if path.stat().st_size > 131_072:
            fail(f"file exceeds 128 KiB: {relative(path)}")
        if path.suffix in {".md", ".py", ".json"}:
            text = path.read_text()
            if len(text.splitlines()) > 650:
                fail(f"file exceeds 650 lines: {relative(path)}")
            if _contains_private_reference(text):
                fail(f"private host/reference detail leaked into {relative(path)}")
        if path.suffix == ".py":
            try:
                compile(path.read_text(), str(path), "exec")
            except SyntaxError as error:
                fail(f"Python syntax error in {relative(path)}: {error}")
    if list(SKILL_DIR.rglob("__pycache__")):
        fail("generated __pycache__ directory is present")
    for markdown in SKILL_DIR.rglob("*.md"):
        text = markdown.read_text()
        for target in re.findall(r"\[[^]]*\]\(([^)]+)\)", text):
            if re.match(r"^[a-z]+://", target) or target.startswith("#"):
                continue
            resolved = (markdown.parent / target.split("#", 1)[0]).resolve()
            try:
                resolved.relative_to(REPO.resolve())
            except ValueError:
                fail(f"link escapes repository in {relative(markdown)}: {target}")
            if not resolved.exists():
                fail(f"dangling link in {relative(markdown)}: {target}")
    for script in re.findall(r"[A-Za-z0-9_./-]+\.py", (SKILL_DIR / "SKILL.md").read_text()):
        candidate = (REPO / script).resolve()
        if not candidate.is_file():
            fail(f"SKILL.md references missing script {script}")


def _contains_private_reference(text: str) -> bool:
    return ("/" + "Users/" + "ivg") in text or ("." + "nemo-bible") in text


def check_schema() -> None:
    schema_path = SKILL_DIR / "references/job-envelope.schema.json"
    try:
        schema = json.loads(schema_path.read_text(), object_pairs_hook=_unique)
    except (ValueError, json.JSONDecodeError) as error:
        fail(f"invalid or duplicate-key schema JSON: {error}")
    properties = set(schema["properties"])
    allowed = set().union(*(required | optional for required, optional in SHAPES.values()))
    if properties != allowed:
        fail(f"schema/Python field drift: schema-only={properties-allowed}, Python-only={allowed-properties}")
    if schema["properties"]["schema_version"].get("const") != SCHEMA_VERSION:
        fail("schema_version drift")
    if set(schema["required"]) != COMMON_FIELDS:
        fail("schema common-field drift")
    if schema["properties"]["coordinator_epoch"].get("maximum") != 4_294_967_295:
        fail("coordinator_epoch is not u32-bounded")
    _resolve_refs(schema, schema)


def _unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate key {key}")
        result[key] = value
    return result


def _resolve_refs(value: object, root: dict[str, object]) -> None:
    if isinstance(value, dict):
        reference = value.get("$ref")
        if isinstance(reference, str) and reference.startswith("#/"):
            target: object = root
            for part in reference[2:].split("/"):
                if not isinstance(target, dict) or part not in target:
                    fail(f"dangling JSON schema reference {reference}")
                target = target[part]
        for nested in value.values():
            _resolve_refs(nested, root)
    elif isinstance(value, list):
        for nested in value:
            _resolve_refs(nested, root)


def check_smokes() -> None:
    for terminal in ("result", "handoff"):
        with tempfile.TemporaryDirectory(prefix=f"nemo-a2a-{terminal}-") as directory:
            result = run_smoke(Path(directory), terminal)
        if not result.get("ok") or result.get("execution_count") != 1:
            fail(f"{terminal} smoke failed: {result}")


def check_cli(binary: Path | None) -> None:
    try:
        check_cli_contract(binary)
    except CliContractError as error:
        fail(str(error))


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-only", action="store_true")
    parser.add_argument("--buzz-bin", type=Path)
    args = parser.parse_args()
    check_adoption()
    if not args.package_only:
        check_integrated_adoption()
    check_links_and_sources()
    check_schema()
    check_cli(args.buzz_bin)
    check_smokes()
    print(json.dumps({
        "ok": True,
        "protocol": "NEMO-A2A-1",
        "skill_version": "1.0.0",
        "mode": "package-only" if args.package_only else "integrated",
        "checks": ["adoption", "golden-block", "links", "schema", "cli-contract", "source-bounds", "result-smoke", "handoff-smoke"],
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
