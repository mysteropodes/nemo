#!/usr/bin/env python3
"""Deterministic local relay/authority mirror for Nemo A2A lifecycle tests only."""

from __future__ import annotations

import datetime as dt
import sqlite3
from contextlib import closing
from pathlib import Path

from a2a_contract import (
    JOB_TERMINAL_AUDIT_GRACE_SECONDS,
    MAX_JOB_TTL_SECONDS,
    ContractError,
    expiry,
    parse_event,
    semantic_digest,
)


class LocalAuthority:
    """Trusted relay context; none of this state is selected by signed content."""

    def __init__(self) -> None:
        self.projects: dict[tuple[str, str], tuple[str, set[str]]] = {}
        self.members: set[tuple[str, str, str]] = set()
        self.sponsors: dict[tuple[str, str], str] = {}
        self.capabilities: dict[tuple[str, str], dict[str, list[str]]] = {}

    def add_project(
        self, community: str, address: str, channel: str, repository: str
    ) -> None:
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

    def authorize_principal(self, community: str, event: dict[str, object]) -> None:
        """Model current relay admission without re-evaluating stored-event scope."""
        sender = event["content_object"]["sender_pubkey"]
        if not any(
            member_community == community and pubkey == sender
            for member_community, _, pubkey in self.members
        ):
            raise ContractError("signed event author is not a current community member")

    def authorize(
        self, community: str, route: str, event: dict[str, object]
    ) -> None:
        self.authorize_principal(community, event)
        body = event["content_object"]
        address, channel = body["project"]["address"], body["project"]["home_channel"]
        project_scope = self.projects.get((community, address))
        if (
            not project_scope
            or project_scope[0] != channel
            or body["repository"]["canonical"] not in project_scope[1]
        ):
            raise ContractError(
                "project address, home channel, and canonical repository are not allowlisted together"
            )
        sender, recipient = body["sender_pubkey"], body["recipient_pubkey"]
        for role, pubkey in (("sender", sender), ("recipient", recipient)):
            if (community, channel, pubkey) not in self.members:
                raise ContractError(f"{role} must be a direct project-home-channel member")
        if self.sponsors.get((community, sender)) != body["sponsor"]["pubkey"]:
            raise ContractError("sponsor assertion does not match authoritative ownership")
        if route == "dm":
            if self.sponsors.get((community, sender)) != self.sponsors.get(
                (community, recipient)
            ):
                raise ContractError("cross-owner DM is forbidden")
        elif route != "project_channel":
            raise ContractError("unknown trusted delivery route")
        if event["kind"] == 43001:
            grant = self.capabilities.get((community, recipient), {}).get(body["capability"])
            if grant is None or any(
                not any(_path_within(path, prefix) for prefix in grant)
                for path in body["repository"]["paths"]
            ):
                raise ContractError("recipient capability does not cover the requested path scope")
        if event["kind"] == 43005 and body["action"] == "handoff":
            if (community, channel, body["handoff_to"]) not in self.members:
                raise ContractError("handoff target must be a direct project-home-channel member")


def _path_within(path: str, prefix: str) -> bool:
    path, prefix = path.rstrip("/"), prefix.rstrip("/")
    return prefix == "." or path == prefix or path.startswith(prefix + "/")


def _is_terminal(event: dict[str, object]) -> bool:
    if event["kind"] == 43002:
        return event["content_object"]["claim"]["status"] == "declined"
    if event["kind"] in {43004, 43006}:
        return True
    if event["kind"] != 43005:
        return False
    body = event["content_object"]
    return body["action"] != "cancel" or "prior_event_id" not in body


def _is_audit_terminal(event: dict[str, object]) -> bool:
    body = event["content_object"]
    return (
        (event["kind"] == 43002 and body["claim"]["status"] == "declined")
        or (event["kind"] == 43005 and body["action"] == "cancelled")
        or (event["kind"] == 43006 and body["outcome"] in {"failed", "indeterminate"})
    )


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

    def publish(
        self, community: str, route: str, frozen: str, now: dt.datetime
    ) -> dict[str, object]:
        event = parse_event(frozen)
        body = event["content_object"]
        self.authority.authorize_principal(community, event)
        with closing(self._connect()) as db, db:
            prior = db.execute(
                "SELECT frozen FROM events WHERE community=? AND event_id=?",
                (community, event["id"]),
            ).fetchone()
            if prior:
                if prior["frozen"] != frozen:
                    raise ContractError("event id collision with changed frozen bytes")
                return {"state": "stored", "event_id": event["id"], "lifecycle": None}
            self.authority.authorize(community, route, event)
            event_expiry = expiry(body["expires_at"])
            late_terminal = event_expiry <= now and _is_audit_terminal(event)
            grace_end = event_expiry + dt.timedelta(
                seconds=JOB_TERMINAL_AUDIT_GRACE_SECONDS
            )
            if event_expiry <= now and (not late_terminal or now > grace_end):
                raise ContractError("job event is expired outside terminal audit grace")
            if event_expiry > now + dt.timedelta(seconds=MAX_JOB_TTL_SECONDS):
                raise ContractError("expires_at exceeds the server seven-day TTL cap")
            created = dt.datetime.fromtimestamp(event["created_at"], dt.timezone.utc)
            if created > event_expiry and (not late_terminal or created > grace_end):
                raise ContractError("event created_at is after expires_at")
            if event_expiry > created + dt.timedelta(seconds=MAX_JOB_TTL_SECONDS):
                raise ContractError("expires_at exceeds the event-created-at seven-day TTL cap")
            self._validate_chain(db, community, event)
            db.execute(
                "INSERT INTO events(community,route,event_id,kind,recipient,frozen) VALUES(?,?,?,?,?,?)",
                (community, route, event["id"], event["kind"], body["recipient_pubkey"], frozen),
            )
        return {"state": "stored", "event_id": event["id"], "lifecycle": None}

    def _event(
        self, db: sqlite3.Connection, community: str, event_id: str
    ) -> dict[str, object]:
        row = db.execute(
            "SELECT frozen FROM events WHERE community=? AND event_id=?",
            (community, event_id),
        ).fetchone()
        if not row:
            raise ContractError("referenced event was not found in this community")
        return parse_event(row["frozen"])

    def _validate_chain(
        self, db: sqlite3.Connection, community: str, event: dict[str, object]
    ) -> None:
        body, kind = event["content_object"], event["kind"]
        if kind == 43001:
            if "supersedes_event_id" in body:
                self._validate_superseding_request(db, community, event)
            elif body["coordinator_epoch"] != 1:
                raise ContractError("initial request coordinator_epoch must be 1")
            self._validate_request_uniqueness(db, community, event)
            return
        request = self._event(db, community, body["request_event_id"])
        if request["kind"] != 43001:
            raise ContractError("request_event_id must reference kind 43001")
        root = request["content_object"]
        for field in (
            "operation_id",
            "idempotency_key",
            "coordinator_epoch",
            "project",
            "repository",
            "expires_at",
        ):
            if body[field] != root[field]:
                raise ContractError(f"transition changed request field {field}")
        if kind == 43002 and body["claim"]["scope_digest"] != semantic_digest(root):
            raise ContractError("claim.scope_digest does not match canonical request content")
        worker_to_requester = (
            body["sender_pubkey"] == root["recipient_pubkey"]
            and body["recipient_pubkey"] == root["sender_pubkey"]
        )
        requester_to_worker = (
            body["sender_pubkey"] == root["sender_pubkey"]
            and body["recipient_pubkey"] == root["recipient_pubkey"]
        )
        if kind in {43002, 43003, 43004, 43006} and not worker_to_requester:
            raise ContractError("only the addressed worker may author this transition")
        if kind == 43005:
            allowed = requester_to_worker if body["action"] == "cancel" else worker_to_requester
            if not allowed:
                raise ContractError("control action signer/addressee is not authorized")
        related = self._related_events(db, community, body["request_event_id"])
        if any(_is_terminal(candidate) for candidate in related):
            raise ContractError("job is terminal; lifecycle forks are forbidden")
        if kind == 43002 and any(
            candidate["kind"] == 43002
            and candidate["content_object"]["claim"]["status"] == body["claim"]["status"]
            for candidate in related
        ):
            raise ContractError(f"duplicate {body['claim']['status']} claim slot")
        if (
            kind == 43005
            and body["action"] == "cancel"
            and "prior_event_id" not in body
            and related
        ):
            raise ContractError("root cancel is only valid before any lifecycle child")
        predecessor_id = body.get("prior_event_id", body["request_event_id"])
        if any(
            candidate["content_object"].get(
                "prior_event_id", candidate["content_object"]["request_event_id"]
            )
            == predecessor_id
            for candidate in related
        ):
            raise ContractError("prior event already has a lifecycle child")
        if "prior_event_id" in body:
            prior = self._event(db, community, body["prior_event_id"])
            prior_body = prior["content_object"]
            if prior_body.get("request_event_id") != body["request_event_id"]:
                raise ContractError("prior_event_id belongs to another request")
            if kind == 43002 and body["claim"]["status"] == "accepted":
                if prior["kind"] != 43002 or prior_body["claim"]["status"] != "processed":
                    raise ContractError("accepted claim must follow processed claim")
            elif kind == 43005 and body["action"] == "cancel":
                if prior["kind"] not in {43002, 43003}:
                    raise ContractError("cancel must follow processed, accepted, or progress")
            elif kind == 43005 and body["action"] == "cancelled":
                if prior["kind"] != 43005 or prior_body["action"] != "cancel":
                    raise ContractError("cancelled acknowledgement must follow cancel request")
            elif kind == 43005:
                if not (
                    (
                        prior["kind"] == 43002
                        and prior_body["claim"]["status"] == "accepted"
                    )
                    or prior["kind"] == 43003
                ):
                    raise ContractError("release/handoff must follow accepted or progress")
            elif not (
                (
                    prior["kind"] == 43002
                    and prior_body["claim"]["status"] == "accepted"
                )
                or prior["kind"] == 43003
            ):
                raise ContractError("lifecycle event must follow accepted or progress")

    def _validate_request_uniqueness(
        self, db: sqlite3.Connection, community: str, event: dict[str, object]
    ) -> None:
        body = event["content_object"]
        rows = db.execute(
            "SELECT frozen FROM events WHERE community=? AND kind=43001", (community,)
        )
        requests = [parse_event(row["frozen"])["content_object"] for row in rows]
        same_epoch = [
            request
            for request in requests
            if request["coordinator_epoch"] == body["coordinator_epoch"]
        ]
        if any(
            request["sender_pubkey"] == body["sender_pubkey"]
            and request["idempotency_key"] == body["idempotency_key"]
            for request in same_epoch
        ):
            raise ContractError("request idempotency key already has a different event")
        if any(request["operation_id"] == body["operation_id"] for request in same_epoch):
            raise ContractError("coordinator epoch already has a request for this operation")

    def _related_events(
        self, db: sqlite3.Connection, community: str, request_id: str
    ) -> list[dict[str, object]]:
        rows = db.execute(
            "SELECT frozen FROM events WHERE community=? AND kind != 43001", (community,)
        )
        return [
            event
            for event in (parse_event(row["frozen"]) for row in rows)
            if event["content_object"].get("request_event_id") == request_id
        ]

    def _validate_superseding_request(
        self, db: sqlite3.Connection, community: str, event: dict[str, object]
    ) -> None:
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
            "operation_id",
            "idempotency_key",
            "project",
            "repository",
            "sponsor",
            "expires_at",
            "capability",
            "summary",
            "acceptance",
        ):
            if body[field] != old[field]:
                raise ContractError(f"superseding request changed {field}")
        if body["coordinator_epoch"] != old["coordinator_epoch"] + 1:
            raise ContractError("superseding request requires exactly the next coordinator_epoch")

    def requests(
        self, community: str, recipient: str, after: int = 0
    ) -> list[tuple[int, str]]:
        with closing(self._connect()) as db, db:
            rows = db.execute(
                "SELECT sequence,frozen FROM events WHERE community=? AND kind=43001 "
                "AND recipient=? AND sequence>? ORDER BY sequence",
                (community, recipient, after),
            ).fetchall()
        return [(row["sequence"], row["frozen"]) for row in rows]

    def frozen(self, community: str, event_id: str) -> str:
        with closing(self._connect()) as db, db:
            row = db.execute(
                "SELECT frozen FROM events WHERE community=? AND event_id=?",
                (community, event_id),
            ).fetchone()
        if not row:
            raise ContractError("event was not found in this community")
        return row["frozen"]

    def events(self, community: str) -> list[dict[str, object]]:
        with closing(self._connect()) as db, db:
            rows = db.execute(
                "SELECT frozen FROM events WHERE community=? ORDER BY sequence", (community,)
            ).fetchall()
        return [parse_event(row["frozen"]) for row in rows]

    def lifecycle_state(self, community: str, request_id: str) -> str:
        related = [
            event
            for event in self.events(community)
            if event["content_object"].get("request_event_id") == request_id
        ]
        if not related:
            return "requested"
        event = related[-1]
        body = event["content_object"]
        if event["kind"] == 43002:
            return body["claim"]["status"]
        if event["kind"] == 43003:
            return body["status"]
        if event["kind"] == 43004:
            return "completed"
        if event["kind"] == 43006:
            return body["outcome"]
        if body["action"] == "cancel" and "prior_event_id" in body:
            return "cancel_requested"
        return "cancelled" if body["action"] == "cancel" else body["action"]
