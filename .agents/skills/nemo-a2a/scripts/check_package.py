#!/usr/bin/env python3
"""Validate portable adoption, links, schema parity, source bounds, and both local smokes."""

from __future__ import annotations

import json
import os
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
from receiver_grants import GRANT_FIELDS  # noqa: E402
from two_agent_smoke import REPOSITORY, run_smoke  # noqa: E402

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
    REPO / ".agents/buzz-preload.json",
    SKILL_DIR / "VERSION",
    SKILL_DIR / "references/adoption.md",
    SKILL_DIR / "references/commands.md",
    SKILL_DIR / "references/buzz-cli-contract-v1.json",
    SKILL_DIR / "references/agent-job-grants.schema.json",
    SKILL_DIR / "references/job-envelope.schema.json",
    SKILL_DIR / "references/protocol.md",
    SKILL_DIR / "references/receiver-grants.md",
    SKILL_DIR / "references/staging-smoke.md",
    SCRIPTS / "a2a_contract.py",
    SCRIPTS / "authorization.py",
    SCRIPTS / "check_cli_contract.py",
    SCRIPTS / "contract_cli.py",
    SCRIPTS / "executor.py",
    SCRIPTS / "local_relay.py",
    SCRIPTS / "receiver_runtime.py",
    SCRIPTS / "receiver_grants.py",
    SCRIPTS / "two_agent_smoke.py",
    Path(__file__).resolve(),
    SKILL_DIR / "tests/test_a2a_contract.py",
    SKILL_DIR / "tests/test_a2a_edges.py",
    SKILL_DIR / "tests/test_authorization.py",
    SKILL_DIR / "tests/fixtures/buzz-cli-help-v1.json",
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
    if (SKILL_DIR / "VERSION").read_text().strip() != "1.3.0":
        fail("unexpected skill VERSION")
    skill = (SKILL_DIR / "SKILL.md").read_text()
    fast_path_contract = {
        "heading": r"## Documentation publication fast path",
        "bounded waits": r"Do not call a wait tool without a live operation or session identifier",
    }
    for requirement, pattern in fast_path_contract.items():
        if not re.search(pattern, skill):
            fail(f"documentation publication fast path is missing: {requirement}")
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
    if "/.buzz/" not in ignore:
        fail("integrated .gitignore does not protect receiver-local Buzz grants")


def check_buzz_preload_manifest() -> None:
    path = REPO / ".agents/buzz-preload.json"
    try:
        manifest = json.loads(path.read_text(), object_pairs_hook=_unique)
    except (ValueError, json.JSONDecodeError) as error:
        fail(f"invalid or duplicate-key Buzz preload manifest: {error}")
    expected = {
        "schema_version": "buzz.project-preload.v2",
        "repository": REPOSITORY,
        "skills": ["nemo-a2a"],
        "policy_resources": [],
    }
    if manifest != expected:
        fail(f"Buzz preload manifest drifted: expected {expected}, got {manifest}")


def check_links_and_sources() -> None:
    if not _contains_private_reference(("/" + "Users/" + "ivg/project")):
        fail("private-path detector regression")
    package_files = [path for path in SKILL_DIR.rglob("*") if path.is_file()]
    for path in package_files:
        if path.stat().st_size > 131_072:
            fail(f"file exceeds 128 KiB: {relative(path)}")
        if path.suffix in {".md", ".py", ".json"}:
            text = path.read_text()
            if path.suffix == ".py":
                limit = 600 if "tests" in path.parts else 500
                if len(text.splitlines()) > limit:
                    fail(f"Python source exceeds {limit} lines: {relative(path)}")
            if _contains_private_reference(text):
                fail(f"private host/reference detail leaked into {relative(path)}")
        if path.suffix == ".py":
            try:
                compile(path.read_text(), str(path), "exec")
            except SyntaxError as error:
                fail(f"Python syntax error in {relative(path)}: {error}")
    if list(SKILL_DIR.rglob("__pycache__")) or any(
        path.suffix in {".pyc", ".pyo"} for path in SKILL_DIR.rglob("*")
    ):
        fail("generated Python bytecode is present")
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


def check_model_security_boundary() -> None:
    combined = "\n".join(
        (SKILL_DIR / relative_path).read_text()
        for relative_path in (
            "SKILL.md",
            "references/commands.md",
            "references/protocol.md",
            "references/staging-smoke.md",
        )
    )
    for required in (
        "buzz_chat_send",
        "buzz_a2a_dispatch",
        "buzz_a2a_inbox",
        "buzz_a2a_status",
        "buzz_a2a_cancel",
        "buzz_a2a_handoff",
        "operator/debug",
        "not a generic signing proxy",
        "POST /api/jobs/authorize",
        "repository_announcement_event_id",
        "process-wide receiver",
        "frozen outbox",
        "checkout_root",
        "path_prefixes",
        "rev-parse --show-toplevel",
        "symbolic-ref",
    ):
        if required not in combined:
            fail(f"model/receiver security contract omits {required!r}")
    for forbidden in (
        "grant_event_id",
        "expected_relay_pubkey",
        "Codex and Claude invoke it with JSON",
        "BUZZ_PRIVATE_KEY",
        "NOSTR_PRIVATE_KEY",
        "BUZZ_AUTH_TAG",
    ):
        if forbidden in combined:
            fail(f"forbidden authorization/model contract remains: {forbidden}")


def check_staging_repository() -> None:
    staging = (SKILL_DIR / "references/staging-smoke.md").read_text()
    coordinate = f"repository.canonical = {REPOSITORY}"
    if staging.count(coordinate) != 1:
        fail("staging repository coordinate drifted from the Nemo repository constant")
    if "repository.canonical = https://github.com/nemo-project/nemo" in staging:
        fail("staging acceptance retains the obsolete Nemo repository coordinate")


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
    definitions = schema.get("$defs", {})
    repository = definitions.get("repository", {}).get("properties", {})
    if repository.get("contracts") != {"$ref": "#/$defs/contractList"}:
        fail("repository contracts are not contract-coordinate-only")
    if repository.get("branch") != {"$ref": "#/$defs/branch"}:
        fail("repository branch is not bound to the conservative ref schema")
    path_pattern = repository.get("paths", {}).get("items", {}).get("pattern", "")
    if "[gG][iI][tT]" not in path_pattern:
        fail("wire path schema does not reject .git case-insensitively")
    project_pattern = (
        definitions.get("project", {}).get("properties", {}).get("address", {}).get("pattern")
    )
    if project_pattern != "^30621:[0-9a-f]{64}:[A-Za-z0-9._-]+$":
        fail("project address portable identifier schema drift")
    if schema["properties"].get("operation_id") != {"$ref": "#/$defs/uuid"}:
        fail("operation UUID is not bound to the canonical non-nil schema")
    _resolve_refs(schema, schema)


def check_grant_schema() -> None:
    schema_path = SKILL_DIR / "references/agent-job-grants.schema.json"
    try:
        schema = json.loads(schema_path.read_text(), object_pairs_hook=_unique)
    except (ValueError, json.JSONDecodeError) as error:
        fail(f"invalid or duplicate-key receiver grant schema JSON: {error}")
    if set(schema.get("required", [])) != {"version", "grants"}:
        fail("receiver grant document fields drifted")
    grant = schema.get("$defs", {}).get("grant", {})
    required = set(grant.get("required", []))
    properties = grant.get("properties", {})
    if required != GRANT_FIELDS or set(properties) != GRANT_FIELDS:
        fail("receiver grant fields drifted from the runtime parser")
    if properties.get("path_prefixes", {}).get("minItems") != 1:
        fail("receiver path_prefixes must be required and nonempty")
    for scalar in ("base_sha", "branch", "worktree_id", "checkout_root"):
        if scalar not in required or properties.get(scalar, {}).get("type") == "array":
            fail(f"receiver grant {scalar} must be a required scalar")
    checkout_pattern = properties.get("checkout_root", {}).get("pattern", "")
    if not checkout_pattern.startswith("^(?:/"):
        fail("receiver checkout_root is not constrained to an absolute native path")
    path_pattern = (
        schema.get("$defs", {}).get("repositoryPath", {}).get("pattern", "")
    )
    if "[gG][iI][tT]" not in path_pattern:
        fail("receiver path schema does not reject .git case-insensitively")
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


def check_tests() -> None:
    result = subprocess.run(
        [sys.executable, "-W", "error::ResourceWarning", "-m", "unittest", "discover", "-s", str(SKILL_DIR / "tests")],
        cwd=REPO,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        text=True,
        capture_output=True,
    )
    if result.returncode:
        fail(f"unit tests failed:\n{result.stdout}{result.stderr}")


def check_cli(binary: Path | None, buzz_repo: Path | None) -> None:
    try:
        check_cli_contract(binary, buzz_repo)
    except CliContractError as error:
        fail(str(error))


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-only", action="store_true")
    parser.add_argument("--buzz-bin", type=Path)
    parser.add_argument("--buzz-repo", type=Path)
    args = parser.parse_args()
    check_adoption()
    if not args.package_only:
        check_integrated_adoption()
    check_links_and_sources()
    check_buzz_preload_manifest()
    check_model_security_boundary()
    check_staging_repository()
    check_schema()
    check_grant_schema()
    check_cli(args.buzz_bin, args.buzz_repo)
    check_tests()
    check_smokes()
    print(json.dumps({
        "ok": True,
        "protocol": "NEMO-A2A-1",
        "skill_version": (SKILL_DIR / "VERSION").read_text().strip(),
        "mode": "package-only" if args.package_only else "integrated",
        "checks": ["adoption", "golden-block", "links", "staging-repository", "schema", "receiver-grant-schema", "cli-contract", "model-security-boundary", "receiver-authorization", "live-checkout", "source-bounds", "unit-tests", "result-smoke", "handoff-smoke"],
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
