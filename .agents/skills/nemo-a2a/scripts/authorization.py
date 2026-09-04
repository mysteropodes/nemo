#!/usr/bin/env python3
"""Strict local mirror of Buzz's short-lived job authorization preflight.

This module is test infrastructure, not a NIP-98 implementation.  It models the
binding, freshness, replay, and receiver-local grant checks that the trusted ACP
sidecar must complete immediately before its durable admission CAS.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import re
import uuid
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from a2a_contract import (
    ContractError,
    expiry,
    github_slug,
    parse_event,
    semantic_digest,
)
from receiver_grants import ReceiverGrant, authorize_receiver_request

if TYPE_CHECKING:
    from local_relay import LocalRelay

AUTH_SCHEMA_VERSION = "buzz.job-authorization.v1"
AUTH_TTL_SECONDS = 5
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PROJECT_COORDINATE = re.compile(r"^30621:([0-9a-f]{64}):(.+)$")
REPOSITORY_COORDINATE = re.compile(r"^30617:([0-9a-f]{64}):([^\x00-\x1f:]+)$")
REQUEST_FIELDS = {
    "schema_version",
    "nonce",
    "request_event_id",
    "semantic_digest",
    "community_id",
    "relay_host",
    "channel_id",
    "project_address",
    "repository",
    "requester_pubkey",
    "recipient_pubkey",
}
RESPONSE_FIELDS = {
    "schema_version",
    "authorized",
    "authorization_id",
    "issued_at",
    "expires_at",
    "binding",
    "project_head_event_id",
    "repository_coordinate",
    "repository_announcement_event_id",
    "requester_owner_pubkey",
    "recipient_owner_pubkey",
}


def loads_strict_json(raw: str) -> object:
    """Decode an authorization document while rejecting duplicate object keys."""

    def unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise ContractError(f"duplicate authorization JSON key: {key}")
            result[key] = value
        return result

    try:
        return json.loads(raw, object_pairs_hook=unique)
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid authorization JSON: {error}") from error


def _exact(name: str, value: object, fields: set[str]) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise ContractError(f"{name} must contain exactly {sorted(fields)}")
    if any(item is None for item in value.values()):
        raise ContractError(f"{name} must not contain null")
    return value


def _uuid(name: str, value: object) -> str:
    if not isinstance(value, str):
        raise ContractError(f"{name} must be a canonical non-nil UUID")
    try:
        parsed = uuid.UUID(value)
    except ValueError as error:
        raise ContractError(f"{name} must be a canonical non-nil UUID") from error
    if parsed.int == 0 or str(parsed) != value:
        raise ContractError(f"{name} must be a canonical non-nil UUID")
    return value


def _hex(name: str, value: object) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise ContractError(f"{name} must be 64-byte lowercase hex")
    return value


def _time(name: str, value: object) -> dt.datetime:
    if not isinstance(value, str):
        raise ContractError(f"{name} must use canonical UTC RFC3339 seconds")
    try:
        parsed = dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(
            tzinfo=dt.timezone.utc
        )
    except ValueError as error:
        raise ContractError(f"{name} must use canonical UTC RFC3339 seconds") from error
    return parsed


def _format_time(value: dt.datetime) -> str:
    return value.astimezone(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def canonical_host(value: str) -> str:
    host = value.strip().lower()
    if host.endswith(":443") or host.endswith(":80"):
        host = host.rsplit(":", 1)[0]
    if host.endswith("."):
        host = host[:-1]
    return host


def validate_authorization_endpoint(url: str, dev_mode: bool = False) -> str:
    """Require HTTPS, except exact loopback HTTP in explicitly enabled tests."""

    parsed = urlsplit(url)
    if (
        parsed.path != "/api/jobs/authorize"
        or parsed.query
        or parsed.fragment
        or parsed.username
        or parsed.password
        or not parsed.hostname
    ):
        raise ContractError("job authorization endpoint is not canonical")
    if parsed.scheme == "https":
        return canonical_host(parsed.netloc)
    if parsed.scheme == "http" and dev_mode and parsed.hostname in {
        "127.0.0.1",
        "::1",
        "localhost",
    }:
        return canonical_host(parsed.netloc)
    raise ContractError("job authorization requires HTTPS; HTTP is loopback dev/test only")


def validate_request(value: object) -> dict[str, object]:
    request = _exact("authorization request", value, REQUEST_FIELDS)
    if request["schema_version"] != AUTH_SCHEMA_VERSION:
        raise ContractError("unsupported job authorization schema_version")
    _uuid("nonce", request["nonce"])
    _hex("request_event_id", request["request_event_id"])
    _hex("semantic_digest", request["semantic_digest"])
    _uuid("community_id", request["community_id"])
    _uuid("channel_id", request["channel_id"])
    if not isinstance(request["relay_host"], str) or canonical_host(request["relay_host"]) != request["relay_host"]:
        raise ContractError("relay_host must be canonical")
    if not isinstance(request["project_address"], str) or not PROJECT_COORDINATE.fullmatch(
        request["project_address"]
    ):
        raise ContractError("project_address must be a canonical kind-30621 coordinate")
    if not isinstance(request["repository"], dict):
        raise ContractError("repository must be an object")
    _hex("requester_pubkey", request["requester_pubkey"])
    _hex("recipient_pubkey", request["recipient_pubkey"])
    if request["requester_pubkey"] == request["recipient_pubkey"]:
        raise ContractError("requester and recipient must differ")
    return request


def validate_response(
    value: object,
    request: dict[str, object],
    now: dt.datetime,
) -> dict[str, object]:
    response = _exact("authorization response", value, RESPONSE_FIELDS)
    if response["schema_version"] != AUTH_SCHEMA_VERSION or response["authorized"] is not True:
        raise ContractError("response is not a job authorization")
    _uuid("authorization_id", response["authorization_id"])
    if response["binding"] != request:
        raise ContractError("authorization response binding does not exactly echo request")
    validate_request(response["binding"])
    issued, expires = _time("issued_at", response["issued_at"]), _time(
        "expires_at", response["expires_at"]
    )
    if expires <= issued or (expires - issued).total_seconds() > AUTH_TTL_SECONDS:
        raise ContractError("authorization lifetime exceeds five seconds")
    if now < issued - dt.timedelta(seconds=1) or now > expires:
        raise ContractError("authorization is stale or not yet valid")
    _hex("project_head_event_id", response["project_head_event_id"])
    if not isinstance(response["repository_coordinate"], str) or not REPOSITORY_COORDINATE.fullmatch(
        response["repository_coordinate"]
    ):
        raise ContractError("repository_coordinate must be a canonical kind-30617 coordinate")
    _hex(
        "repository_announcement_event_id",
        response["repository_announcement_event_id"],
    )
    _hex("requester_owner_pubkey", response["requester_owner_pubkey"])
    _hex("recipient_owner_pubkey", response["recipient_owner_pubkey"])
    return response


@dataclass(frozen=True)
class AuthorityBindings:
    """Expected relay-derived evidence kept separate from local checkout grants."""

    recipient_pubkey: str
    requester_owner_pubkey: str
    recipient_owner_pubkey: str
    project_head_event_id: str
    repository_coordinate: str
    repository_announcement_event_id: str


class LocalAuthorizationService:
    """Deterministic stand-in for Host/NIP-98 guarded POST /api/jobs/authorize."""

    def __init__(self, relay: LocalRelay, community_id: str, relay_host: str) -> None:
        self.relay = relay
        self.community_id = _uuid("community_id", community_id)
        self.relay_host = canonical_host(relay_host)
        self.nonces: set[str] = set()
        self.nip98_events: set[str] = set()

    @staticmethod
    def project_head(address: str, channel: str) -> str:
        return hashlib.sha256(f"project:{address}:{channel}".encode()).hexdigest()

    @staticmethod
    def repository_announcement(repository: str) -> str:
        return hashlib.sha256(f"repository:{repository}".encode()).hexdigest()

    @staticmethod
    def repository_coordinate(project_address: str, repository: str) -> str:
        owner = project_address.split(":", 2)[1]
        return f"30617:{owner}:{github_slug(repository)}"

    def authorize(
        self,
        payload: dict[str, object],
        *,
        caller_pubkey: str,
        host_header: str,
        nip98_event_id: str,
        now: dt.datetime,
        route: str,
    ) -> dict[str, object]:
        request = validate_request(payload)
        _hex("NIP-98 event id", nip98_event_id)
        if canonical_host(host_header) != self.relay_host:
            raise ContractError("Host does not resolve to the authorization community")
        if request["community_id"] != self.community_id or request["relay_host"] != self.relay_host:
            raise ContractError("authorization community or host binding mismatch")
        if caller_pubkey != request["recipient_pubkey"]:
            raise ContractError("authorization caller must be the addressed recipient")
        if request["nonce"] in self.nonces or nip98_event_id in self.nip98_events:
            raise ContractError("authorization nonce or NIP-98 event was replayed")
        event = parse_event(self.relay.frozen(self.community_id, request["request_event_id"]))
        body = event["content_object"]
        expected = {
            "request_event_id": event["id"],
            "semantic_digest": semantic_digest(body),
            "channel_id": body["project"]["home_channel"],
            "project_address": body["project"]["address"],
            "repository": body["repository"],
            "requester_pubkey": body["sender_pubkey"],
            "recipient_pubkey": body["recipient_pubkey"],
        }
        if any(request[field] != value for field, value in expected.items()):
            raise ContractError("authorization fields do not match the stored signed request")
        if now > expiry(body["expires_at"]):
            raise ContractError("stored job request is expired")
        self.relay.authority.authorize(self.community_id, route, event)
        if self.relay.lifecycle_state(self.community_id, event["id"]) in {
            "declined",
            "completed",
            "failed",
            "indeterminate",
            "cancel_requested",
            "cancelled",
            "release",
            "handoff",
        }:
            raise ContractError("terminal or cancelled request cannot be authorized for execution")
        self.nonces.add(request["nonce"])
        self.nip98_events.add(nip98_event_id)
        project_head = self.project_head(request["project_address"], request["channel_id"])
        announcement = self.repository_announcement(request["repository"]["canonical"])
        return {
            "schema_version": AUTH_SCHEMA_VERSION,
            "authorized": True,
            "authorization_id": str(uuid.uuid5(uuid.NAMESPACE_URL, request["nonce"])),
            "issued_at": _format_time(now),
            "expires_at": _format_time(now + dt.timedelta(seconds=AUTH_TTL_SECONDS)),
            "binding": json.loads(json.dumps(request)),
            "project_head_event_id": project_head,
            "repository_coordinate": self.repository_coordinate(
                request["project_address"], request["repository"]["canonical"]
            ),
            "repository_announcement_event_id": announcement,
            "requester_owner_pubkey": self.relay.authority.sponsors[
                (self.community_id, request["requester_pubkey"])
            ],
            "recipient_owner_pubkey": self.relay.authority.sponsors[
                (self.community_id, request["recipient_pubkey"])
            ],
        }


class LocalAuthorizationClient:
    """Trusted receiver-side exact comparison around the deterministic service."""

    def __init__(
        self,
        service: LocalAuthorizationService,
        endpoint: str,
        grant: ReceiverGrant | tuple[ReceiverGrant, ...],
        authority: AuthorityBindings,
        *,
        dev_mode: bool = False,
    ) -> None:
        self.service = service
        self.relay_host = validate_authorization_endpoint(endpoint, dev_mode)
        self.grants = (grant,) if isinstance(grant, ReceiverGrant) else grant
        self.grant = self.grants[0] if len(self.grants) == 1 else None
        self.authority = authority

    def preflight(
        self,
        event: dict[str, object],
        now: dt.datetime,
        route: str,
    ) -> dict[str, str]:
        body = event["content_object"]
        if body["recipient_pubkey"] != self.authority.recipient_pubkey:
            raise ContractError("request recipient does not match receiver authority")
        nonce = str(uuid.uuid4())
        payload = {
            "schema_version": AUTH_SCHEMA_VERSION,
            "nonce": nonce,
            "request_event_id": event["id"],
            "semantic_digest": semantic_digest(body),
            "community_id": self.service.community_id,
            "relay_host": self.relay_host,
            "channel_id": body["project"]["home_channel"],
            "project_address": body["project"]["address"],
            "repository": json.loads(json.dumps(body["repository"])),
            "requester_pubkey": body["sender_pubkey"],
            "recipient_pubkey": body["recipient_pubkey"],
        }
        response = self.service.authorize(
            payload,
            caller_pubkey=body["recipient_pubkey"],
            host_header=self.relay_host,
            nip98_event_id=hashlib.sha256(f"nip98:{nonce}".encode()).hexdigest(),
            now=now,
            route=route,
        )
        validate_response(response, payload, now)
        expected = {
            "requester_owner_pubkey": self.authority.requester_owner_pubkey,
            "recipient_owner_pubkey": self.authority.recipient_owner_pubkey,
            "project_head_event_id": self.authority.project_head_event_id,
            "repository_coordinate": self.authority.repository_coordinate,
            "repository_announcement_event_id": self.authority.repository_announcement_event_id,
        }
        if any(response[field] != value for field, value in expected.items()):
            raise ContractError("authorization evidence does not match receiver authority")
        grant, checkout = authorize_receiver_request(self.grants, event)
        return {
            "authorization_id": response["authorization_id"],
            "authorization_expires_at": response["expires_at"],
            "grant_digest": grant.grant_digest,
            "checkout_root": str(checkout),
        }
