#!/usr/bin/env python3
"""Strict local mirror of the Buzz jobs.v1 envelope and relay safety door.

This module deliberately does not implement Nostr signing. ``freeze_event`` creates a
deterministic local event record so the smoke test can prove byte-for-byte receipt replay.
Live cryptographic and relay acceptance belongs to Buzz staging.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import hashlib
import json
import re
import sqlite3
import sys
import uuid
from contextlib import closing
from pathlib import Path
from urllib.parse import urlsplit

SCHEMA_VERSION = "buzz.jobs.v1"
PROTOCOL_VERSION = "NEMO-A2A-1"
MAX_JOB_TTL_SECONDS = 604_800
KINDS = {
    43001: "request",
    43002: "claim",
    43003: "progress",
    43004: "result",
    43005: "control",
    43006: "error",
}
TERMINAL_KINDS = {43004, 43005, 43006}
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
    43003: (
        COMMON_FIELDS | {"request_event_id", "prior_event_id", "status", "message", "evidence"},
        set(),
    ),
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
PROJECT = re.compile(r"^30621:([0-9a-f]{64}):([^:]+)$")
WORKTREE = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")
IDEMPOTENCY = re.compile(r"^[!-~]{1,128}$")
LOCAL_PATH = re.compile(r"(?:^|\s)(?:/Users/|/home/|/[A-Za-z0-9_.-]+/|~[/\\]|[A-Za-z]:[\\/]|file://)")
SECRET = re.compile(r"(?:-----BEGIN [A-Z ]*PRIVATE KEY-----|github_pat_|ghp_[A-Za-z0-9]|sk-[A-Za-z0-9]{12}|Bearer\s+[A-Za-z0-9])")


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
    if isinstance(value, str) and SECRET.search(value):
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
    if len(value.encode()) > maximum or any(ord(ch) < 32 and ch not in "\n\t" for ch in value):
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
    if str(parsed) != text:
        raise ContractError(f"{name} must be canonical lowercase UUID text")
    return text


def _hex(name: str, value: object, pattern: re.Pattern[str] = HEX64) -> str:
    text = _text(name, value, 64)
    if not pattern.fullmatch(text):
        raise ContractError(f"{name} must be canonical lowercase hex")
    return text


def _expiry(value: object) -> dt.datetime:
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
    if (
        parts.scheme != "https"
        or parts.netloc != "github.com"
        or parts.username
        or parts.password
        or parts.port is not None
        or parts.query
        or parts.fragment
        or len(segments) != 3
        or not segments[1]
        or not segments[2]
        or segments[2].endswith(".git")
        or text != text.lower()
    ):
        raise ContractError("repository.canonical must be strict lowercase https://github.com/owner/repo")
    if not all(re.fullmatch(r"[a-z0-9][a-z0-9._-]*", segment) for segment in segments[1:]):
        raise ContractError("repository.canonical contains a non-canonical owner or repository")
    return f"{segments[1]}/{segments[2]}"


def _repo_path(value: str) -> None:
    if (
        value.startswith(("/", "~"))
        or "\\" in value
        or "//" in value
        or any(part in {"", ".", ".."} for part in value.rstrip("/").split("/"))
    ):
        raise ContractError(f"repository path must be normalized and repo-relative: {value}")


def _portable_references(name: str, values: object) -> None:
    for value in _string_list(name, values):
        if LOCAL_PATH.search(value):
            raise ContractError(f"{name} must not contain a host-local path: {value}")


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
    _text("repository.branch", repository["branch"], 512)
    if not isinstance(repository["worktree_id"], str) or not WORKTREE.fullmatch(repository["worktree_id"]):
        raise ContractError("repository.worktree_id must be an opaque portable identifier")
    for path in _string_list("repository.paths", repository["paths"], required=True):
        _repo_path(path)
    _portable_references("repository.contracts", repository["contracts"])
    for field in ("github_issue", "github_pr", "github_run"):
        if field in repository:
            _text(f"repository.{field}", repository[field], 512)
    _hex("sender_pubkey", body["sender_pubkey"])
    _hex("recipient_pubkey", body["recipient_pubkey"])
    if body["sender_pubkey"] == body["recipient_pubkey"]:
        raise ContractError("sender and recipient must differ")
    sponsor = _exact_keys("sponsor", body["sponsor"], {"pubkey", "github_login"})
    _hex("sponsor.pubkey", sponsor["pubkey"])
    _text("sponsor.github_login", sponsor["github_login"], 512)
    _expiry(body["expires_at"])

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
        claim = _exact_keys("claim", body["claim"], {"status", "scope_digest"})
        if claim["status"] not in {"processed", "accepted"}:
            raise ContractError("claim.status must be processed or accepted")
        _hex("claim.scope_digest", claim["scope_digest"])
        if claim["status"] == "processed" and "prior_event_id" in body:
            raise ContractError("processed claim must not carry prior_event_id")
        if claim["status"] == "accepted" and "prior_event_id" not in body:
            raise ContractError("accepted claim requires prior_event_id")
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
        if body["action"] not in {"cancel", "release", "handoff"}:
            raise ContractError("control action must be cancel, release, or handoff")
        _text("reason", body["reason"])
        if body["action"] == "handoff":
            if "handoff_to" not in body or "prior_event_id" not in body:
                raise ContractError("handoff requires handoff_to and prior_event_id")
            _hex("handoff_to", body["handoff_to"])
        elif "handoff_to" in body:
            raise ContractError("handoff_to is only valid for handoff")
        if body["action"] == "release" and "prior_event_id" not in body:
            raise ContractError("release requires prior_event_id")
    elif kind == 43006:
        if body["outcome"] != "error":
            raise ContractError("error outcome must be error")
        _text("code", body["code"], 512)
        _text("message", body["message"])
        if not isinstance(body["retryable"], bool):
            raise ContractError("retryable must be boolean")
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
    unsigned = {key: event[key] for key in ("kind", "pubkey", "created_at", "tags", "content")}
    if hashlib.sha256(canonical_json(unsigned).encode()).hexdigest() != event["id"]:
        raise ContractError("local event id does not match frozen event bytes")
    event["content_object"] = content
    return event


class LocalAuthority:
    """Trusted relay context; none of this state is selected by signed content."""

    def __init__(self) -> None:
        self.projects: dict[tuple[str, str], tuple[str, set[str]]] = {}
        self.members: set[tuple[str, str, str]] = set()
        self.sponsors: dict[tuple[str, str], str] = {}
        self.capabilities: dict[tuple[str, str], dict[str, list[str]]] = {}

    def add_project(self, community: str, address: str, channel: str, repository: str) -> None:
        self.projects[(community, address)] = (channel, {repository})

    def add_agent(
        self,
        community: str,
        channel: str,
        pubkey: str,
        sponsor: str,
        capabilities: dict[str, list[str]] | None = None,
    ) -> None:
        self.members.add((community, channel, pubkey))
        self.sponsors[(community, pubkey)] = sponsor
        self.capabilities[(community, pubkey)] = capabilities or {}

    def authorize(self, community: str, route: str, event: dict[str, object]) -> None:
        body = event["content_object"]
        address, channel = body["project"]["address"], body["project"]["home_channel"]
        project_scope = self.projects.get((community, address))
        if not project_scope or project_scope[0] != channel or body["repository"]["canonical"] not in project_scope[1]:
            raise ContractError("project address, home channel, and canonical repository are not allowlisted together")
        sender, recipient = body["sender_pubkey"], body["recipient_pubkey"]
        for role, pubkey in (("sender", sender), ("recipient", recipient)):
            if (community, channel, pubkey) not in self.members:
                raise ContractError(f"{role} must be a direct project-home-channel member")
        if self.sponsors.get((community, sender)) != body["sponsor"]["pubkey"]:
            raise ContractError("sponsor assertion does not match authoritative ownership")
        if route == "dm":
            if self.sponsors.get((community, sender)) != self.sponsors.get((community, recipient)):
                raise ContractError("cross-owner DM is forbidden")
        elif route != "project_channel":
            raise ContractError("unknown trusted delivery route")
        if event["kind"] == 43001:
            grant = self.capabilities.get((community, recipient), {}).get(body["capability"])
            if grant is None or any(not any(_path_within(path, prefix) for prefix in grant) for path in body["repository"]["paths"]):
                raise ContractError("recipient capability does not cover the requested path scope")
        if event["kind"] == 43005 and body["action"] == "handoff":
            if (community, channel, body["handoff_to"]) not in self.members:
                raise ContractError("handoff target must be a direct project-home-channel member")


def _path_within(path: str, prefix: str) -> bool:
    path, prefix = path.rstrip("/"), prefix.rstrip("/")
    return prefix == "." or path == prefix or path.startswith(prefix + "/")


class LocalRelay:
    """SQLite event store with trusted community/route metadata and strict transitions."""

    def __init__(self, path: Path, authority: LocalAuthority) -> None:
        self.path = path
        self.authority = authority
        with closing(self._connect()) as db, db:
            db.execute(
                """CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT, community TEXT NOT NULL,
                route TEXT NOT NULL, event_id TEXT NOT NULL, kind INTEGER NOT NULL,
                recipient TEXT NOT NULL, frozen TEXT NOT NULL,
                UNIQUE(community, event_id))"""
            )

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.path)
        db.row_factory = sqlite3.Row
        return db

    def publish(self, community: str, route: str, frozen: str, now: dt.datetime) -> dict[str, object]:
        event = parse_event(frozen)
        body = event["content_object"]
        expiry = _expiry(body["expires_at"])
        if expiry <= now:
            raise ContractError("job event is expired")
        if expiry > now + dt.timedelta(seconds=MAX_JOB_TTL_SECONDS):
            raise ContractError("expires_at exceeds the server seven-day TTL cap")
        created = dt.datetime.fromtimestamp(event["created_at"], dt.timezone.utc)
        if created > expiry:
            raise ContractError("event created_at is after expires_at")
        if expiry > created + dt.timedelta(seconds=MAX_JOB_TTL_SECONDS):
            raise ContractError("expires_at exceeds the event-created-at seven-day TTL cap")
        self.authority.authorize(community, route, event)
        with closing(self._connect()) as db, db:
            prior = db.execute(
                "SELECT frozen FROM events WHERE community=? AND event_id=?",
                (community, event["id"]),
            ).fetchone()
            if prior:
                if prior["frozen"] != frozen:
                    raise ContractError("event id collision with changed frozen bytes")
                return {"status": "already_stored", "event_id": event["id"], "lifecycle": None}
            self._validate_chain(db, community, event)
            db.execute(
                "INSERT INTO events(community,route,event_id,kind,recipient,frozen) VALUES(?,?,?,?,?,?)",
                (community, route, event["id"], event["kind"], body["recipient_pubkey"], frozen),
            )
        return {"status": "stored", "event_id": event["id"], "lifecycle": None}

    def _event(self, db: sqlite3.Connection, community: str, event_id: str) -> dict[str, object]:
        row = db.execute(
            "SELECT frozen FROM events WHERE community=? AND event_id=?", (community, event_id)
        ).fetchone()
        if not row:
            raise ContractError("referenced event was not found in this community")
        return parse_event(row["frozen"])

    def _validate_chain(self, db: sqlite3.Connection, community: str, event: dict[str, object]) -> None:
        body, kind = event["content_object"], event["kind"]
        if kind == 43001:
            if "supersedes_event_id" in body:
                self._validate_superseding_request(db, community, event)
            return
        request = self._event(db, community, body["request_event_id"])
        if request["kind"] != 43001:
            raise ContractError("request_event_id must reference kind 43001")
        root = request["content_object"]
        for field in ("operation_id", "idempotency_key", "coordinator_epoch", "project", "repository", "expires_at"):
            if body[field] != root[field]:
                raise ContractError(f"transition changed request field {field}")
        worker_to_requester = body["sender_pubkey"] == root["recipient_pubkey"] and body["recipient_pubkey"] == root["sender_pubkey"]
        requester_to_worker = body["sender_pubkey"] == root["sender_pubkey"] and body["recipient_pubkey"] == root["recipient_pubkey"]
        if kind in {43002, 43003, 43004, 43006} and not worker_to_requester:
            raise ContractError("only the addressed worker may author this transition")
        if kind == 43005:
            allowed = requester_to_worker if body["action"] == "cancel" else worker_to_requester
            if not allowed:
                raise ContractError("control action signer/addressee is not authorized")
        related = self._related_events(db, community, body["request_event_id"])
        if any(candidate["kind"] in TERMINAL_KINDS for candidate in related):
            raise ContractError("job is terminal; lifecycle forks are forbidden")
        if kind == 43002 and any(
            candidate["kind"] == 43002
            and candidate["content_object"]["claim"]["status"] == body["claim"]["status"]
            for candidate in related
        ):
            raise ContractError(f"duplicate {body['claim']['status']} claim slot")
        if kind == 43005 and body["action"] == "cancel" and "prior_event_id" not in body and related:
            raise ContractError("root cancel is only valid before any lifecycle child")
        if "prior_event_id" in body:
            if any(
                candidate["content_object"].get("prior_event_id") == body["prior_event_id"]
                for candidate in related
            ):
                raise ContractError("prior event already has a lifecycle child")
            prior = self._event(db, community, body["prior_event_id"])
            prior_body = prior["content_object"]
            if prior_body.get("request_event_id") != body["request_event_id"]:
                raise ContractError("prior_event_id belongs to another request")
            if kind == 43002 and body["claim"]["status"] == "accepted":
                if prior["kind"] != 43002 or prior_body["claim"]["status"] != "processed":
                    raise ContractError("accepted claim must follow processed claim")
            elif not (
                (prior["kind"] == 43002 and prior_body["claim"]["status"] == "accepted")
                or prior["kind"] == 43003
            ):
                raise ContractError("lifecycle event must follow accepted or progress")

    def _related_events(
        self, db: sqlite3.Connection, community: str, request_id: str
    ) -> list[dict[str, object]]:
        rows = db.execute(
            "SELECT frozen FROM events WHERE community=? AND kind != 43001", (community,)
        )
        return [
            event for event in (parse_event(row["frozen"]) for row in rows)
            if event["content_object"].get("request_event_id") == request_id
        ]

    def _validate_superseding_request(self, db: sqlite3.Connection, community: str, event: dict[str, object]) -> None:
        body = event["content_object"]
        handoff = self._event(db, community, body["supersedes_event_id"])
        handoff_body = handoff["content_object"]
        if handoff["kind"] != 43005 or handoff_body["action"] != "handoff":
            raise ContractError("supersedes_event_id must reference terminal handoff")
        old_request = self._event(db, community, handoff_body["request_event_id"])
        old = old_request["content_object"]
        if body["sender_pubkey"] != old["sender_pubkey"] or body["sponsor"] != old["sponsor"]:
            raise ContractError("superseding request must be signed by the original coordinator")
        if body["recipient_pubkey"] != handoff_body["handoff_to"]:
            raise ContractError("superseding request recipient must equal handoff_to")
        for field in (
            "operation_id", "idempotency_key", "project", "repository", "sponsor", "expires_at",
            "capability", "summary", "acceptance",
        ):
            if body[field] != old[field]:
                raise ContractError(f"superseding request changed {field}")
        if body["coordinator_epoch"] != old["coordinator_epoch"] + 1:
            raise ContractError("superseding request requires exactly the next coordinator_epoch")

    def requests(self, community: str, recipient: str, after: int = 0) -> list[tuple[int, str]]:
        with closing(self._connect()) as db, db:
            rows = db.execute(
                "SELECT sequence,frozen FROM events WHERE community=? AND kind=43001 AND recipient=? AND sequence>? ORDER BY sequence",
                (community, recipient, after),
            ).fetchall()
        return [(row["sequence"], row["frozen"]) for row in rows]

    def frozen(self, community: str, event_id: str) -> str:
        with closing(self._connect()) as db, db:
            row = db.execute(
                "SELECT frozen FROM events WHERE community=? AND event_id=?", (community, event_id)
            ).fetchone()
        if not row:
            raise ContractError("event was not found in this community")
        return row["frozen"]

    def events(self, community: str) -> list[dict[str, object]]:
        with closing(self._connect()) as db, db:
            rows = db.execute("SELECT frozen FROM events WHERE community=? ORDER BY sequence", (community,)).fetchall()
        return [parse_event(row["frozen"]) for row in rows]


def _main() -> int:
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


if __name__ == "__main__":
    raise SystemExit(_main())
