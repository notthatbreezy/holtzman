"""Local configuration and durable conversation state; no credentials stored here."""
import json
import sqlite3
import uuid
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ROOT = REPO / ".local/holt"
DEFAULTS = {
    "microphone": None, "speaker": None, "listen": True, "speak": True,
    "allow_actions": True, "mcp_url": "http://skippy:7777/mcp",
    "model_url": "http://127.0.0.1:8080", "transcription_url": "http://127.0.0.1:8081",
    "context_size": 16384, "silence_ms": 850, "max_utterance_seconds": 25,
    "model_provider": "local", "openai_model": "gpt-5.4-mini", "openai_reasoning": "low",
    "openai_web_search": True,
    "allow_hosted_images": False,
    "voice": "bm_george", "preferences": "Address me as Captain. Use George's normal register, dry humor and dynamic urgency.",
    "voice_effect": "dry",
}


def load_settings(root=ROOT):
    path = Path(root) / "settings.json"
    data = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    return DEFAULTS | {k: v for k, v in data.items() if k in DEFAULTS}


def save_settings(settings, root=ROOT):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    path = root / "settings.json"
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps({k: settings[k] for k in DEFAULTS}, indent=2), encoding="utf-8")
    temporary.replace(path)


class Memory:
    def __init__(self, root=ROOT):
        root = Path(root)
        root.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(root / "memory.sqlite3", timeout=15)
        self.db.executescript("""
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, created TEXT, summary TEXT DEFAULT '', covered INTEGER DEFAULT 0);
            CREATE TABLE IF NOT EXISTS turns(id INTEGER PRIMARY KEY, session TEXT, created TEXT, question TEXT, answer TEXT);
            CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT);
        """)
        row = self.db.execute("SELECT value FROM metadata WHERE key='session'").fetchone()
        self.session = row[0] if row else self.new_session()

    def new_session(self):
        self.session = str(uuid.uuid4())
        self.db.execute("INSERT INTO sessions(id,created) VALUES(?,?)", (self.session, self.now()))
        self.db.execute("INSERT OR REPLACE INTO metadata VALUES('session',?)", (self.session,))
        self.db.commit()
        return self.session

    @staticmethod
    def now():
        return datetime.now(timezone.utc).isoformat()

    def append(self, question, reply):
        self.db.execute("INSERT INTO turns(session,created,question,answer) VALUES(?,?,?,?)",
                        (self.session, self.now(), question, json.dumps(reply)))
        self.db.commit()

    def context(self):
        summary, covered = self.db.execute("SELECT summary,covered FROM sessions WHERE id=?", (self.session,)).fetchone()
        rows = self.db.execute("SELECT id,created,question,answer FROM turns WHERE session=? AND id>? ORDER BY id",
                               (self.session, covered)).fetchall()
        return summary, rows

    def compact(self, summary, through):
        self.db.execute("UPDATE sessions SET summary=?,covered=? WHERE id=?", (summary, through, self.session))
        self.db.commit()

    def recent(self, limit=30):
        rows = self.db.execute("SELECT question,answer FROM turns WHERE session=? ORDER BY id DESC LIMIT ?",
                               (self.session, limit)).fetchall()
        return [(q, json.loads(a)) for q, a in reversed(rows)]

    def close(self):
        self.db.close()
