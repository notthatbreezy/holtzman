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


def pronounce(text):
    """Expand display shorthand only for TTS; preserve the on-screen transcript."""
    text = speech_text(text)
    text = re.sub(r"(?<![A-Za-z])cr\s*/\s*t\b", " credits per tonne", text, flags=re.I)
    text = re.sub(r"\b(\d[\d,.]*)\s*([kKmMbB])\s*(?:cr|credits)\b",
                  lambda m: m[1] + " " + {"k": "thousand", "m": "million", "b": "billion"}[m[2].lower()] + " credits", text)
    text = re.sub(r"(?<=\d)\s*ly\b", " light years", text, flags=re.I)
    text = re.sub(r"(?<=\d)\s*ls\b", " light seconds", text, flags=re.I)
    text = re.sub(r"(?<=\d)\s*cr\b", " credits", text, flags=re.I)
    text = re.sub(r"(?<=\d)\s*t\b", " tonnes", text)
    text = re.sub(r"(?<=\d)\s*h\b", " hours", text)
    text = re.sub(r"\b([1-8])([ABCDE])\b", r"class \1, rating \2", text)
    acronyms = {"FSD": "frame shift drive", "SCO": "supercruise overcharge", "AFMU": "auto field maintenance unit",
                "DSS": "detailed surface scanner", "SRV": "surface reconnaissance vehicle"}
    for short, spoken in acronyms.items():
        text = re.sub(r"\b" + short + r"\b", spoken, text)
    text = re.sub(r"\bAD(?=\s+Leonis\b)", "A D", text)
    text = re.sub(r"\b([A-Z]{2,4})(?=\s+\d)", lambda m: " ".join(m[1]), text)
    for roman, spoken in (("IV", "four"), ("III", "three"), ("II", "two"), ("I", "one")):
        text = re.sub(r"\bMk\.?\s+" + roman + r"\b", "Mark " + spoken, text, flags=re.I)
    return re.sub(r" {2,}", " ", text)
