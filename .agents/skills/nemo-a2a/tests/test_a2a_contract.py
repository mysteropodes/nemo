from __future__ import annotations

import copy
import datetime as dt
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

from a2a_contract import (  # noqa: E402
    COMMON_FIELDS,
    ContractError,
    IdempotencyConflict,
    canonical_json,
    freeze_event,
    parse_event,
    semantic_digest,
    validate_content,
)
from executor import Executor, ExecutorLedger, make_handler  # noqa: E402
from local_relay import LocalRelay  # noqa: E402
from two_agent_smoke import (  # noqa: E402
    COMMUNITY,
    CREATED,
    EXPIRES,
    NOW,
    REQUESTER,
    REQUESTER_SPONSOR,
    WORKER_A,
    WORKER_A_SPONSOR,
    WORKER_B,
    WORKER_B_SPONSOR,
    authorization_client,
    authority,
    create_checkout,
    request_content,
    run_smoke,
    superseding_request,
)


class ContractTests(unittest.TestCase):
    def test_contract_cli_entrypoint_remains_usable(self) -> None:
        body = request_content()
        result = subprocess.run(
            [sys.executable, str(SCRIPTS / "a2a_contract.py"), "digest-request"],
            input=canonical_json(body),
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["scope_digest"], semantic_digest(body))

    def test_result_smoke_is_exactly_once_after_reconnect(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = run_smoke(Path(directory), "result")
        self.assertEqual(result["event_counts"], {
            "request": 1, "processed": 1, "accepted": 1, "result": 1, "handoff": 0
        })
        self.assertEqual(result["execution_count"], 1)
        self.assertEqual(result["side_effect_count"], 1)
        self.assertEqual(result["reconnect_disposition"], "replayed")
        self.assertTrue(result["frozen_receipt_bytes_replayed"])
        self.assertIsNone(result["relay_ack_lifecycle"])

    def test_cross_language_fixture_corpus(self) -> None:
        fixture_path = Path(__file__).parent / "fixtures/jobs-v1.json"
        corpus = json.loads(fixture_path.read_text())
        self.assertEqual(corpus["fixture_schema"], "nemo.a2a-jobs-fixtures.v1")
        for case in corpus["cases"]:
            with self.subTest(case=case["name"]):
                unsigned = {
                    "kind": case["kind"],
                    "pubkey": case["author_pubkey"],
                    "created_at": corpus["created_at"],
                    "tags": case["tags"],
                    "content": canonical_json(case["content"]),
                }
                frozen = canonical_json({
                    "id": hashlib.sha256(canonical_json(unsigned).encode()).hexdigest(),
                    **unsigned,
                })
                if case["valid"]:
                    self.assertEqual(parse_event(frozen)["kind"], case["kind"])
                    if "semantic_digest" in case:
                        self.assertEqual(
                            semantic_digest(case["content"]), case["semantic_digest"]
                        )
                else:
                    with self.assertRaises(ContractError) as raised:
                        parse_event(frozen)
                    self.assertIn(case["error_contains"], str(raised.exception))

    def test_handoff_alone_never_executes_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = run_smoke(Path(directory), "handoff")
        self.assertEqual(result["event_counts"], {
            "request": 1, "processed": 1, "accepted": 1, "result": 0, "handoff": 1
        })
        self.assertTrue(result["handoff_requires_new_request"])

    def test_superseding_request_is_required_before_target_executes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = create_checkout(root / "checkout")
            relay = LocalRelay(root / "relay.sqlite3", authority())
            request_frozen = freeze_event(
                43001, REQUESTER, request_content(checkout=checkout), CREATED
            )
            relay.publish(COMMUNITY, "project_channel", request_frozen, NOW)
            ledger_a = ExecutorLedger(root / "a.sqlite3")
            worker_a = Executor(
                "a", WORKER_A, WORKER_A_SPONSOR, "owner-a", ledger_a, relay,
                authorization_client(relay, checkout),
                make_handler(ledger_a, COMMUNITY, "a", "handoff", WORKER_B),
            )
            worker_a.drain(COMMUNITY, "project_channel", NOW)
            handoff = next(event for event in relay.events(COMMUNITY) if event["kind"] == 43005)

            ledger_b = ExecutorLedger(root / "b.sqlite3")
            worker_b = Executor(
                "b", WORKER_B, WORKER_B_SPONSOR, "owner-b", ledger_b, relay,
                authorization_client(relay, checkout, WORKER_B, WORKER_B_SPONSOR),
                make_handler(ledger_b, COMMUNITY, "b", "result"),
            )
            self.assertEqual(worker_b.drain(COMMUNITY, "project_channel", NOW), [])
            self.assertEqual(ledger_b.effect_count(), 0)
            relay.publish(
                COMMUNITY,
                "project_channel",
                superseding_request(handoff, checkout),
                NOW,
            )
            result = worker_b.drain(COMMUNITY, "project_channel", NOW)
            self.assertEqual(result[0]["disposition"], "executed")
            self.assertEqual(ledger_b.effect_count(), 1)

    def test_superseding_request_requires_exact_next_epoch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = create_checkout(root / "checkout")
            relay = LocalRelay(root / "relay.sqlite3", authority())
            original = freeze_event(
                43001, REQUESTER, request_content(checkout=checkout), CREATED
            )
            relay.publish(COMMUNITY, "project_channel", original, NOW)
            ledger = ExecutorLedger(root / "a.sqlite3")
            Executor(
                "a", WORKER_A, WORKER_A_SPONSOR, "owner-a", ledger, relay,
                authorization_client(relay, checkout),
                make_handler(ledger, COMMUNITY, "a", "handoff", WORKER_B),
            ).drain(COMMUNITY, "project_channel", NOW)
            handoff = next(event for event in relay.events(COMMUNITY) if event["kind"] == 43005)
            body = request_content(WORKER_B, 1, checkout)
            body["supersedes_event_id"] = handoff["id"]
            invalid = freeze_event(43001, REQUESTER, body, CREATED + 4)
            with self.assertRaisesRegex(ContractError, "next coordinator_epoch"):
                relay.publish(COMMUNITY, "project_channel", invalid, NOW)

    def test_receiver_ledger_conflicts_on_same_key_changed_body(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = create_checkout(root / "checkout")
            relay = LocalRelay(root / "relay.sqlite3", authority())
            original = freeze_event(
                43001, REQUESTER, request_content(checkout=checkout), CREATED
            )
            relay.publish(COMMUNITY, "project_channel", original, NOW)
            ledger = ExecutorLedger(root / "worker.sqlite3")
            worker = Executor(
                "a", WORKER_A, WORKER_A_SPONSOR, "owner-a", ledger, relay,
                authorization_client(relay, checkout),
                make_handler(ledger, COMMUNITY, "a", "result"),
            )
            worker.drain(COMMUNITY, "project_channel", NOW)
            changed = request_content(checkout=checkout)
            changed["summary"] = "Changed semantics under the same retry key"
            changed_frozen = freeze_event(43001, REQUESTER, changed, CREATED + 4)
            with self.assertRaises(IdempotencyConflict):
                worker.process(COMMUNITY, "project_channel", changed_frozen, NOW)

    def test_event_author_and_closed_tags_are_authoritative(self) -> None:
        body = request_content()
        with self.assertRaisesRegex(ContractError, "event author"):
            freeze_event(43001, WORKER_A, body, CREATED)
        frozen = freeze_event(43001, REQUESTER, body, CREATED)
        event = json.loads(frozen)
        event["tags"].append(["h", body["project"]["home_channel"]])
        with self.assertRaisesRegex(ContractError, "closed canonical"):
            parse_event(canonical_json(event))
        event = json.loads(frozen)
        event["tags"].append(["client", "unbounded"])
        with self.assertRaisesRegex(ContractError, "closed canonical"):
            parse_event(canonical_json(event))

        result = self._result_content()
        frozen = freeze_event(43004, WORKER_A, result, CREATED + 3)
        event = json.loads(frozen)
        e_indexes = [index for index, tag in enumerate(event["tags"]) if tag[0] == "e"]
        event["tags"][e_indexes[0]], event["tags"][e_indexes[1]] = (
            event["tags"][e_indexes[1]], event["tags"][e_indexes[0]]
        )
        unsigned = {
            key: event[key] for key in ("kind", "pubkey", "created_at", "tags", "content")
        }
        event["id"] = hashlib.sha256(canonical_json(unsigned).encode()).hexdigest()
        with self.assertRaisesRegex(ContractError, "canonical root/reply order"):
            parse_event(canonical_json(event))

    def test_github_and_a_tags_are_exact_body_bindings(self) -> None:
        frozen = freeze_event(43001, REQUESTER, request_content(), CREATED)
        event = json.loads(frozen)
        event["tags"] = [tag for tag in event["tags"] if tag[0] != "github-issue"]
        with self.assertRaisesRegex(ContractError, "closed canonical"):
            parse_event(canonical_json(event))
        event = json.loads(frozen)
        next(tag for tag in event["tags"] if tag[0] == "a")[1] = "30621:" + "aa" * 32 + ":other"
        with self.assertRaisesRegex(ContractError, "closed canonical"):
            parse_event(canonical_json(event))

    def test_github_ids_are_positive_decimal_and_issue_pr_are_exclusive(self) -> None:
        for invalid in ("0", "01", "-1", "one", "1" * 21):
            body = request_content()
            body["repository"]["github_issue"] = invalid
            with self.subTest(identifier=invalid), self.assertRaisesRegex(
                ContractError, "positive decimal"
            ):
                validate_content(43001, body)
        body = request_content()
        body["repository"]["github_pr"] = "2"
        with self.assertRaisesRegex(ContractError, "mutually exclusive"):
            validate_content(43001, body)

    def test_wire_repository_fields_match_rust_contract(self) -> None:
        for branch in ("codex/a2a", "release-1.0", "_portable/topic"):
            body = request_content()
            body["repository"]["branch"] = branch
            validate_content(43001, body)
        for branch in (
            "@", "/topic", "topic/", "topic.", "a//b", "a..b", "a@{b",
            "a~b", "a^b", "a:b", "a?b", "a*b", "a[b", "a\\b",
            ".hidden", "a/.hidden", "a.lock", "a/a.lock", "a\x7f",
        ):
            body = request_content()
            body["repository"]["branch"] = branch
            with self.subTest(branch=branch), self.assertRaises(ContractError):
                validate_content(43001, body)

        body = request_content()
        body["repository"]["paths"] = ["~checkout/file"]
        validate_content(43001, body)
        for path in (
            "path/",
            "./path",
            "path/../escape",
            ".git/config",
            "src/.GiT/config",
        ):
            body = request_content()
            body["repository"]["paths"] = [path]
            with self.subTest(path=path), self.assertRaises(ContractError):
                validate_content(43001, body)

        for reference in (
            "git:" + "a" * 40,
            "buzz:event:" + "b" * 64,
            "https://github.com/nemo-project/nemo/actions/runs/1",
        ):
            body = request_content()
            body["repository"]["contracts"] = [reference]
            with self.subTest(reference=reference), self.assertRaisesRegex(
                ContractError, "only inert contract:"
            ):
                validate_content(43001, body)

    def test_project_identifier_and_uuid_spelling_match_rust_contract(self) -> None:
        for identifier in ("nemo", "project_1.0-beta"):
            body = request_content()
            owner = body["project"]["address"].split(":", 2)[1]
            body["project"]["address"] = f"30621:{owner}:{identifier}"
            validate_content(43001, body)
        for identifier in ("with space", "nested/id", "colon:id", "emoji-🦆"):
            body = request_content()
            owner = body["project"]["address"].split(":", 2)[1]
            body["project"]["address"] = f"30621:{owner}:{identifier}"
            with self.subTest(identifier=identifier), self.assertRaises(ContractError):
                validate_content(43001, body)
        for field_path in ("operation_id", "project.home_channel"):
            body = request_content()
            if field_path == "operation_id":
                body["operation_id"] = "00000000-0000-0000-0000-000000000000"
            else:
                body["project"]["home_channel"] = "00000000-0000-0000-0000-000000000000"
            with self.subTest(field=field_path), self.assertRaisesRegex(
                ContractError, "non-nil"
            ):
                validate_content(43001, body)

    def test_rust_inert_github_url_semantics_are_mirrored(self) -> None:
        result = self._result_content()
        for reference in (
            "https://github.com",
            "https://github.com/",
            "HTTPS://GITHUB.COM/Owner/Repo/Artifact",
            "https://github.com:443/owner/repo",
        ):
            result["evidence"] = [reference]
            with self.subTest(reference=reference):
                validate_content(43004, result)

    def test_unsigned_authority_metadata_is_not_accepted_in_content(self) -> None:
        for field in ("community", "authorization", "route", "event_type", "protocol_version"):
            body = request_content()
            body[field] = "attacker-selected"
            with self.assertRaisesRegex(ContractError, "unknown fields"):
                validate_content(43001, body)

    def test_duplicate_json_keys_are_rejected(self) -> None:
        frozen = freeze_event(43001, REQUESTER, request_content(), CREATED)
        event = json.loads(frozen)
        event["content"] = event["content"].replace(
            '"schema_version":"buzz.jobs.v1"',
            '"schema_version":"buzz.jobs.v1","schema_version":"buzz.jobs.v1"',
        )
        with self.assertRaisesRegex(ContractError, "duplicate JSON key"):
            parse_event(canonical_json(event))

    def test_project_channel_repository_triple_is_allowlisted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            body = request_content()
            body["repository"]["canonical"] = "https://github.com/nemo-project/other"
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            with self.assertRaisesRegex(ContractError, "allowlisted together"):
                relay.publish(
                    COMMUNITY, "project_channel",
                    freeze_event(43001, REQUESTER, body, CREATED), NOW,
                )

    def test_cross_owner_dm_and_sponsor_inheritance_are_forbidden(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            auth = authority()
            relay = LocalRelay(Path(directory) / "relay.sqlite3", auth)
            request = freeze_event(43001, REQUESTER, request_content(), CREATED)
            with self.assertRaisesRegex(ContractError, "cross-owner DM"):
                relay.publish(COMMUNITY, "dm", request, NOW)
        with tempfile.TemporaryDirectory() as directory:
            auth = authority()
            auth.members.remove((COMMUNITY, request_content()["project"]["home_channel"], WORKER_A))
            relay = LocalRelay(Path(directory) / "relay.sqlite3", auth)
            with self.assertRaisesRegex(ContractError, "direct project-home-channel member"):
                relay.publish(COMMUNITY, "project_channel", request, NOW)

    def test_same_owner_dm_still_requires_both_direct_members(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            auth = authority()
            auth.sponsors[(COMMUNITY, WORKER_A)] = REQUESTER_SPONSOR
            body = request_content()
            relay = LocalRelay(Path(directory) / "relay.sqlite3", auth)
            relay.publish(COMMUNITY, "dm", freeze_event(43001, REQUESTER, body, CREATED), NOW)
            self.assertEqual(len(relay.events(COMMUNITY)), 1)

    def test_server_ttl_cap_and_expiry_are_enforced(self) -> None:
        for expires, message in (
            ("2030-01-01T00:00:09Z", "expired"),
            ("2030-01-08T00:00:11Z", "seven-day TTL"),
        ):
            with self.subTest(expires=expires), tempfile.TemporaryDirectory() as directory:
                body = request_content()
                body["expires_at"] = expires
                relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
                with self.assertRaisesRegex(ContractError, message):
                    relay.publish(COMMUNITY, "project_channel", freeze_event(43001, REQUESTER, body, CREATED), NOW)
        with tempfile.TemporaryDirectory() as directory:
            body = request_content()
            body["expires_at"] = "2030-01-08T00:00:01Z"
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            later_now = dt.datetime(2030, 1, 2, tzinfo=dt.timezone.utc)
            with self.assertRaisesRegex(ContractError, "event-created-at"):
                relay.publish(
                    COMMUNITY, "project_channel",
                    freeze_event(43001, REQUESTER, body, CREATED), later_now,
                )

    def test_host_local_paths_and_unsafe_repository_urls_are_rejected(self) -> None:
        for canonical in (
            "https://user:token@github.com/nemo-project/nemo",
            "https://github.com/nemo-project/nemo.git",
            "https://github.com/nemo-project/nemo?token=x",
            "https://github.com/nemo-project/nemo/",
        ):
            body = request_content()
            body["repository"]["canonical"] = canonical
            with self.assertRaises(ContractError):
                validate_content(43001, body)
        result = self._result_content()
        result["evidence"] = ["/" + "Users/example/private/log.txt"]
        with self.assertRaisesRegex(ContractError, "host-local path"):
            validate_content(43004, result)
        request = request_content()
        request["repository"]["contracts"] = ["python3 /" + "Users/example/private/check.py"]
        with self.assertRaisesRegex(ContractError, "host-local path"):
            validate_content(43001, request)
        request = request_content()
        request["summary"] = "use token github_" + "pat_secretvalue1"
        with self.assertRaisesRegex(ContractError, "credential material"):
            validate_content(43001, request)

    def test_request_and_followup_correlation_fields_are_kind_bound(self) -> None:
        body = request_content()
        body["request_event_id"] = "aa" * 32
        with self.assertRaisesRegex(ContractError, "unknown fields"):
            validate_content(43001, body)
        result = self._result_content()
        del result["prior_event_id"]
        with self.assertRaisesRegex(ContractError, "missing fields"):
            validate_content(43004, result)

    def test_lifecycle_fork_after_terminal_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = create_checkout(root / "checkout")
            relay = LocalRelay(root / "relay.sqlite3", authority())
            request_frozen = freeze_event(
                43001, REQUESTER, request_content(checkout=checkout), CREATED
            )
            relay.publish(COMMUNITY, "project_channel", request_frozen, NOW)
            ledger = ExecutorLedger(root / "worker.sqlite3")
            Executor(
                "a", WORKER_A, WORKER_A_SPONSOR, "owner-a", ledger, relay,
                authorization_client(relay, checkout),
                make_handler(ledger, COMMUNITY, "a", "result"),
            ).drain(COMMUNITY, "project_channel", NOW)
            request = parse_event(request_frozen)
            accepted = next(
                event for event in relay.events(COMMUNITY)
                if event["kind"] == 43002 and event["content_object"]["claim"]["status"] == "accepted"
            )
            progress = {
                **self._response_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": accepted["id"],
                "status": "progress",
                "message": "Fork after terminal",
                "evidence": [],
            }
            with self.assertRaisesRegex(ContractError, "terminal"):
                relay.publish(
                    COMMUNITY, "project_channel",
                    freeze_event(43003, WORKER_A, progress, CREATED + 5), NOW,
                )

    def test_duplicate_claim_slots_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            relay = LocalRelay(root / "relay.sqlite3", authority())
            request, processed, accepted = self._publish_claims(relay)
            duplicate_processed = freeze_event(
                43002,
                WORKER_A,
                copy.deepcopy(processed["content_object"]),
                CREATED + 8,
            )
            with self.assertRaisesRegex(ContractError, "duplicate processed"):
                relay.publish(COMMUNITY, "project_channel", duplicate_processed, NOW)
            duplicate_accepted = freeze_event(
                43002,
                WORKER_A,
                copy.deepcopy(accepted["content_object"]),
                CREATED + 9,
            )
            with self.assertRaisesRegex(ContractError, "duplicate accepted"):
                relay.publish(COMMUNITY, "project_channel", duplicate_accepted, NOW)
            self.assertEqual(request["kind"], 43001)

    def test_each_prior_has_only_one_lifecycle_child(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay = LocalRelay(Path(directory) / "relay.sqlite3", authority())
            request, _, accepted = self._publish_claims(relay)
            first = {
                **self._response_common(request["content_object"]),
                "request_event_id": request["id"],
                "prior_event_id": accepted["id"],
                "status": "progress",
                "message": "First child",
                "evidence": [],
            }
            relay.publish(
                COMMUNITY, "project_channel", freeze_event(43003, WORKER_A, first, CREATED + 3), NOW
            )
            sibling = copy.deepcopy(first)
            sibling["message"] = "Parallel sibling"
            with self.assertRaisesRegex(ContractError, "already has a lifecycle child"):
                relay.publish(
                    COMMUNITY, "project_channel",
                    freeze_event(43003, WORKER_A, sibling, CREATED + 4), NOW,
                )

    @staticmethod
    def _response_common(request: dict[str, object]) -> dict[str, object]:
        fields = {
            key: copy.deepcopy(request[key]) for key in (
                "schema_version", "operation_id", "idempotency_key", "coordinator_epoch",
                "project", "repository", "expires_at",
            )
        }
        fields.update({
            "sender_pubkey": WORKER_A,
            "recipient_pubkey": REQUESTER,
            "sponsor": {"pubkey": WORKER_A_SPONSOR, "github_login": "owner-a"},
        })
        return fields

    def _result_content(self) -> dict[str, object]:
        return {
            **self._response_common(request_content()),
            "request_event_id": "aa" * 32,
            "prior_event_id": "bb" * 32,
            "outcome": "success",
            "artifacts": [],
            "evidence": [],
        }

    def _publish_claims(
        self, relay: LocalRelay, accepted: bool = True
    ) -> tuple[dict[str, object], dict[str, object], dict[str, object] | None]:
        request_frozen = freeze_event(43001, REQUESTER, request_content(), CREATED)
        relay.publish(COMMUNITY, "project_channel", request_frozen, NOW)
        request = parse_event(request_frozen)
        digest = semantic_digest(request["content_object"])
        processed_content = {
            **self._response_common(request["content_object"]),
            "request_event_id": request["id"],
            "claim": {"status": "processed", "scope_digest": digest},
        }
        processed_frozen = freeze_event(43002, WORKER_A, processed_content, CREATED + 1)
        relay.publish(COMMUNITY, "project_channel", processed_frozen, NOW)
        processed = parse_event(processed_frozen)
        if not accepted:
            return request, processed, None
        accepted_content = {
            **self._response_common(request["content_object"]),
            "request_event_id": request["id"],
            "prior_event_id": processed["id"],
            "claim": {"status": "accepted", "scope_digest": digest},
        }
        accepted_frozen = freeze_event(43002, WORKER_A, accepted_content, CREATED + 2)
        relay.publish(COMMUNITY, "project_channel", accepted_frozen, NOW)
        return request, processed, parse_event(accepted_frozen)


if __name__ == "__main__":
    unittest.main()
