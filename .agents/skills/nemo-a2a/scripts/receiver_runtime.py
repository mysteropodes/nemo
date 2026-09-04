#!/usr/bin/env python3
"""Receiver process-safety helpers used by the deterministic A2A proof."""

from __future__ import annotations

import fcntl
import os
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator, Mapping

from a2a_contract import ContractError

EXACT_SECRET_NAMES = {
    "BUZZ_PRIVATE_KEY",
    "NOSTR_PRIVATE_KEY",
    "BUZZ_AUTH_TAG",
}
SECRET_NAME_MARKERS = (
    "PRIVATE_KEY",
    "SIGNING",
    "SIGNER",
    "JOB_CONTROL",
)
AUTH_NAME_MARKERS = ("KEY", "TOKEN", "AUTH", "SIGN", "SECRET")


def scrub_spawn_environment(source: Mapping[str, str]) -> dict[str, str]:
    """Return a child/model environment with signing and job-control secrets removed."""

    result: dict[str, str] = {}
    for name, value in source.items():
        upper = name.upper()
        provider_credential = upper.startswith(("BUZZ_", "NOSTR_")) and any(
            marker in upper for marker in AUTH_NAME_MARKERS
        )
        if (
            upper in EXACT_SECRET_NAMES
            or provider_credential
            or any(marker in upper for marker in SECRET_NAME_MARKERS)
        ):
            continue
        result[name] = value
    return result


@contextmanager
def exclusive_receiver(lock_path: Path) -> Iterator[None]:
    """Hold one non-blocking process-wide receiver lock for a ledger."""

    lock_path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ContractError("receiver ledger already has an active process owner") from error
        yield
    finally:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)


def publish_frozen_with_retry(relay, community: str, route: str, frozen: str, now, attempts: int = 3):
    """Retry a transient relay ACK with the exact already-frozen event bytes."""

    if attempts < 1:
        raise ValueError("attempts must be positive")
    last_error: Exception | None = None
    for _ in range(attempts):
        try:
            return relay.publish(community, route, frozen, now)
        except (ConnectionError, TimeoutError) as error:
            last_error = error
    assert last_error is not None
    raise last_error
