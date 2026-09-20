"""Redact evaluation artifacts before they are persisted or inspected."""
from __future__ import annotations

import os
import re
from typing import Any, Iterable
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

REDACTED = "[REDACTED]"
REDACTION_VERSION = "v1"
_SECRET_NAME = re.compile(r"(?:api[_-]?key|access[_-]?token|refresh[_-]?token|authorization|cookie|password|secret|oauth[_-]?code)", re.I)
_BEARER = re.compile(r"\b(?:bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}", re.I)
_KEY = re.compile(r"\b(?:sk|pk|rk|ghp|xox[baprs])[-_][A-Za-z0-9_-]{8,}\b", re.I)
_EMAIL = re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.I)
_PHONE = re.compile(r"(?<!\w)(?:\+?\d[\s().-]?){8,}\d(?!\w)")
_QUERY_SECRET = {"access_token", "api_key", "apikey", "code", "id_token", "password", "refresh_token", "secret", "token"}


def configured_secrets(environment: dict[str, str] | None = None) -> tuple[str, ...]:
    """Return only values intentionally supplied to the evaluation environment."""
    environment = environment or os.environ
    return tuple(value for name, value in environment.items() if name.startswith("ASSISTANT_EVAL_SECRET_") and value)


def _redact_text(text: str, secrets: Iterable[str]) -> str:
    for secret in secrets:
        if secret:
            text = text.replace(secret, REDACTED)
    text = _BEARER.sub(REDACTED, text)
    text = _KEY.sub(REDACTED, text)
    text = _EMAIL.sub(REDACTED, text)
    return _PHONE.sub(REDACTED, text)


def _redact_url(text: str) -> str:
    parsed = urlsplit(text)
    if not parsed.scheme or not parsed.query:
        return text
    query = [(key, REDACTED if key.lower() in _QUERY_SECRET else value) for key, value in parse_qsl(parsed.query, keep_blank_values=True)]
    return urlunsplit((parsed.scheme, parsed.netloc, parsed.path, urlencode(query), parsed.fragment))


def redact(value: Any, secrets: Iterable[str] = ()) -> Any:
    """Return a redacted copy; this never mutates caller-owned trace data."""
    if isinstance(value, dict):
        return {key: REDACTED if _SECRET_NAME.search(str(key)) else redact(item, secrets) for key, item in value.items()}
    if isinstance(value, list):
        return [redact(item, secrets) for item in value]
    if isinstance(value, str):
        return _redact_url(_redact_text(value, secrets))
    return value


def secret_violations(value: Any, secrets: Iterable[str] = ()) -> list[str]:
    """Return non-sensitive labels for values that must not enter an artifact."""
    rendered = str(value)
    findings: list[str] = []
    if any(secret and secret in rendered for secret in secrets):
        findings.append("configured_secret")
    if _BEARER.search(rendered) or _KEY.search(rendered):
        findings.append("credential_pattern")
    if _EMAIL.search(rendered) or _PHONE.search(rendered):
        findings.append("personal_identifier")
    if _SECRET_NAME.search(rendered) and REDACTED not in rendered:
        findings.append("secret_field")
    return findings
