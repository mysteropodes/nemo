from __future__ import annotations

import copy
import datetime as dt
import hashlib
import json
import subprocess
import tempfile
import unittest
import uuid
from dataclasses import replace
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
)
from authorization import (  # noqa: E402
    AUTH_SCHEMA_VERSION,
    LocalAuthorizationClient,
    LocalAuthorizationService,
    loads_strict_json,
    validate_authorization_endpoint,
    validate_response,
)
from executor import Executor, ExecutorLedger, make_handler  # noqa: E402
from local_relay import LocalRelay  # noqa: E402
from receiver_grants import ReceiverGrant, load_receiver_grants  # noqa: E402
from receiver_runtime import exclusive_receiver, scrub_spawn_environment  # noqa: E402
from two_agent_smoke import (  # noqa: E402
    COMMUNITY,
    CREATED,
    HOME_CHANNEL,
    NOW,
    PROJECT_ADDRESS,
    RELAY_HOST,
    REQUESTER,
    REQUESTER_SPONSOR,
    WORKER_A,
    WORKER_A_SPONSOR,
    CheckoutFixture,
    authorization_client,
    authority,
    create_checkout,
    request_content,
)


def setup_request(root: Path) -> tuple[LocalRelay, dict[str, object], CheckoutFixture]:
    checkout = create_checkout(root / "checkout")
    relay = LocalRelay(root / "relay.sqlite3", authority())
    frozen = freeze_event(
        43001, REQUESTER, request_content(checkout=checkout), CREATED
    )
    relay.publish(COMMUNITY, "project_channel", frozen, NOW)
    return relay, parse_event(frozen), checkout


def payload(
    request: dict[str, object],
    nonce: str = "12345678-1234-4234-8234-123456789abc",
) -> dict[str, object]:
    body = request["content_object"]
    return {
        "schema_version": AUTH_SCHEMA_VERSION,
        "nonce": nonce,
        "request_event_id": request["id"],
        "semantic_digest": semantic_digest(body),
        "community_id": COMMUNITY,
        "relay_host": RELAY_HOST,
        "channel_id": body["project"]["home_channel"],
        "project_address": body["project"]["address"],
        "repository": copy.deepcopy(body["repository"]),
        "requester_pubkey": body["sender_pubkey"],
        "recipient_pubkey": body["recipient_pubkey"],
    }


def worker_common(request: dict[str, object]) -> dict[str, object]:
    body = request["content_object"]
    common = {name: copy.deepcopy(body[name]) for name in COMMON_FIELDS}
    common.update(
        {
            "sender_pubkey": WORKER_A,
            "recipient_pubkey": REQUESTER,
            "sponsor": {
                "pubkey": WORKER_A_SPONSOR,
                "github_login": "worker-a-owner",
            },
        }
    )
    return common


def grant_value(checkout: CheckoutFixture) -> dict[str, object]:
    return {
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


def git(checkout: CheckoutFixture, *arguments: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(checkout.root), *arguments],
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


class AuthorizationContractTests(unittest.TestCase):
    def test_endpoint_policy_is_https_or_explicit_exact_loopback(self) -> None:
        self.assertEqual(
            validate_authorization_endpoint("https://relay.example/api/jobs/authorize"),
            "relay.example",
        )
        for host in ("localhost", "127.0.0.1", "[::1]"):
            with self.subTest(host=host):
                self.assertEqual(
                    validate_authorization_endpoint(
                        f"http://{host}:3000/api/jobs/authorize", dev_mode=True
                    ),
                    f"{host}:3000",
                )
        for endpoint, dev_mode in (
            ("http://localhost/api/jobs/authorize", False),
            ("http://relay.example/api/jobs/authorize", True),
            ("https://relay.example/api/jobs/authorize?community=other", False),
            ("https://user@relay.example/api/jobs/authorize", False),
            ("https://relay.example/api/jobs/authorize/extra", False),
        ):
            with self.subTest(endpoint=endpoint), self.assertRaises(ContractError):
                validate_authorization_endpoint(endpoint, dev_mode)

    def test_fresh_response_has_exact_echo_and_no_signature(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            service = LocalAuthorizationService(relay, COMMUNITY, RELAY_HOST)
            request_payload = payload(request)
            response = service.authorize(
                request_payload,
                caller_pubkey=WORKER_A,
                host_header=RELAY_HOST,
                nip98_event_id="8" * 64,
                now=NOW,
                route="project_channel",
            )
            validate_response(response, request_payload, NOW)
            self.assertEqual(response["binding"], request_payload)
            self.assertIn("repository_announcement_event_id", response)
            self.assertNotIn("grant_event_id", response)
            self.assertNotIn("signature", response)
            changed = copy.deepcopy(response)
            changed["signature"] = "not-part-of-the-contract"
            with self.assertRaisesRegex(ContractError, "exactly"):
                validate_response(changed, request_payload, NOW)
            with self.assertRaisesRegex(ContractError, "duplicate"):
                loads_strict_json(
                    '{"schema_version":"buzz.job-authorization.v1",'
                    '"schema_version":"buzz.job-authorization.v1"}'
                )

    def test_stale_response_and_server_replays_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            service = LocalAuthorizationService(relay, COMMUNITY, RELAY_HOST)
            first = payload(request)
            response = service.authorize(
                first,
                caller_pubkey=WORKER_A,
                host_header=RELAY_HOST,
                nip98_event_id="8" * 64,
                now=NOW,
                route="project_channel",
            )
            with self.assertRaisesRegex(ContractError, "stale"):
                validate_response(response, first, NOW + dt.timedelta(seconds=6))
            with self.assertRaisesRegex(ContractError, "replayed"):
                service.authorize(
                    first,
                    caller_pubkey=WORKER_A,
                    host_header=RELAY_HOST,
                    nip98_event_id="9" * 64,
                    now=NOW,
                    route="project_channel",
                )
            second = payload(request, str(uuid.uuid4()))
            with self.assertRaisesRegex(ContractError, "replayed"):
                service.authorize(
                    second,
                    caller_pubkey=WORKER_A,
                    host_header=RELAY_HOST,
                    nip98_event_id="8" * 64,
                    now=NOW,
                    route="project_channel",
                )

    def test_every_authority_binding_is_checked(self) -> None:
        mutations = {
            "community": ("community_id", "22345678-1234-4234-8234-123456789abc"),
            "relay host": ("relay_host", "other.example"),
            "channel": ("channel_id", "32345678-1234-4234-8234-123456789abc"),
            "project": ("project_address", f"30621:{'a' * 64}:other"),
            "event": ("request_event_id", "f" * 64),
            "digest": ("semantic_digest", "f" * 64),
            "requester": ("requester_pubkey", "a" * 64),
            "recipient": ("recipient_pubkey", "b" * 64),
        }
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            service = LocalAuthorizationService(relay, COMMUNITY, RELAY_HOST)
            for name, (field, value) in mutations.items():
                changed = payload(request)
                changed[field] = value
                with self.subTest(name=name), self.assertRaises(ContractError):
                    service.authorize(
                        changed,
                        caller_pubkey=changed["recipient_pubkey"],
                        host_header=changed["relay_host"],
                        nip98_event_id=hashlib.sha256(name.encode()).hexdigest(),
                        now=NOW,
                        route="project_channel",
                    )
            changed = payload(request)
            changed["repository"]["canonical"] = "https://github.com/nemo-project/other"
            with self.assertRaisesRegex(ContractError, "stored signed request"):
                service.authorize(
                    changed,
                    caller_pubkey=WORKER_A,
                    host_header=RELAY_HOST,
                    nip98_event_id="7" * 64,
                    now=NOW,
                    route="project_channel",
                )
            with self.assertRaisesRegex(ContractError, "addressed recipient"):
                service.authorize(
                    payload(request, str(uuid.uuid4())),
                    caller_pubkey=REQUESTER,
                    host_header=RELAY_HOST,
                    nip98_event_id="6" * 64,
                    now=NOW,
                    route="project_channel",
                )

    def test_receiver_local_grant_checks_owner_event_scope_and_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            client = authorization_client(relay, checkout)
            admission = client.preflight(request, NOW, "project_channel")
            self.assertRegex(admission["grant_digest"], r"^[0-9a-f]{64}$")

            bad_owner = LocalAuthorizationClient(
                client.service,
                "http://localhost/api/jobs/authorize",
                client.grant,
                replace(client.authority, recipient_owner_pubkey="a" * 64),
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "receiver authority"):
                bad_owner.preflight(request, NOW, "project_channel")

            bad_event = LocalAuthorizationClient(
                client.service,
                "http://localhost/api/jobs/authorize",
                client.grant,
                replace(client.authority, project_head_event_id="b" * 64),
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "receiver authority"):
                bad_event.preflight(request, NOW, "project_channel")

            bad_path = LocalAuthorizationClient(
                client.service,
                "http://localhost/api/jobs/authorize",
                replace(client.grant, path_prefixes=("src",)),
                client.authority,
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "receiver-local grant"):
                bad_path.preflight(request, NOW, "project_channel")

            bad_checkout_coordinate = LocalAuthorizationClient(
                client.service,
                "http://localhost/api/jobs/authorize",
                replace(client.grant, branch="codex/not-authorized"),
                client.authority,
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "receiver-local grant"):
                bad_checkout_coordinate.preflight(request, NOW, "project_channel")

            ambiguous = LocalAuthorizationClient(
                client.service,
                "http://localhost/api/jobs/authorize",
                (client.grant, client.grant),
                client.authority,
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "exactly one"):
                ambiguous.preflight(request, NOW, "project_channel")

    def test_receiver_grant_canonicalizes_an_absolute_checkout_alias(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            relay, request, checkout = setup_request(root)
            alias = root / "tmp-checkout"
            alias.symlink_to(checkout.root, target_is_directory=True)

            baseline = authorization_client(relay, checkout)
            value = {
                "project_address": baseline.grant.project_address,
                "home_channel": baseline.grant.home_channel,
                "repository": baseline.grant.repository,
                "requester_pubkeys": list(baseline.grant.requester_pubkeys),
                "capabilities": list(baseline.grant.capabilities),
                "path_prefixes": list(baseline.grant.path_prefixes),
                "base_sha": baseline.grant.base_sha,
                "branch": baseline.grant.branch,
                "worktree_id": baseline.grant.worktree_id,
                "checkout_root": str(alias),
            }
            canonical_grant = ReceiverGrant.from_value(value)
            client = LocalAuthorizationClient(
                baseline.service,
                "http://localhost/api/jobs/authorize",
                canonical_grant,
                baseline.authority,
                dev_mode=True,
            )

            admission = client.preflight(request, NOW, "project_channel")
            self.assertEqual(canonical_grant.checkout_root, str(checkout.root))
            self.assertRegex(admission["grant_digest"], r"^[0-9a-f]{64}$")

    def test_receiver_grant_document_requires_exact_scalar_checkout_fields(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            checkout = create_checkout(Path(directory) / "checkout")
            valid = grant_value(checkout)
            loaded = load_receiver_grants(json.dumps({"version": 1, "grants": [valid]}))
            self.assertEqual(loaded[0].base_sha, checkout.base_sha)
            self.assertEqual(loaded[0].path_prefixes, (".agents/skills/nemo-a2a",))

            invalid_values: list[dict[str, object]] = []
            for missing in ("base_sha", "branch", "worktree_id", "checkout_root"):
                candidate = copy.deepcopy(valid)
                del candidate[missing]
                invalid_values.append(candidate)
            for scalar in ("base_sha", "branch", "worktree_id", "checkout_root"):
                candidate = copy.deepcopy(valid)
                candidate[scalar] = [candidate[scalar]]
                invalid_values.append(candidate)
            for prefixes in ([], ["src/.GiT/config"]):
                candidate = copy.deepcopy(valid)
                candidate["path_prefixes"] = prefixes
                invalid_values.append(candidate)
            relative = copy.deepcopy(valid)
            relative["checkout_root"] = "checkout"
            invalid_values.append(relative)
            legacy = copy.deepcopy(valid)
            legacy["branches"] = [legacy.pop("branch")]
            invalid_values.append(legacy)

            for candidate in invalid_values:
                with self.subTest(candidate=candidate), self.assertRaises(ContractError):
                    load_receiver_grants(
                        json.dumps({"version": 1, "grants": [candidate]})
                    )

    def test_receiver_live_checkout_identity_is_revalidated_on_every_admission(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))

            git(checkout, "checkout", "-b", "codex/drift")
            with self.assertRaisesRegex(ContractError, "live checkout"):
                authorization_client(relay, checkout).preflight(
                    request, NOW, "project_channel"
                )
            git(checkout, "checkout", checkout.branch)

            (checkout.root / "drift.txt").write_text("drift\n")
            git(checkout, "add", "drift.txt")
            git(checkout, "commit", "-m", "test: drift checkout head")
            with self.assertRaisesRegex(ContractError, "live checkout"):
                authorization_client(relay, checkout).preflight(
                    request, NOW, "project_channel"
                )
            git(checkout, "reset", "--hard", checkout.base_sha)

            git(checkout, "remote", "set-url", "origin", "https://github.com/block/buzz.git")
            with self.assertRaisesRegex(ContractError, "live checkout"):
                authorization_client(relay, checkout).preflight(
                    request, NOW, "project_channel"
                )
            git(checkout, "remote", "set-url", "origin", checkout.repository + ".git")
            authorization_client(relay, checkout).preflight(
                request, NOW, "project_channel"
            )

    def test_receiver_rejects_checkout_subdirectory_as_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            subdirectory = checkout.root / ".agents"
            candidate = grant_value(checkout)
            candidate["checkout_root"] = str(subdirectory)
            grant = ReceiverGrant.from_value(candidate)
            baseline = authorization_client(relay, checkout)
            client = LocalAuthorizationClient(
                baseline.service,
                "http://localhost/api/jobs/authorize",
                grant,
                baseline.authority,
                dev_mode=True,
            )
            with self.assertRaisesRegex(ContractError, "live checkout"):
                client.preflight(request, NOW, "project_channel")

    def test_accepted_ingest_revalidates_after_preflight(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            relay, request, checkout = setup_request(Path(directory))
            digest = semantic_digest(request["content_object"])
            processed_content = {
                **worker_common(request),
                "request_event_id": request["id"],
                "claim": {"status": "processed", "scope_digest": digest},
            }
            processed_frozen = freeze_event(43002, WORKER_A, processed_content, CREATED + 1)
            relay.publish(COMMUNITY, "project_channel", processed_frozen, NOW)
            authorization_client(relay, checkout).preflight(request, NOW, "project_channel")

            accepted_content = {
                **worker_common(request),
                "request_event_id": request["id"],
                "prior_event_id": parse_event(processed_frozen)["id"],
                "claim": {"status": "accepted", "scope_digest": digest},
            }
            relay.authority.members.remove((COMMUNITY, HOME_CHANNEL, WORKER_A))
            with self.assertRaisesRegex(ContractError, "current community member"):
                relay.publish(
                    COMMUNITY,
                    "project_channel",
                    freeze_event(43002, WORKER_A, accepted_content, CREATED + 2),
                    NOW,
                )

    def test_authorization_id_is_consumed_once_inside_admission_cas(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            relay, request, checkout = setup_request(root)
            admission = authorization_client(relay, checkout).preflight(
                request, NOW, "project_channel"
            )
            ledger = ExecutorLedger(root / "worker.sqlite3")
            digest = semantic_digest(request["content_object"])
            ledger.observe(COMMUNITY, request, digest, "processed-a")
            self.assertTrue(
                ledger.admit(COMMUNITY, request, "accepted-a", admission)
            )

            second_body = request_content(checkout=checkout)
            second_body["operation_id"] = "22345678-1234-4234-8234-123456789abc"
            second_body["idempotency_key"] = "second-admission"
            second = parse_event(freeze_event(43001, REQUESTER, second_body, CREATED + 10))
            ledger.observe(
                COMMUNITY,
                second,
                semantic_digest(second["content_object"]),
                "processed-b",
            )
            with self.assertRaisesRegex(ContractError, "already consumed"):
                ledger.admit(COMMUNITY, second, "accepted-b", admission)
            self.assertEqual(ledger.row(COMMUNITY, second)["status"], "processed")
            self.assertEqual(ledger.admission_count(), 1)


class ReceiverRuntimeTests(unittest.TestCase):
    def test_frozen_outbox_retries_a_lost_ack_without_duplicate_execution(self) -> None:
        class LostAckRelay:
            def __init__(self, relay: LocalRelay) -> None:
                self.relay = relay
                self.failed = False
                self.attempts: list[str] = []

            def __getattr__(self, name: str):
                return getattr(self.relay, name)

            def publish(self, community: str, route: str, frozen: str, now: dt.datetime):
                self.attempts.append(frozen)
                receipt = self.relay.publish(community, route, frozen, now)
                if not self.failed:
                    self.failed = True
                    raise ConnectionError("simulated lost relay ACK")
                return receipt

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            relay, _, checkout = setup_request(root)
            lossy = LostAckRelay(relay)
            ledger = ExecutorLedger(root / "worker.sqlite3")
            worker = Executor(
                "worker-a",
                WORKER_A,
                WORKER_A_SPONSOR,
                "worker-a-owner",
                ledger,
                lossy,
                authorization_client(relay, checkout),
                make_handler(ledger, COMMUNITY, "worker-a", "result"),
            )
            result = worker.drain(COMMUNITY, "project_channel", NOW)
            self.assertEqual(result[0]["disposition"], "executed")
            self.assertEqual(lossy.attempts[0], lossy.attempts[1])
            self.assertEqual(ledger.effect_count(), 1)
            self.assertEqual(ledger.admission_count(), 1)
            self.assertEqual(len(relay.events(COMMUNITY)), 4)

    def test_receiver_ledger_has_one_process_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "receiver.lock"
            with exclusive_receiver(lock):
                with self.assertRaisesRegex(ContractError, "active process owner"):
                    with exclusive_receiver(lock):
                        self.fail("second receiver acquired the same ledger lock")

    def test_spawned_model_environment_has_no_signing_or_control_secret(self) -> None:
        source = {
            "PATH": "/usr/bin",
            "SAFE_TASK": "smoke",
            "BUZZ_PRIVATE_KEY": "redacted",
            "NOSTR_PRIVATE_KEY": "redacted",
            "BUZZ_AUTH_TAG": "redacted",
            "A2A_SIGNING_SECRET": "redacted",
            "JOB_CONTROL_TOKEN": "redacted",
        }
        child = scrub_spawn_environment(source)
        self.assertEqual(child, {"PATH": "/usr/bin", "SAFE_TASK": "smoke"})


if __name__ == "__main__":
    unittest.main()
