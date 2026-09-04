#!/usr/bin/env python3
"""Strict local mirror of the Buzz jobs.v1 envelope and relay safety door.

This module deliberately does not implement Nostr signing. ``freeze_event`` creates a
deterministic local event record so the smoke test can prove byte-for-byte receipt replay.
Live cryptographic and relay acceptance belongs to Buzz staging.
"""

from __future__ import annotations

import collections
import datetime as dt
import hashlib
import json
import re
import unicodedata
import uuid
from urllib.parse import urlsplit

SCHEMA_VERSION = "buzz.jobs.v1"
PROTOCOL_VERSION = "NEMO-A2A-1"
MAX_JOB_TTL_SECONDS = 604_800
JOB_TERMINAL_AUDIT_GRACE_SECONDS = 86_400
KINDS = {
    43001: "request",
    43002: "claim",
    43003: "progress",
    43004: "result",
    43005: "control",
    43006: "error",
}
COMMON_FIELDS = {
    "schema_version",
    "operation_id",
    "idempotency_key",
    "coordinator_epoch",
    "project",
    "repository",
    "sender_pubkey",
    "recipient_pubkey",
    "sponsor",
    "expires_at",
}
SHAPES = {
    43001: (COMMON_FIELDS | {"capability", "summary", "acceptance"}, {"supersedes_event_id"}),
    43002: (COMMON_FIELDS | {"request_event_id", "claim"}, {"prior_event_id"}),
    43003: (COMMON_FIELDS | {"request_event_id", "prior_event_id", "status", "message", "evidence"}, set()),
    43004: (
        COMMON_FIELDS | {"request_event_id", "prior_event_id", "outcome", "artifacts", "evidence"},
        {"candidate_sha", "capabilities"},
    ),
    43005: (COMMON_FIELDS | {"request_event_id", "action", "reason"}, {"prior_event_id", "handoff_to"}),
    43006: (
        COMMON_FIELDS | {"request_event_id", "prior_event_id", "outcome", "code", "message", "retryable"},
        set(),
    ),
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SHA = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
PROJECT = re.compile(r"^30621:([0-9a-f]{64}):([A-Za-z0-9._-]{1,512})$")
WORKTREE = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")
IDEMPOTENCY = re.compile(r"^[!-~]{1,128}$")
POSITIVE_DECIMAL = re.compile(r"^[1-9][0-9]{0,19}$")
SECRET = re.compile(
    r"(?:-----BEGIN [A-Z ]*PRIVATE KEY-----|(?:github_pat_|ghp_|sk-)[A-Za-z0-9]{12})",
    re.IGNORECASE,
)
SECRET_MARKERS = (
    "token=",
    "password=",
    "secret=",
    "authorization:",
    "bearer ",
    "begin private key",
    "github_token",
    "api_key",
)


class ContractError(ValueError):
    """A strict wire, authorization, or lifecycle failure."""


class IdempotencyConflict(ContractError):
    """The same authority-domain key was reused for changed semantics."""


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def loads_unique(value: str) -> object:
    try:
        return json.loads(value, object_pairs_hook=_reject_duplicate_keys)
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid JSON: {error}") from error


def canonical_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def semantic_digest(content: dict[str, object]) -> str:
    return hashlib.sha256(canonical_json(content).encode()).hexdigest()


def _no_null(value: object) -> None:
    if value is None:
        raise ContractError("null is forbidden; omit optional fields")
    if isinstance(value, dict):
        for nested in value.values():
            _no_null(nested)
    elif isinstance(value, list):
        for nested in value:
            _no_null(nested)


def _no_secrets(value: object) -> None:
    if isinstance(value, str):
        lower = value.lower()
        if SECRET.search(value) or any(marker in lower for marker in SECRET_MARKERS):
            raise ContractError("signed job content must not contain credential material")
    if isinstance(value, dict):
        for nested in value.values():
            _no_secrets(nested)
    elif isinstance(value, list):
        for nested in value:
            _no_secrets(nested)


def _exact_keys(name: str, value: object, required: set[str], optional: set[str] = set()) -> dict:
    if not isinstance(value, dict):
        raise ContractError(f"{name} must be an object")
    keys = set(value)
    missing = required - keys
    unknown = keys - required - optional
    if missing:
        raise ContractError(f"{name} missing fields: {sorted(missing)}")
    if unknown:
        raise ContractError(f"{name} unknown fields: {sorted(unknown)}")
    return value


def _text(name: str, value: object, maximum: int = 8192) -> str:
    if not isinstance(value, str) or not value or value.strip() != value:
        raise ContractError(f"{name} must be a non-empty trimmed string")
    if len(value.encode()) > maximum or any(
        unicodedata.category(ch) == "Cc" and ch not in "\n\t" for ch in value
    ):
        raise ContractError(f"{name} exceeds its bound or contains control characters")
    return value


def _string_list(name: str, value: object, required: bool = False) -> list[str]:
    if not isinstance(value, list) or len(value) > 256 or (required and not value):
        raise ContractError(f"{name} must contain {int(required)}-256 strings")
    return [_text(name, item) for item in value]


def _uuid(name: str, value: object) -> str:
    text = _text(name, value, 36)
    try:
        parsed = uuid.UUID(text)
    except ValueError as error:
        raise ContractError(f"{name} must be a UUID") from error
    if parsed.int == 0 or str(parsed) != text:
        raise ContractError(f"{name} must be canonical non-nil lowercase UUID text")
    return text


def _hex(name: str, value: object, pattern: re.Pattern[str] = HEX64) -> str:
    text = _text(name, value, 64)
    if not pattern.fullmatch(text):
        raise ContractError(f"{name} must be canonical lowercase hex")
    return text


def expiry(value: object) -> dt.datetime:
    text = _text("expires_at", value, 20)
    try:
        parsed = dt.datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)
    except ValueError as error:
        raise ContractError("expires_at must be canonical UTC RFC3339 seconds") from error
    return parsed


def github_slug(canonical: object) -> str:
    text = _text("repository.canonical", canonical, 512)
    parts = urlsplit(text)
    segments = parts.path.split("/")
    try:
        port = parts.port
    except ValueError as error:
        raise ContractError("repository.canonical contains an invalid port") from error
    if (
        parts.scheme != "https"
        or parts.netloc != "github.com"
        or parts.username
        or parts.password
        or port is not None
        or parts.query
        or parts.fragment
        or len(segments) != 3
        or not segments[1]
        or not segments[2]
        or segments[2].endswith(".git")
        or text != text.lower()
    ):
        raise ContractError("repository.canonical must be strict lowercase https://github.com/owner/repo")
    if not all(re.fullmatch(r"[a-z0-9._-]+", segment) for segment in segments[1:]):
        raise ContractError("repository.canonical contains a non-canonical owner or repository")
    if any(segment in {".", ".."} for segment in segments[1:]):
        raise ContractError("repository.canonical contains a non-canonical owner or repository")
    return f"{segments[1]}/{segments[2]}"


def _branch(value: object) -> str:
    text = _text("repository.branch", value, 512)
    invalid = set("~^:?*[\\")
    if (
        text == "@"
        or text.startswith("/")
        or text.endswith(("/", "."))
        or "//" in text
        or ".." in text
        or "@{" in text
        or any(ord(ch) <= 0x20 or ord(ch) == 0x7F or ch in invalid for ch in text)
        or any(segment.startswith(".") or segment.endswith(".lock") for segment in text.split("/"))
    ):
        raise ContractError("repository.branch must be a conservative canonical git ref name")
    return text


def _repo_path(value: str) -> None:
    parts = value.split("/")
    if (
        value.startswith("/")
        or "\\" in value
        or any(part in {"", ".", ".."} or part.casefold() == ".git" for part in parts)
    ):
        raise ContractError(f"repository path must be normalized and repo-relative: {value}")


def _portable_references(name: str, values: object) -> None:
    for value in _string_list(name, values):
        lower = value.lower()
        windows_absolute = (
            len(value) > 1 and value[0].isascii() and value[0].isalpha() and value[1] == ":"
        )
        if (
            value.startswith(("/", "~"))
            or "\\" in value
            or any(segment == ".." for segment in value.split("/"))
            or lower.startswith("file:")
            or "/users/" in lower
            or "/home/" in lower
            or SECRET.search(value)
            or any(marker in lower for marker in SECRET_MARKERS)
            or windows_absolute
        ):
            raise ContractError(f"{name} must not contain a host-local path: {value}")
        if value.startswith("git:"):
            _hex(name, value[4:], SHA)
            continue
        if value.startswith("contract:"):
            contract = value[9:]
            if (
                not re.fullmatch(r"[A-Za-z0-9._/-]{1,128}", contract)
                or any(segment in {"", ".."} for segment in contract.split("/"))
            ):
                raise ContractError(f"{name} contract reference is not a portable identifier")
            continue
        if value.startswith("buzz:event:"):
            _hex(name, value[11:])
            continue
        parsed = urlsplit(value)
        try:
            port = parsed.port
        except ValueError as error:
            raise ContractError(f"{name} reference is not portable") from error
        if (
            parsed.scheme != "https"
            or parsed.hostname != "github.com"
            or parsed.username
            or parsed.password
            or port not in {None, 443}
            or parsed.query
            or parsed.fragment
        ):
            raise ContractError(
                f"{name} must use git:/contract:/buzz:event: or credential-free GitHub HTTPS"
            )


def _contract_references(values: object) -> None:
    _portable_references("repository.contracts", values)
    for value in values:
        if not value.startswith("contract:"):
            raise ContractError(
                "repository.contracts must contain only inert contract: coordinates"
            )


def validate_content(kind: int, content: object) -> dict[str, object]:
    if kind not in SHAPES:
        raise ContractError(f"kind {kind} is not a jobs.v1 kind")
    _no_null(content)
    _no_secrets(content)
    required, optional = SHAPES[kind]
    body = _exact_keys("job content", content, required, optional)
    if body["schema_version"] != SCHEMA_VERSION:
        raise ContractError(f"schema_version must be {SCHEMA_VERSION}")
    _uuid("operation_id", body["operation_id"])
    if not isinstance(body["idempotency_key"], str) or not IDEMPOTENCY.fullmatch(body["idempotency_key"]):
        raise ContractError("idempotency_key must be 1-128 printable ASCII bytes without spaces")
    epoch = body["coordinator_epoch"]
    if isinstance(epoch, bool) or not isinstance(epoch, int) or not 1 <= epoch <= 2**32 - 1:
        raise ContractError("coordinator_epoch must be a positive u32")
    project = _exact_keys("project", body["project"], {"address", "home_channel"})
    if not PROJECT.fullmatch(_text("project.address", project["address"], 512)):
        raise ContractError("project.address must be a canonical 30621 NIP-33 coordinate")
    _uuid("project.home_channel", project["home_channel"])
    repository = _exact_keys(
        "repository",
        body["repository"],
        {"canonical", "base_sha", "branch", "worktree_id", "paths", "contracts"},
        {"github_issue", "github_pr", "github_run"},
    )
    github_slug(repository["canonical"])
    _hex("repository.base_sha", repository["base_sha"], SHA)
    _branch(repository["branch"])
    if not isinstance(repository["worktree_id"], str) or not WORKTREE.fullmatch(repository["worktree_id"]):
        raise ContractError("repository.worktree_id must be an opaque portable identifier")
    for path in _string_list("repository.paths", repository["paths"], required=True):
        _repo_path(path)
    _contract_references(repository["contracts"])
    for field in ("github_issue", "github_pr", "github_run"):
        if field in repository:
            if not isinstance(repository[field], str) or not POSITIVE_DECIMAL.fullmatch(repository[field]):
                raise ContractError(f"repository.{field} must be a canonical positive decimal ID")
    if "github_issue" in repository and "github_pr" in repository:
        raise ContractError("repository.github_issue and github_pr are mutually exclusive")
    _hex("sender_pubkey", body["sender_pubkey"])
    _hex("recipient_pubkey", body["recipient_pubkey"])
    if body["sender_pubkey"] == body["recipient_pubkey"]:
        raise ContractError("sender and recipient must differ")
    sponsor = _exact_keys("sponsor", body["sponsor"], {"pubkey", "github_login"})
    _hex("sponsor.pubkey", sponsor["pubkey"])
    _text("sponsor.github_login", sponsor["github_login"], 512)
    expiry(body["expires_at"])

    if kind == 43001:
        _text("capability", body["capability"], 512)
        _text("summary", body["summary"])
        _string_list("acceptance", body["acceptance"], required=True)
        if "supersedes_event_id" in body:
            _hex("supersedes_event_id", body["supersedes_event_id"])
    else:
        _hex("request_event_id", body["request_event_id"])
        if "prior_event_id" in body:
            _hex("prior_event_id", body["prior_event_id"])
            if body["prior_event_id"] == body["request_event_id"]:
                raise ContractError("prior_event_id must differ from request_event_id")
    if kind == 43002:
        claim = _exact_keys(
            "claim", body["claim"], {"status", "scope_digest"}, {"reason"}
        )
        if claim["status"] not in {"processed", "accepted", "declined"}:
            raise ContractError("claim.status must be processed, accepted, or declined")
        _hex("claim.scope_digest", claim["scope_digest"])
        if claim["status"] in {"processed", "declined"} and "prior_event_id" in body:
            raise ContractError(f"{claim['status']} claim must not carry prior_event_id")
        if claim["status"] == "accepted" and "prior_event_id" not in body:
            raise ContractError("accepted claim requires prior_event_id")
        if claim["status"] == "declined":
            reason = claim.get("reason")
            if not isinstance(reason, str) or not re.fullmatch(
                r"[a-z0-9][a-z0-9._-]{0,63}", reason
            ):
                raise ContractError("declined claim requires a 1-64 byte machine reason")
        elif "reason" in claim:
            raise ContractError("claim.reason is only valid for declined")
    elif kind == 43003:
        if body["status"] not in {"progress", "blocked"}:
            raise ContractError("progress status must be progress or blocked")
        _text("message", body["message"])
        _portable_references("evidence", body["evidence"])
    elif kind == 43004:
        if body["outcome"] != "success":
            raise ContractError("result outcome must be success")
        if "candidate_sha" in body:
            _hex("candidate_sha", body["candidate_sha"], SHA)
        _portable_references("artifacts", body["artifacts"])
        _portable_references("evidence", body["evidence"])
        if "capabilities" in body:
            _string_list("capabilities", body["capabilities"])
    elif kind == 43005:
        if body["action"] not in {"cancel", "cancelled", "release", "handoff"}:
            raise ContractError("control action must be cancel, cancelled, release, or handoff")
        _text("reason", body["reason"])
        if body["action"] == "handoff":
            if "handoff_to" not in body or "prior_event_id" not in body:
                raise ContractError("handoff requires handoff_to and prior_event_id")
            _hex("handoff_to", body["handoff_to"])
        elif "handoff_to" in body:
            raise ContractError("handoff_to is only valid for handoff")
        if body["action"] in {"cancelled", "release"} and "prior_event_id" not in body:
            raise ContractError(f"{body['action']} requires prior_event_id")
    elif kind == 43006:
        if body["outcome"] not in {"failed", "indeterminate"}:
            raise ContractError("error outcome must be failed or indeterminate")
        _text("code", body["code"], 512)
        _text("message", body["message"])
        if not isinstance(body["retryable"], bool):
            raise ContractError("retryable must be boolean")
        if body["outcome"] == "indeterminate" and body["retryable"]:
            raise ContractError("indeterminate outcome requires retryable=false")
    return body


def _expected_tags(kind: int, content: dict[str, object]) -> list[list[str]]:
    repo = content["repository"]
    tags = [
        ["h", content["project"]["home_channel"]],
        ["p", content["recipient_pubkey"]],
        ["i", content["operation_id"]],
        ["k", content["idempotency_key"]],
        ["a", content["project"]["address"]],
        ["github-repository", github_slug(repo["canonical"])],
    ]
    for field, tag in (("github_issue", "github-issue"), ("github_pr", "github-pr"), ("github_run", "github-run")):
        if field in repo:
            tags.append([tag, repo[field]])
    if kind == 43001 and "supersedes_event_id" in content:
        tags.append(["e", content["supersedes_event_id"], "", "supersedes"])
    elif kind != 43001:
        tags.append(["e", content["request_event_id"], "", "root"])
        if "prior_event_id" in content:
            tags.append(["e", content["prior_event_id"], "", "reply"])
    return tags


def freeze_event(kind: int, author: str, content: dict[str, object], created_at: int) -> str:
    validate_content(kind, content)
    _hex("event author", author)
    if author != content["sender_pubkey"]:
        raise ContractError("sender_pubkey does not match event author")
    unsigned = {
        "kind": kind,
        "pubkey": author,
        "created_at": created_at,
        "tags": _expected_tags(kind, content),
        "content": canonical_json(content),
    }
    event_id = hashlib.sha256(canonical_json(unsigned).encode()).hexdigest()
    return canonical_json({"id": event_id, **unsigned})


def parse_event(frozen: str) -> dict[str, object]:
    event = loads_unique(frozen)
    event = _exact_keys("local event", event, {"id", "kind", "pubkey", "created_at", "tags", "content"})
    if isinstance(event["kind"], bool) or event["kind"] not in KINDS:
        raise ContractError("invalid job event kind")
    _hex("event id", event["id"])
    _hex("event author", event["pubkey"])
    if isinstance(event["created_at"], bool) or not isinstance(event["created_at"], int):
        raise ContractError("created_at must be integer epoch seconds")
    content = loads_unique(event["content"]) if isinstance(event["content"], str) else None
    content = validate_content(event["kind"], content)
    if event["pubkey"] != content["sender_pubkey"]:
        raise ContractError("sender_pubkey does not match event author")
    if not isinstance(event["tags"], list) or any(
        not isinstance(tag, list) or not all(isinstance(part, str) for part in tag) for tag in event["tags"]
    ):
        raise ContractError("tags must be arrays of strings")
    expected = collections.Counter(tuple(tag) for tag in _expected_tags(event["kind"], content))
    actual = collections.Counter(tuple(tag) for tag in event["tags"])
    if actual != expected:
        raise ContractError("event tags must equal the closed canonical jobs.v1 tag set")
    expected_e = [tag for tag in _expected_tags(event["kind"], content) if tag[0] == "e"]
    actual_e = [tag for tag in event["tags"] if tag[0] == "e"]
    if actual_e != expected_e:
        raise ContractError("event lifecycle e tags must use canonical root/reply order")
    unsigned = {key: event[key] for key in ("kind", "pubkey", "created_at", "tags", "content")}
    if hashlib.sha256(canonical_json(unsigned).encode()).hexdigest() != event["id"]:
        raise ContractError("local event id does not match frozen event bytes")
    event["content_object"] = content
    return event


if __name__ == "__main__":
    from contract_cli import main
    raise SystemExit(main())
