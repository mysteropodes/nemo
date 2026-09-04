#!/usr/bin/env python3
"""Durable at-most-once executor used by the deterministic two-agent proof."""

from __future__ import annotations

import datetime as dt
import json
import sqlite3
from contextlib import closing
from pathlib import Path, PurePosixPath
from typing import Callable

from a2a_contract import (
    COMMON_FIELDS,
    ContractError,
    IdempotencyConflict,
    freeze_event,
    parse_event,
    semantic_digest,
)
from authorization import LocalAuthorizationClient
from local_relay import LocalRelay
from receiver_runtime import exclusive_receiver, publish_frozen_with_retry

TerminalHandler = Callable[[dict[str, object]], tuple[int, dict[str, object]]]


def resolve_repo_path(checkout: Path, repo_relative: str) -> Path:
    """Resolve an authorized wire path without crossing checkout or Git metadata boundaries."""

    path = PurePosixPath(repo_relative)
    if (
        not repo_relative
        or path.is_absolute()
        or "\\" in repo_relative
        or "//" in repo_relative
        or any(part in {"", ".", ".."} or part.casefold() == ".git" for part in path.parts)
    ):
        raise ContractError("repository path is not a safe normalized repo-relative path")
    root = checkout.resolve(strict=True)
    resolved = root.joinpath(*path.parts).resolve(strict=False)
    try:
        resolved.relative_to(root)
    except ValueError as error:
        raise ContractError("repository path escapes the authorized checkout") from error
    return resolved


class ExecutorLedger:
    """Receiver-owned ledger keyed by trusted community, author, and retry key."""

    def __init__(self, path: Path) -> None:
        self.path = path
        with closing(self._connect()) as db, db:
            db.executescript(
                """
                CREATE TABLE IF NOT EXISTS jobs (
                  community TEXT NOT NULL,
                  request_author TEXT NOT NULL,
                  idempotency_key TEXT NOT NULL,
                  digest TEXT NOT NULL,
                  operation_id TEXT NOT NULL,
                  request_event_id TEXT NOT NULL,
                  status TEXT NOT NULL,
                  execution_count INTEGER NOT NULL DEFAULT 0,
                  processed_frozen TEXT NOT NULL,
                  accepted_frozen TEXT,
                  terminal_frozen TEXT,
                  PRIMARY KEY(community, request_author, idempotency_key)
                );
                CREATE TABLE IF NOT EXISTS cursors (
                  community TEXT NOT NULL,
                  worker_id TEXT NOT NULL,
                  sequence INTEGER NOT NULL,
                  PRIMARY KEY(community, worker_id)
                );
                CREATE TABLE IF NOT EXISTS effects (
                  operation_id TEXT NOT NULL,
                  coordinator_epoch INTEGER NOT NULL,
                  worker_id TEXT NOT NULL,
                  PRIMARY KEY(operation_id, coordinator_epoch, worker_id)
                );
                CREATE TABLE IF NOT EXISTS admissions (
                  authorization_id TEXT PRIMARY KEY,
                  community TEXT NOT NULL,
                  request_event_id TEXT NOT NULL,
                  grant_digest TEXT NOT NULL,
                  authorization_expires_at TEXT NOT NULL
                );
                """
            )

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.path)
        db.row_factory = sqlite3.Row
        return db

    @staticmethod
    def _key(request: dict[str, object], community: str) -> tuple[str, str, str]:
        body = request["content_object"]
        return community, body["sender_pubkey"], body["idempotency_key"]

    def observe(self, community: str, request: dict[str, object], digest: str, processed: str) -> bool:
        key = self._key(request, community)
        body = request["content_object"]
        with closing(self._connect()) as db, db:
            db.execute("BEGIN IMMEDIATE")
            row = db.execute(
                "SELECT digest FROM jobs WHERE community=? AND request_author=? AND idempotency_key=?",
                key,
            ).fetchone()
            if row:
                if row["digest"] != digest:
                    raise IdempotencyConflict("same authority-domain idempotency key has changed request semantics")
                return False
            db.execute(
                """INSERT INTO jobs(
                community,request_author,idempotency_key,digest,operation_id,request_event_id,
                status,processed_frozen) VALUES(?,?,?,?,?,?,?,?)""",
                (*key, digest, body["operation_id"], request["id"], "processed", processed),
            )
        return True

    def admit(
        self,
        community: str,
        request: dict[str, object],
        accepted: str,
        authorization: dict[str, str],
    ) -> bool:
        """Consume fresh authorization in the same CAS that owns the frozen Accepted."""

        key = self._key(request, community)
        with closing(self._connect()) as db, db:
            db.execute("BEGIN IMMEDIATE")
            changed = db.execute(
                """UPDATE jobs SET status='admitted',execution_count=1,accepted_frozen=?
                WHERE community=? AND request_author=? AND idempotency_key=?
                AND status='processed' AND execution_count=0""",
                (accepted, *key),
            ).rowcount
            if changed:
                try:
                    db.execute(
                        """INSERT INTO admissions(
                        authorization_id,community,request_event_id,grant_digest,
                        authorization_expires_at) VALUES(?,?,?,?,?)""",
                        (
                            authorization["authorization_id"],
                            community,
                            request["id"],
                            authorization["grant_digest"],
                            authorization["authorization_expires_at"],
                        ),
                    )
                except sqlite3.IntegrityError as error:
                    raise ContractError("authorization response was already consumed") from error
        return changed == 1

    def confirm_accepted(self, community: str, request: dict[str, object]) -> bool:
        """Record that the relay durably ingested the full-reauthorized Accepted event."""

        key = self._key(request, community)
        with closing(self._connect()) as db, db:
            db.execute("BEGIN IMMEDIATE")
            changed = db.execute(
                """UPDATE jobs SET status='accepted'
                WHERE community=? AND request_author=? AND idempotency_key=?
                AND status='admitted' AND execution_count=1""",
                key,
            ).rowcount
        return changed == 1

    def finish(self, community: str, request: dict[str, object], terminal: str) -> None:
        key = self._key(request, community)
        with closing(self._connect()) as db, db:
            db.execute("BEGIN IMMEDIATE")
            changed = db.execute(
                """UPDATE jobs SET status='terminal',terminal_frozen=?
                WHERE community=? AND request_author=? AND idempotency_key=?
                AND status='accepted' AND execution_count=1 AND terminal_frozen IS NULL""",
                (terminal, *key),
            ).rowcount
            if changed != 1:
                raise ContractError("terminal disposition requires one accepted atomic claim")

    def row(self, community: str, request: dict[str, object]) -> sqlite3.Row:
        key = self._key(request, community)
        with closing(self._connect()) as db, db:
            row = db.execute(
                "SELECT * FROM jobs WHERE community=? AND request_author=? AND idempotency_key=?",
                key,
            ).fetchone()
        if not row:
            raise ContractError("ledger row not found")
        return row

    def cursor(self, community: str, worker_id: str) -> int:
        with closing(self._connect()) as db, db:
            row = db.execute(
                "SELECT sequence FROM cursors WHERE community=? AND worker_id=?", (community, worker_id)
            ).fetchone()
        return row["sequence"] if row else 0

    def checkpoint(self, community: str, worker_id: str, sequence: int) -> None:
        with closing(self._connect()) as db, db:
            db.execute(
                """INSERT INTO cursors(community,worker_id,sequence) VALUES(?,?,?)
                ON CONFLICT(community,worker_id) DO UPDATE SET sequence=excluded.sequence""",
                (community, worker_id, sequence),
            )

    def record_effect(self, community: str, request: dict[str, object], worker_id: str) -> None:
        body = request["content_object"]
        row = self.row(community, request)
        if row["status"] != "accepted" or row["execution_count"] != 1:
            raise ContractError("side effect attempted before accepted atomic claim")
        with closing(self._connect()) as db, db:
            try:
                db.execute(
                    "INSERT INTO effects(operation_id,coordinator_epoch,worker_id) VALUES(?,?,?)",
                    (body["operation_id"], body["coordinator_epoch"], worker_id),
                )
            except sqlite3.IntegrityError as error:
                raise ContractError("duplicate side effect") from error

    def effect_count(self) -> int:
        with closing(self._connect()) as db, db:
            return db.execute("SELECT COUNT(*) AS count FROM effects").fetchone()["count"]

    def admission_count(self) -> int:
        with closing(self._connect()) as db, db:
            return db.execute("SELECT COUNT(*) AS count FROM admissions").fetchone()["count"]


class Executor:
    """Consumes addressed requests and freezes receipts before publication."""

    def __init__(
        self,
        worker_id: str,
        pubkey: str,
        sponsor_pubkey: str,
        github_login: str,
        ledger: ExecutorLedger,
        relay: LocalRelay,
        authorization: LocalAuthorizationClient,
        handler: TerminalHandler,
    ) -> None:
        self.worker_id = worker_id
        self.pubkey = pubkey
        self.sponsor = {"pubkey": sponsor_pubkey, "github_login": github_login}
        self.ledger = ledger
        self.relay = relay
        self.authorization = authorization
        self.handler = handler

    def _common_response(self, request: dict[str, object]) -> dict[str, object]:
        body = request["content_object"]
        common = {name: json.loads(json.dumps(body[name])) for name in COMMON_FIELDS}
        common["sender_pubkey"] = self.pubkey
        common["recipient_pubkey"] = body["sender_pubkey"]
        common["sponsor"] = self.sponsor
        return common

    def _receipt(
        self,
        request: dict[str, object],
        kind: int,
        fields: dict[str, object],
        offset: int,
    ) -> str:
        content = {**self._common_response(request), "request_event_id": request["id"], **fields}
        return freeze_event(kind, self.pubkey, content, request["created_at"] + offset)

    def process(self, community: str, route: str, request_frozen: str, now: dt.datetime) -> dict[str, object]:
        request = parse_event(request_frozen)
        if request["kind"] != 43001 or request["content_object"]["recipient_pubkey"] != self.pubkey:
            raise ContractError("executor received a request addressed to another agent")
        digest = semantic_digest(request["content_object"])
        processed = self._receipt(
            request,
            43002,
            {"claim": {"status": "processed", "scope_digest": digest}},
            1,
        )
        is_new = self.ledger.observe(community, request, digest, processed)
        row = self.ledger.row(community, request)
        publish_frozen_with_retry(self.relay, community, route, row["processed_frozen"], now)
        processed_event = parse_event(row["processed_frozen"])
        accepted = self._receipt(
            request,
            43002,
            {
                "prior_event_id": processed_event["id"],
                "claim": {"status": "accepted", "scope_digest": digest},
            },
            2,
        )
        row = self.ledger.row(community, request)
        if row["status"] == "processed":
            authorization = self.authorization.preflight(request, now, route)
            self.ledger.admit(community, request, accepted, authorization)
        row = self.ledger.row(community, request)
        if row["status"] == "admitted":
            publish_frozen_with_retry(
                self.relay, community, route, row["accepted_frozen"], now
            )
            claimed = self.ledger.confirm_accepted(community, request)
        else:
            claimed = False
        if claimed:
            kind, fields = self.handler(request)
            accepted_event = parse_event(row["accepted_frozen"])
            terminal = self._receipt(
                request,
                kind,
                {"prior_event_id": accepted_event["id"], **fields},
                3,
            )
            self.ledger.finish(community, request, terminal)
            publish_frozen_with_retry(self.relay, community, route, terminal, now)
            return {"disposition": "executed", "request_event_id": request["id"]}
        row = self.ledger.row(community, request)
        if row["accepted_frozen"]:
            publish_frozen_with_retry(
                self.relay, community, route, row["accepted_frozen"], now
            )
        if row["terminal_frozen"]:
            publish_frozen_with_retry(
                self.relay, community, route, row["terminal_frozen"], now
            )
        return {
            "disposition": "replayed" if not is_new else "already_claimed",
            "request_event_id": request["id"],
        }

    def drain(
        self,
        community: str,
        route: str,
        now: dt.datetime,
        checkpoint: bool = True,
    ) -> list[dict[str, object]]:
        lock_path = self.ledger.path.with_name(self.ledger.path.name + ".receiver.lock")
        with exclusive_receiver(lock_path):
            cursor = self.ledger.cursor(community, self.worker_id)
            results = []
            for sequence, frozen in self.relay.requests(community, self.pubkey, cursor):
                results.append(self.process(community, route, frozen, now))
                if checkpoint:
                    self.ledger.checkpoint(community, self.worker_id, sequence)
            return results


def make_handler(
    ledger: ExecutorLedger,
    community: str,
    worker_id: str,
    terminal: str,
    handoff_to: str | None = None,
) -> TerminalHandler:
    def handle(request: dict[str, object]) -> tuple[int, dict[str, object]]:
        ledger.record_effect(community, request, worker_id)
        if terminal == "result":
            return 43004, {
                "outcome": "success",
                "candidate_sha": "d" * 40,
                "artifacts": ["git:d" + "d" * 39],
                "evidence": ["contract:python-unittest"],
            }
        if terminal == "handoff" and handoff_to:
            return 43005, {
                "action": "handoff",
                "reason": "Recipient boundary requires the next agent",
                "handoff_to": handoff_to,
            }
        raise ContractError("handler terminal must be result or handoff with target")

    return handle
