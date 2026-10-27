"""Credential-safe error reports: frame locations and selected exception details only."""
import traceback
import json
from datetime import datetime, timezone

from client.state import ROOT


def trace(request_id, stage, **details):
    """Local conversation/tool data only; never pass transport objects or headers."""
    directory = ROOT / "results"
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / "client-tools.jsonl").open("a", encoding="utf-8") as output:
        output.write(json.dumps({"time": datetime.now(timezone.utc).isoformat(),
                                 "request": request_id, "stage": stage, **details}) + "\n")


def describe(error):
    if isinstance(error, BaseExceptionGroup):
        return "; ".join(describe(child) for child in error.exceptions)
    detail = str(error)[:400] if isinstance(error, (TypeError, ValueError, AttributeError, RuntimeError)) else ""
    return type(error).__name__ + (": " + detail if detail else "")


def record(error):
    directory = ROOT / "results"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "client-errors.log"
    with path.open("a", encoding="utf-8") as output:
        output.write(datetime.now(timezone.utc).isoformat() + " " + describe(error) + "\n")
        def frames(exc):
            for frame in traceback.extract_tb(exc.__traceback__):
                output.write(f"  {frame.filename}:{frame.lineno} in {frame.name}\n")
            if isinstance(exc, BaseExceptionGroup):
                for child in exc.exceptions:
                    frames(child)
        frames(error)
    return describe(error)


def exception_detail(exc):
    """Task-group wrappers must not hide a timeout or device failure."""
    if isinstance(exc, BaseExceptionGroup):
        return "; ".join(dict.fromkeys(exception_detail(child) for child in exc.exceptions))[:500]
    # Avoid exception strings which can embed requests, headers or credentials.
    known = {"ConnectTimeout": "connection timed out", "ReadTimeout": "response timed out",
             "ConnectError": "connection could not be established", "ReadError": "connection interrupted",
             "MCPError": "MCP server/session error", "PortAudioError": "audio device failure"}
    name = type(exc).__name__
    return name + (": " + known[name] if name in known else "")
