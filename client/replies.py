"""Explicit silent-turn contract; blank or malformed model responses aren't silence."""
import json
import re


def non_speech(text):
    return bool(re.fullmatch(r"\s*[\[(](?:cough(?:ing)?|breathing|laugh(?:ing|ter)?|music|silence|blank_audio|inaudible|applause)[\])]\s*[.!]?\s*", text, re.I))


def silent_reply(content):
    try:
        value = json.loads(content)
    except (ValueError, TypeError):
        return None
    if isinstance(value, dict) and value.get("respond") is False and value.get("text") == "":
        return {"text": "", "delivery": "normal", "respond": False}
    return None


def spoken_reply(content):
    try:
        value = json.loads(content)
    except (ValueError, TypeError):
        return None
    if (isinstance(value, dict) and isinstance(value.get("text"), str)
            and 0 < len(value["text"].strip()) <= 4000
            and value.get("delivery") in ("normal", "caution", "urgent", "critical")):
        # Optional control fields must never cause the JSON itself to be spoken.
        return {"text": value["text"].strip(), "delivery": value["delivery"]}
    return None
