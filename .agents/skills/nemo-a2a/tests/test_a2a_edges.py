from __future__ import annotations

import copy
import datetime as dt
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
import sys

sys.path.insert(0, str(SCRIPTS))

from a2a_contract import (  # noqa: E402
    COMMON_FIELDS,
    ContractError,
    freeze_event,
    parse_event,
    semantic_digest,
    validate_content,
)
from executor import resolve_repo_path  # noqa: E402
from local_relay import LocalRelay  # noqa: E402
from two_agent_smoke import (  # noqa: E402
    COMMUNITY,
    CREATED,
    NOW,
    REQUESTER,
    WORKER_A,
    WORKER_A_SPONSOR,
    WORKER_B,
    WORKER_B_SPONSOR,
    authority,
    request_content,
)


def worker_common(request: dict[str, object]) -> dict[str, object]:
    common = {key: copy.deepcopy(request[key]) for key in COMMON_FIELDS}
    common.update(
        {
            "sender_pubkey": WORKER_A,
            "recipient_pubkey": REQUESTER,
            "sponsor": {"pubkey": WORKER_A_SPONSOR, "github_login": "owner-a"},
        }
    )
    return common


def publish_request(relay: LocalRelay) -> tuple[str, dict[str, object]]:
    frozen = freeze_event(43001, REQUESTER, request_content(), CREATED)
    relay.publish(COMMUNITY, "project_channel", frozen, NOW)
    return frozen, parse_event(frozen)


def publish_processed(
    relay: LocalRelay, request: dict[str, object]
) -> dict[str, object]:
    content = {
        **worker_common(request["content_object"]),
        "request_event_id": request["id"],
        "claim": {
            "status": "processed",
            "scope_digest": semantic_digest(request["content_object"]),
        },
    }
    frozen = freeze_event(43002, WORKER_A, content, CREATED + 1)
    relay.publish(COMMUNITY, "project_channel", frozen, NOW)
    return parse_event(frozen)


def publish_accepted(
    relay: LocalRelay, request: dict[str, object], processed: dict[str, object]
) -> dict[str, object]:
    content = {
        **worker_common(request["content_object"]),
        "request_event_id": request["id"],
        "prior_event_id": processed["id"],
        "claim": {
            "status": "accepted",
            "scope_digest": semantic_digest(request["content_object"]),
        },
    }
    frozen = freeze_event(43002, WORKER_A, content, CREATED + 2)
    relay.publish(COMMUNITY, "project_channel", frozen, NOW)
    return parse_event(frozen)


def publish_short_request(relay: LocalRelay) -> dict[str, object]:
    body = request_content()
    body["expires_at"] = "2030-01-01T00:00:20Z"
    frozen = freeze_event(43001, REQUESTER, body, CREATED)
    relay.publish(COMMUNITY, "project_channel", frozen, NOW)
    return parse_event(frozen)


def requester_cancel(
    request: dict[str, object], prior_id: str | None = None, reason: str = "Stop"
) -> dict[str, object]:
    content = {
        **{key: copy.deepcopy(request["content_object"][key]) for key in COMMON_FIELDS},
        "request_event_id": request["id"],
        "action": "cancel",
        "reason": reason,
    }
    if prior_id is not None:
        content["prior_event_id"] = prior_id
    return content


class RelayIdentityTests(unittest.TestCase):
    def test_request_replay_after_expiry_and_request_uniqueness(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            frozen, _ = publish_request(relay)
            after_expiry = dt.datetime(2030, 1, 2, tzinfo=dt.timezone.utc)
            receipt = relay.publish(COMMUNITY, "project_channel", frozen, after_expiry)
            self.assertEqual(receipt["state"], "stored")
            self.assertEqual(len(relay.events(COMMUNITY)), 1)

            relay.authority.projects.clear()
            relay.authority.members.remove(
                (
                    COMMUNITY,
                    request_content()["project"]["home_channel"],
                    WORKER_A,
                )
            )
            relay.authority.capabilities.clear()
            changed_scope_receipt = relay.publish(
                COMMUNITY, "project_channel", frozen, after_expiry
            )
            self.assertEqual(changed_scope_receipt, receipt)
            relay.authority = authority()

            changed = request_content()
            changed["summary"] = "Changed under the same requester idempotency key"
            with self.assertRaisesRegex(ContractError, "idempotency key"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43001, REQUESTER, changed, CREATED + 1),
                    NOW,
                )

            other = request_content()
            other.update(
                {
                    "idempotency_key": "other-author-key",
                    "sender_pubkey": WORKER_B,
                    "recipient_pubkey": WORKER_A,
                    "sponsor": {
                        "pubkey": WORKER_B_SPONSOR,
                        "github_login": "worker-b-owner",
                    },
                }
            )
            with self.assertRaisesRegex(ContractError, "coordinator epoch"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43001, WORKER_B, other, CREATED + 2),
                    NOW,
                )

    def test_initial_request_epoch_must_be_one(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            with self.assertRaisesRegex(ContractError, "epoch must be 1"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43001, REQUESTER, request_content(epoch=2), CREATED),
                    NOW,
                )

    def test_claim_scope_digest_must_match_request(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            content = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "claim": {"status": "processed", "scope_digest": "0" * 64},
            }
            with self.assertRaisesRegex(ContractError, "scope_digest"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43002, WORKER_A, content, CREATED + 1),
                    NOW,
                )

    def test_declined_is_terminal_from_request_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            content = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "claim": {
                    "status": "declined",
                    "scope_digest": semantic_digest(request["content_object"]),
                    "reason": "capability_unavailable",
                },
            }
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(43002, WORKER_A, content, CREATED + 1),
                NOW,
            )
            self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), "declined")
            processed = copy.deepcopy(content)
            processed["claim"] = {
                "status": "processed",
                "scope_digest": semantic_digest(request["content_object"]),
            }
            with self.assertRaisesRegex(ContractError, "terminal"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43002, WORKER_A, processed, CREATED + 2),
                    NOW,
                )

    def test_terminal_audit_grace_allows_only_worker_terminal_receipts(self) -> None:
        audit_now = dt.datetime(2030, 1, 1, 0, 0, 30, tzinfo=dt.timezone.utc)

        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "declined.sqlite3", authority())
            request = publish_short_request(relay)
            declined = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "claim": {
                    "status": "declined",
                    "scope_digest": semantic_digest(request["content_object"]),
                    "reason": "capability_unavailable",
                },
            }
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(43002, WORKER_A, declined, CREATED + 30),
                audit_now,
            )
            self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), "declined")

        for outcome, retryable in (("failed", True), ("indeterminate", False)):
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as directory:
                relay = LocalRelay(Path(directory) / "error.sqlite3", authority())
                request = publish_short_request(relay)
                processed = publish_processed(relay, request)
                accepted = publish_accepted(relay, request, processed)
                failure = {
                    **worker_common(request["content_object"]),
                    "request_event_id": request["id"],
                    "prior_event_id": accepted["id"],
                    "outcome": outcome,
                    "code": "executor_failure",
                    "message": "Terminal audit receipt",
                    "retryable": retryable,
                }
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43006, WORKER_A, failure, CREATED + 30),
                    audit_now,
                )
                self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), outcome)

        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "cancelled.sqlite3", authority())
            request = publish_short_request(relay)
            processed = publish_processed(relay, request)
            accepted = publish_accepted(relay, request, processed)
            cancel_frozen = freeze_event(
                43005,
                REQUESTER,
                requester_cancel(request, accepted["id"]),
                CREATED + 3,
            )
            relay.publish(COMMUNITY, "project_channel", cancel_frozen, NOW)
            cancelled = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": parse_event(cancel_frozen)["id"],
                "action": "cancelled",
                "reason": "Executor is quiescent",
            }
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(43005, WORKER_A, cancelled, CREATED + 30),
                audit_now,
            )
            self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), "cancelled")

        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "progress.sqlite3", authority())
            request = publish_short_request(relay)
            processed = publish_processed(relay, request)
            accepted = publish_accepted(relay, request, processed)
            progress = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": accepted["id"],
                "status": "progress",
                "message": "Too late",
                "evidence": [],
            }
            with self.assertRaisesRegex(ContractError, "expired"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43003, WORKER_A, progress, CREATED + 30),
                    audit_now,
                )

        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "after-grace.sqlite3", authority())
            request = publish_short_request(relay)
            declined = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "claim": {
                    "status": "declined",
                    "scope_digest": semantic_digest(request["content_object"]),
                    "reason": "capability_unavailable",
                },
            }
            after_grace = dt.datetime(2030, 1, 2, 0, 0, 21, tzinfo=dt.timezone.utc)
            with self.assertRaisesRegex(ContractError, "outside terminal audit grace"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43002, WORKER_A, declined, CREATED + 86_421),
                    after_grace,
                )


class CancelLifecycleTests(unittest.TestCase):
    def test_root_cancel_is_terminal_before_processing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            cancel = freeze_event(43005, REQUESTER, requester_cancel(request), CREATED + 1)
            relay.publish(COMMUNITY, "project_channel", cancel, NOW)
            self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), "cancelled")
            processed = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "claim": {
                    "status": "processed",
                    "scope_digest": semantic_digest(request["content_object"]),
                },
            }
            with self.assertRaisesRegex(ContractError, "terminal"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43002, WORKER_A, processed, CREATED + 2),
                    NOW,
                )

    def test_accepted_cancel_requires_worker_cancelled_ack(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            processed = publish_processed(relay, request)
            accepted = publish_accepted(relay, request, processed)
            cancel_frozen = freeze_event(
                43005,
                REQUESTER,
                requester_cancel(request, accepted["id"]),
                CREATED + 3,
            )
            relay.publish(COMMUNITY, "project_channel", cancel_frozen, NOW)
            cancel = parse_event(cancel_frozen)
            self.assertEqual(
                relay.lifecycle_state(COMMUNITY, request["id"]), "cancel_requested"
            )

            cancelled = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": cancel["id"],
                "action": "cancelled",
                "reason": "Executor is quiescent",
            }
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(43005, WORKER_A, cancelled, CREATED + 4),
                NOW,
            )
            self.assertEqual(relay.lifecycle_state(COMMUNITY, request["id"]), "cancelled")

    def test_cancel_and_result_race_share_predecessor_fence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            processed = publish_processed(relay, request)
            accepted = publish_accepted(relay, request, processed)
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(
                    43005,
                    REQUESTER,
                    requester_cancel(request, accepted["id"]),
                    CREATED + 3,
                ),
                NOW,
            )
            result = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": accepted["id"],
                "outcome": "success",
                "artifacts": ["git:" + "d" * 40],
                "evidence": ["contract:python-unittest"],
            }
            with self.assertRaisesRegex(ContractError, "already has a lifecycle child"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43004, WORKER_A, result, CREATED + 4),
                    NOW,
                )

    def test_duplicate_cancel_and_release_after_processed_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            processed = publish_processed(relay, request)
            first = requester_cancel(request, processed["id"])
            relay.publish(
                COMMUNITY,
                "project_channel",
                freeze_event(43005, REQUESTER, first, CREATED + 2),
                NOW,
            )
            second = requester_cancel(request, processed["id"], "Stop again")
            with self.assertRaisesRegex(ContractError, "already has a lifecycle child"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43005, REQUESTER, second, CREATED + 3),
                    NOW,
                )

        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            _, request = publish_request(relay)
            processed = publish_processed(relay, request)
            release = {
                **worker_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": processed["id"],
                "action": "release",
                "reason": "Cannot claim",
            }
            with self.assertRaisesRegex(ContractError, "release/handoff"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43005, WORKER_A, release, CREATED + 2),
                    NOW,
                )


class PortableReferenceTests(unittest.TestCase):
    @staticmethod
    def result_content() -> dict[str, object]:
        return {
            **worker_common(request_content()),
            "request_event_id": "aa" * 32,
            "prior_event_id": "bb" * 32,
            "outcome": "success",
            "artifacts": [],
            "evidence": [],
        }

    def test_portable_reference_allowlist_and_denials(self) -> None:
        valid = self.result_content()
        valid["artifacts"] = [
            "git:" + "d" * 40,
            "https://github.com/nemo-project/nemo/actions/runs/1",
        ]
        valid["evidence"] = ["contract:python/tests", "buzz:event:" + "e" * 64]
        validate_content(43004, valid)
        for reference in (
            "artifact.txt",
            "https://example.com/owner/repo",
            "https://github.com/nemo-project/nemo?token=redacted",
            "https://github.com/nemo-project/nemo#fragment",
            "contract:../escape",
            "contract:unit//test",
            "buzz:event:short",
        ):
            body = self.result_content()
            body["artifacts"] = [reference]
            with self.subTest(reference=reference), self.assertRaises(ContractError):
                validate_content(43004, body)

    def test_checkout_path_resolution_rejects_metadata_traversal_and_symlink_escape(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = root / "checkout"
            outside = root / "outside"
            checkout.mkdir()
            outside.mkdir()
            (checkout / "src").mkdir()
            self.assertEqual(
                resolve_repo_path(checkout, "src/main.rs"),
                (checkout / "src/main.rs").resolve(),
            )
            for path in ("../outside", ".git/config", "src\\main.rs", "src//main.rs"):
                with self.subTest(path=path), self.assertRaises(ContractError):
                    resolve_repo_path(checkout, path)
            (checkout / "escape").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ContractError, "escapes"):
                resolve_repo_path(checkout, "escape/result.txt")


if __name__ == "__main__":
    unittest.main()
