#!/usr/bin/env python3
"""Receiver-local A2A grant parsing and live checkout authorization."""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import uuid
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit

from a2a_contract import ContractError, canonical_json, github_slug

GRANT_VERSION = 1
MAX_GRANTS = 128
MAX_CAPABILITIES = 64
MAX_PATH_PREFIXES = 128
GRANT_FIELDS = {
    "project_address",
    "home_channel",
    "repository",
    "requester_pubkeys",
    "capabilities",
    "path_prefixes",
    "base_sha",
    "branch",
    "worktree_id",
    "checkout_root",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SHA = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
PROJECT = re.compile(r"^30621:[0-9a-f]{64}:.+$")


def _strict_json(raw: str) -> object:
    def unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise ContractError(f"duplicate receiver grant JSON key: {key}")
            result[key] = value
        return result

    try:
        return json.loads(raw, object_pairs_hook=unique)
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid receiver grant JSON: {error}") from error


def _token(name: str, value: object) -> str:
    if (
        not isinstance(value, str)
        or not 1 <= len(value.encode()) <= 128
        or value.strip() != value
        or not value.isascii()
        or not value.isprintable()
        or any(character.isspace() for character in value)
    ):
        raise ContractError(f"{name} must be a 1-128 byte printable token")
    return value


def _canonical_uuid(name: str, value: object) -> str:
    if not isinstance(value, str):
        raise ContractError(f"{name} must be a canonical UUID")
    try:
        parsed = uuid.UUID(value)
    except ValueError as error:
        raise ContractError(f"{name} must be a canonical UUID") from error
    if str(parsed) != value:
        raise ContractError(f"{name} must be a canonical UUID")
    return value


def _unique_strings(
    name: str,
    value: object,
    maximum: int,
    validator,
) -> tuple[str, ...]:
    if not isinstance(value, list) or not value or len(value) > maximum:
        raise ContractError(f"{name} must contain 1-{maximum} entries")
    checked = tuple(validator(name, item) for item in value)
    if len(set(checked)) != len(checked):
        raise ContractError(f"{name} must contain unique entries")
    return checked


def _pubkey(name: str, value: object) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise ContractError(f"{name} must contain canonical lowercase public keys")
    return value


def _path_prefix(name: str, value: object) -> str:
    if not isinstance(value, str) or not safe_relative_path(value):
        raise ContractError(
            f"{name} must contain normalized repository-relative paths outside .git"
        )
    return value


def safe_relative_path(value: str) -> bool:
    parts = value.split("/")
    return (
        bool(value)
        and not value.startswith("/")
        and "\\" not in value
        and all(
            part not in {"", ".", ".."} and part.casefold() != ".git"
            for part in parts
        )
    )


def canonical_git_origin(value: str) -> str:
    if value.startswith("git@github.com:"):
        path = value.removeprefix("git@github.com:")
    else:
        parsed = urlsplit(value)
        try:
            port = parsed.port
        except ValueError as error:
            raise ContractError("origin is not a canonical GitHub remote") from error
        if (
            parsed.scheme != "https"
            or parsed.hostname != "github.com"
            or parsed.username
            or parsed.password
            or port is not None
            or parsed.query
            or parsed.fragment
        ):
            raise ContractError("origin is not a canonical GitHub remote")
        path = parsed.path.removeprefix("/")
    path = path.removesuffix(".git")
    if len(path.split("/")) != 2 or any(not part for part in path.split("/")):
        raise ContractError("origin must identify one GitHub owner/repository")
    canonical = f"https://github.com/{path.lower()}"
    github_slug(canonical)
    return canonical


def _git(root: Path, *arguments: str) -> str:
    environment = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_CONFIG_NOSYSTEM": "1",
    }
    try:
        result = subprocess.run(
            ["git", "-C", str(root), *arguments],
            env=environment,
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ContractError("live checkout inspection failed") from error
    if result.returncode or result.stderr or not result.stdout.strip():
        raise ContractError("live checkout inspection failed")
    return result.stdout.strip()


@dataclass(frozen=True)
class ReceiverGrant:
    """Exact receiver-local grant document entry; never signed or sent to a model."""

    project_address: str
    home_channel: str
    repository: str
    requester_pubkeys: tuple[str, ...]
    capabilities: tuple[str, ...]
    path_prefixes: tuple[str, ...]
    base_sha: str
    branch: str
    worktree_id: str
    checkout_root: str

    @classmethod
    def from_value(cls, value: object) -> "ReceiverGrant":
        if not isinstance(value, dict) or set(value) != GRANT_FIELDS:
            raise ContractError(
                f"receiver grant must contain exactly {sorted(GRANT_FIELDS)}"
            )
        if not isinstance(value["project_address"], str) or not PROJECT.fullmatch(
            value["project_address"]
        ):
            raise ContractError("project_address must be a canonical 30621 coordinate")
        home_channel = _canonical_uuid("home_channel", value["home_channel"])
        repository = value["repository"]
        if not isinstance(repository, str):
            raise ContractError("repository must be canonical GitHub HTTPS")
        github_slug(repository)
        if not isinstance(value["base_sha"], str) or not SHA.fullmatch(
            value["base_sha"]
        ):
            raise ContractError("base_sha must be 40 or 64 lowercase hexadecimal characters")
        branch = _token("branch", value["branch"])
        worktree_id = _token("worktree_id", value["worktree_id"])
        root_value = value["checkout_root"]
        if not isinstance(root_value, str) or not Path(root_value).is_absolute():
            raise ContractError("checkout_root must be an absolute existing Git checkout")
        try:
            root = Path(root_value).resolve(strict=True)
        except OSError as error:
            raise ContractError(
                "checkout_root must be an absolute existing Git checkout"
            ) from error
        return cls(
            project_address=value["project_address"],
            home_channel=home_channel,
            repository=repository,
            requester_pubkeys=_unique_strings(
                "requester_pubkeys", value["requester_pubkeys"], MAX_GRANTS, _pubkey
            ),
            capabilities=_unique_strings(
                "capabilities", value["capabilities"], MAX_CAPABILITIES, _token
            ),
            path_prefixes=_unique_strings(
                "path_prefixes", value["path_prefixes"], MAX_PATH_PREFIXES, _path_prefix
            ),
            base_sha=value["base_sha"],
            branch=branch,
            worktree_id=worktree_id,
            checkout_root=str(root),
        )

    @property
    def grant_digest(self) -> str:
        return hashlib.sha256(
            canonical_json(
                {
                    "project_address": self.project_address,
                    "home_channel": self.home_channel,
                    "repository": self.repository,
                    "requester_pubkeys": list(self.requester_pubkeys),
                    "capabilities": list(self.capabilities),
                    "path_prefixes": list(self.path_prefixes),
                    "base_sha": self.base_sha,
                    "branch": self.branch,
                    "worktree_id": self.worktree_id,
                    "checkout_root": self.checkout_root,
                }
            ).encode()
        ).hexdigest()

    def authorize_request(self, event: dict[str, object]) -> Path:
        if not self.matches_request(event):
            raise ContractError("request is outside the exact receiver-local grant")
        body = event["content_object"]
        repository = body["repository"]
        if any(not self._path_inside_checkout(path) for path in repository["paths"]):
            raise ContractError("request path escapes the receiver-local checkout")
        try:
            root = Path(self.checkout_root).resolve(strict=True)
            top = Path(_git(root, "rev-parse", "--show-toplevel")).resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise ContractError("live checkout inspection failed") from error
        origin = canonical_git_origin(_git(root, "remote", "get-url", "origin"))
        branch = _git(root, "symbolic-ref", "--quiet", "--short", "HEAD")
        head = _git(root, "rev-parse", "HEAD")
        if (
            top != root
            or origin != self.repository
            or branch != self.branch
            or head != self.base_sha
        ):
            raise ContractError("live checkout root, origin, branch, or HEAD changed")
        return root

    def matches_request(self, event: dict[str, object]) -> bool:
        body = event["content_object"]
        repository = body["repository"]
        return not (
            body["project"]["address"] != self.project_address
            or body["project"]["home_channel"] != self.home_channel
            or repository["canonical"] != self.repository
            or repository["base_sha"] != self.base_sha
            or repository["branch"] != self.branch
            or repository["worktree_id"] != self.worktree_id
            or body["capability"] not in self.capabilities
            or body["sender_pubkey"] not in self.requester_pubkeys
            or not self.path_prefixes
            or any(not self._path_allowed(path) for path in repository["paths"])
        )

    def _path_allowed(self, value: str) -> bool:
        if not safe_relative_path(value):
            return False
        return any(
            value == prefix or value.startswith(prefix + "/")
            for prefix in self.path_prefixes
        )

    def _path_inside_checkout(self, value: str) -> bool:
        try:
            root = Path(self.checkout_root).resolve(strict=True)
            candidate = root.joinpath(*PurePosixPath(value).parts).resolve(strict=False)
            candidate.relative_to(root)
        except (OSError, RuntimeError, ValueError):
            return False
        return True


def load_receiver_grants(raw: str) -> tuple[ReceiverGrant, ...]:
    document = _strict_json(raw)
    if not isinstance(document, dict) or set(document) != {"version", "grants"}:
        raise ContractError("receiver grant document requires only version and grants")
    if document["version"] != GRANT_VERSION:
        raise ContractError("receiver grant document version must be 1")
    values = document["grants"]
    if not isinstance(values, list) or len(values) > MAX_GRANTS:
        raise ContractError(f"receiver grant document allows at most {MAX_GRANTS} grants")
    return tuple(ReceiverGrant.from_value(value) for value in values)


def authorize_receiver_request(
    grants: tuple[ReceiverGrant, ...], event: dict[str, object]
) -> tuple[ReceiverGrant, Path]:
    matches = tuple(grant for grant in grants if grant.matches_request(event))
    if len(matches) != 1:
        raise ContractError("request must match exactly one receiver-local grant")
    grant = matches[0]
    return grant, grant.authorize_request(event)
