#!/usr/bin/env python3
"""Repeatable local proof: one request, one claim, one execution, no reconnect duplicate."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from a2a_contract import freeze_event, parse_event
from authorization import AuthorityBindings, LocalAuthorizationClient, LocalAuthorizationService
from executor import Executor, ExecutorLedger, make_handler
from local_relay import LocalAuthority, LocalRelay
from receiver_grants import ReceiverGrant

COMMUNITY = "99999999-1234-4234-8234-123456789abc"
RELAY_HOST = "localhost"
PROJECT_AUTHOR = "11" * 32
PROJECT_ADDRESS = f"30621:{PROJECT_AUTHOR}:nemo"
HOME_CHANNEL = "11111111-2222-4333-8444-555555555555"
REQUESTER = "22" * 32
REQUESTER_SPONSOR = "33" * 32
WORKER_A = "44" * 32
WORKER_A_SPONSOR = "55" * 32
WORKER_B = "66" * 32
WORKER_B_SPONSOR = "77" * 32
CREATED = int(dt.datetime(2030, 1, 1, tzinfo=dt.timezone.utc).timestamp())
NOW = dt.datetime(2030, 1, 1, 0, 0, 10, tzinfo=dt.timezone.utc)
EXPIRES = "2030-01-01T01:00:00Z"
REPOSITORY = "https://github.com/mysteropodes/nemo"
BRANCH = "codex/a2a-local-smoke"
WORKTREE_ID = "local-smoke-1"


@dataclass(frozen=True)
class CheckoutFixture:
    root: Path
    repository: str
    base_sha: str
    branch: str
    worktree_id: str


def create_checkout(root: Path) -> CheckoutFixture:
    root.mkdir(parents=True)

    def git(*arguments: str) -> str:
        result = subprocess.run(
            ["git", "-C", str(root), *arguments],
            check=True,
            text=True,
            capture_output=True,
        )
        return result.stdout.strip()

    git("init")
    git("config", "user.name", "Nemo A2A Test")
    git("config", "user.email", "nemo-a2a@example.invalid")
    git("checkout", "-b", BRANCH)
    target = root / ".agents/skills/nemo-a2a"
    target.mkdir(parents=True)
    (target / "smoke.txt").write_text("receiver checkout fixture\n")
    git("add", ".agents/skills/nemo-a2a/smoke.txt")
    git("commit", "-m", "test: create receiver checkout fixture")
    git("remote", "add", "origin", REPOSITORY + ".git")
    return CheckoutFixture(root.resolve(), REPOSITORY, git("rev-parse", "HEAD"), BRANCH, WORKTREE_ID)


def authority() -> LocalAuthority:
    auth = LocalAuthority()
    auth.add_project(COMMUNITY, PROJECT_ADDRESS, HOME_CHANNEL, REPOSITORY)
    auth.add_agent(COMMUNITY, HOME_CHANNEL, REQUESTER, REQUESTER_SPONSOR)
    capability = {"nemo.a2a.smoke": [".agents/skills/nemo-a2a"]}
    auth.add_agent(COMMUNITY, HOME_CHANNEL, WORKER_A, WORKER_A_SPONSOR, capability)
    auth.add_agent(COMMUNITY, HOME_CHANNEL, WORKER_B, WORKER_B_SPONSOR, capability)
    return auth


def request_content(
    recipient: str = WORKER_A,
    epoch: int = 1,
    checkout: CheckoutFixture | None = None,
) -> dict[str, object]:
    repository = checkout.repository if checkout else REPOSITORY
    base_sha = checkout.base_sha if checkout else "ab" * 20
    branch = checkout.branch if checkout else BRANCH
    worktree_id = checkout.worktree_id if checkout else WORKTREE_ID
    return {
        "schema_version": "buzz.jobs.v1",
        "operation_id": "12345678-1234-4234-8234-123456789abc",
        "idempotency_key": "nemo-a2a-local-smoke-v1",
        "coordinator_epoch": epoch,
        "project": {"address": PROJECT_ADDRESS, "home_channel": HOME_CHANNEL},
        "repository": {
            "canonical": repository,
            "github_issue": "1",
            "base_sha": base_sha,
            "branch": branch,
            "worktree_id": worktree_id,
            "paths": [".agents/skills/nemo-a2a"],
            "contracts": ["contract:python-unittest"],
        },
        "sender_pubkey": REQUESTER,
        "recipient_pubkey": recipient,
        "sponsor": {"pubkey": REQUESTER_SPONSOR, "github_login": "nemo-coordinator"},
        "expires_at": EXPIRES,
        "capability": "nemo.a2a.smoke",
        "summary": "Prove one durable execution across a reconnect replay",
        "acceptance": ["exactly one accepted claim", "exactly one terminal disposition"],
    }


def authorization_client(
    relay: LocalRelay,
    checkout: CheckoutFixture,
    recipient: str = WORKER_A,
    recipient_owner: str = WORKER_A_SPONSOR,
) -> LocalAuthorizationClient:
    service = LocalAuthorizationService(relay, COMMUNITY, RELAY_HOST)
    grant = ReceiverGrant.from_value(
        {
            "project_address": PROJECT_ADDRESS,
            "home_channel": HOME_CHANNEL,
            "repository": checkout.repository,
            "requester_pubkeys": [REQUESTER],
            "capabilities": ["nemo.a2a.smoke"],
            "path_prefixes": [".agents/skills/nemo-a2a"],
            "base_sha": checkout.base_sha,
            "branch": checkout.branch,
            "worktree_id": checkout.worktree_id,
            "checkout_root": str(checkout.root),
        }
    )
    bindings = AuthorityBindings(
        recipient_pubkey=recipient,
        requester_owner_pubkey=REQUESTER_SPONSOR,
        recipient_owner_pubkey=recipient_owner,
        project_head_event_id=service.project_head(PROJECT_ADDRESS, HOME_CHANNEL),
        repository_coordinate=service.repository_coordinate(PROJECT_ADDRESS, REPOSITORY),
        repository_announcement_event_id=service.repository_announcement(REPOSITORY),
    )
    return LocalAuthorizationClient(
        service,
        "http://localhost/api/jobs/authorize",
        grant,
        bindings,
        dev_mode=True,
    )


def superseding_request(
    handoff: dict[str, object], checkout: CheckoutFixture, epoch: int = 2
) -> str:
    content = request_content(WORKER_B, epoch, checkout)
    content["supersedes_event_id"] = handoff["id"]
    return freeze_event(43001, REQUESTER, content, CREATED + 4)


def _counts(events: list[dict[str, object]]) -> dict[str, int]:
    result = {"request": 0, "processed": 0, "accepted": 0, "result": 0, "handoff": 0}
    for event in events:
        if event["kind"] == 43001:
            result["request"] += 1
        elif event["kind"] == 43002:
            result[event["content_object"]["claim"]["status"]] += 1
        elif event["kind"] == 43004:
            result["result"] += 1
        elif event["kind"] == 43005 and event["content_object"]["action"] == "handoff":
            result["handoff"] += 1
    return result


def run_smoke(state_dir: Path, terminal: str) -> dict[str, object]:
    state_dir.mkdir(parents=True, exist_ok=True)
    relay_path = state_dir / "relay.sqlite3"
    worker_a_path = state_dir / "worker-a.sqlite3"
    worker_b_path = state_dir / "worker-b.sqlite3"
    if any(path.exists() for path in (relay_path, worker_a_path, worker_b_path)):
        raise RuntimeError("--state-dir must not contain a prior smoke database")
    checkout = create_checkout(state_dir / "checkout")

    relay = LocalRelay(relay_path, authority())
    request_frozen = freeze_event(
        43001, REQUESTER, request_content(checkout=checkout), CREATED
    )
    relay_receipt = relay.publish(COMMUNITY, "project_channel", request_frozen, NOW)
    if relay_receipt["lifecycle"] is not None:
        raise AssertionError("relay acknowledgement synthesized lifecycle acceptance")

    ledger_a = ExecutorLedger(worker_a_path)
    first = Executor(
        "worker-a",
        WORKER_A,
        WORKER_A_SPONSOR,
        "worker-a-owner",
        ledger_a,
        relay,
        authorization_client(relay, checkout),
        make_handler(ledger_a, COMMUNITY, "worker-a", terminal, WORKER_B),
    )
    first_results = first.drain(COMMUNITY, "project_channel", NOW, checkpoint=False)

    # Simulate a disconnect after all receipts are durable but before the consumer cursor.
    relay = LocalRelay(relay_path, authority())
    reopened_ledger = ExecutorLedger(worker_a_path)
    reconnect = Executor(
        "worker-a",
        WORKER_A,
        WORKER_A_SPONSOR,
        "worker-a-owner",
        reopened_ledger,
        relay,
        authorization_client(relay, checkout),
        make_handler(reopened_ledger, COMMUNITY, "worker-a", terminal, WORKER_B),
    )
    replay_results = reconnect.drain(COMMUNITY, "project_channel", NOW, checkpoint=True)
    request = parse_event(request_frozen)
    ledger_row = reopened_ledger.row(COMMUNITY, request)
    frozen_match = all(
        relay.frozen(COMMUNITY, parse_event(ledger_row[field])["id"]) == ledger_row[field]
        for field in ("processed_frozen", "accepted_frozen", "terminal_frozen")
    )
    events = relay.events(COMMUNITY)
    counts = _counts(events)
    expected = {
        "request": 1,
        "processed": 1,
        "accepted": 1,
        "result": int(terminal == "result"),
        "handoff": int(terminal == "handoff"),
    }
    if counts != expected:
        raise AssertionError(f"unexpected lifecycle counts: {counts}, expected {expected}")
    if (
        ledger_row["execution_count"] != 1
        or reopened_ledger.effect_count() != 1
        or reopened_ledger.admission_count() != 1
    ):
        raise AssertionError("reconnect caused duplicate execution")
    if first_results[0]["disposition"] != "executed" or replay_results[0]["disposition"] != "replayed":
        raise AssertionError("expected one execution followed by one durable replay")
    if not frozen_match:
        raise AssertionError("receipt replay rebuilt bytes instead of reusing the frozen event")

    handoff_requires_request = None
    if terminal == "handoff":
        ledger_b = ExecutorLedger(worker_b_path)
        worker_b = Executor(
            "worker-b",
            WORKER_B,
            WORKER_B_SPONSOR,
            "worker-b-owner",
            ledger_b,
            relay,
            authorization_client(relay, checkout, WORKER_B, WORKER_B_SPONSOR),
            make_handler(ledger_b, COMMUNITY, "worker-b", "result"),
        )
        handoff_only = worker_b.drain(COMMUNITY, "project_channel", NOW)
        handoff_requires_request = not handoff_only and ledger_b.effect_count() == 0
        if not handoff_requires_request:
            raise AssertionError("43005 handoff alone started the next worker")

    return {
        "ok": True,
        "protocol": "NEMO-A2A-1",
        "transport": "local-sqlite-deterministic-hash-not-nostr-signatures",
        "terminal": terminal,
        "relay_ack_lifecycle": relay_receipt["lifecycle"],
        "event_counts": counts,
        "execution_count": ledger_row["execution_count"],
        "authorization_admission_count": reopened_ledger.admission_count(),
        "side_effect_count": reopened_ledger.effect_count(),
        "reconnect_disposition": replay_results[0]["disposition"],
        "frozen_receipt_bytes_replayed": frozen_match,
        "handoff_requires_new_request": handoff_requires_request,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--terminal", choices=("result", "handoff"), default="result")
    parser.add_argument("--state-dir", type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.state_dir:
        result = run_smoke(args.state_dir, args.terminal)
    else:
        with tempfile.TemporaryDirectory(prefix="nemo-a2a-") as temporary:
            result = run_smoke(Path(temporary), args.terminal)
    if args.json:
        print(json.dumps(result, sort_keys=True))
    else:
        print("PASS", result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
