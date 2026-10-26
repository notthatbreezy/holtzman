"""Explicit silent-turn contract; blank or malformed model responses aren't silence."""
import json
import re


def non_speech(text):
    return bool(re.fullmatch(r"\s*[\[(](?:music|silence|blank_audio|inaudible)[\])]\s*[.!]?\s*", text, re.I))


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


def speech_text(text):
    text = re.sub(r"cite.*?", "", text)
    text = re.sub(r"\[([^\]]+)\]\(https?://[^)]+\)", r"\1", text)
    text = re.sub(r"https?://\S+", "", text)
    return text.replace("**", "").replace("`", "").strip()
