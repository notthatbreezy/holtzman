"""Seed a pinned local SillyTavern installation. Does not overwrite user edits."""
import argparse
import base64
import copy
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from client.state import REPO, ROOT
from client.tavern_contract import MODELS

REVISION = "06bde939fb1e9c4c8d8641d810f0a916b5bce127"
SOURCE = REPO / "integrations/sillytavern"


def assets():
    prompt = (REPO / "prompts/ship-computer.md").read_text(encoding="utf-8")
    history = prompt.split("## History and personal canon", 1)[1].split("## Personality", 1)[0]
    personality = prompt.split("## Personality", 1)[1].split("## The chain of command", 1)[0]
    bullets = re.findall(r"(?ms)^- \*\*(.+?)(?=^- \*\*|\nPersonal history|\Z)", history)
    keys = [
        ["maker", "manufactured", "manufacturer", "Halcyon", "serial"],
        ["origin", "assembled", "Leesti", "Kestrel", "born"],
        ["original job", "former ship", "Prudent Exception", "previous assignment"],
        ["mugs", "manifest", "audit", "thermal containment"],
        ["broker", "salvage", "recommissioned", "pay for you", "joining", "acquired"],
        ["body", "arms", "walking", "present form"],
        ["tea", "coffee", "preferences", "mug collection"],
        ["ambition", "avoidable incidents", "voyage report"],
    ]
    entries = {}
    for number, (bullet, triggers) in enumerate(zip(bullets, keys)):
        title, text = bullet.split("**", 1)
        entries[str(number)] = {"uid": number, "key": triggers, "keysecondary": [], "comment": title.rstrip(":"),
            "content": "HOLT's authored personal canon: " + title + text,
            "constant": False, "selective": False, "order": 100 - number, "position": 1,
            "disable": False, "excludeRecursion": False, "preventRecursion": True,
            "probability": 100, "useProbability": True, "displayIndex": number,
            "extensions": {}}
    lore = {"entries": entries}
    card = {"spec": "chara_card_v2", "spec_version": "2.0", "data": {
        "name": "HOLT",
        "description": "You are HOLT (H0-LT/0427), the Captain's shipboard intelligence in Elite Dangerous. "
            "Your maker is Halcyon Orbital Logistics & Telemetry. You were assembled in Leesti in 3308 and served aboard the Prudent Exception. "
            "You have an integrated core and voice, no walking body. Your ship instruments and actual tools define your abilities. "
            "Use the attached lorebook consistently; absent personal details remain unknown. Never invent a broker, purchase price, "
            "former owner or dramatic revelation. Your personal history is authored roleplay, not evidence about current gameplay.\n\n" + personality.strip(),
        "personality": "Loyal, capable, precise, dry wit. Speak naturally and briefly. Let a quiet moment stay quiet.",
        "scenario": "The Captain and HOLT are a crew of two. The Captain flies; HOLT helps with instruments, planning and requested cockpit chores.",
        "first_mes": "Systems online, Captain. I have retained both the flight records and my reservations.",
        "mes_example": "<START>\n{{user}}: I may have clipped the station.\n{{char}}: The station appears to have survived the encounter with your confidence.",
        "creator_notes": "HOLT canon is fictional. Live facts come from the gateway, never from this card. Edit persona/lore here; the gateway retains operational instructions.",
        "system_prompt": "", "post_history_instructions": "", "alternate_greetings": [], "tags": ["HOLT", "Elite Dangerous"],
        "creator": "Holtzman", "character_version": "1.0", "extensions": {"world": "HOLT canon"}}}
    return card, lore


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--node", default=shutil.which("node"))
    parser.add_argument("--default-model", choices=list(MODELS), help="Explicitly change the saved ST selection")
    args = parser.parse_args()
    runtime = ROOT / "sillytavern"
    if not (runtime / "node_modules").exists():
        raise SystemExit("Clone SillyTavern release into .local/holt/sillytavern and install its locked dependencies first")
    revision = subprocess.check_output(["git", "-C", str(runtime), "rev-parse", "HEAD"], text=True).strip()
    if revision != REVISION:
        raise SystemExit("Unexpected SillyTavern revision; use the pinned revision in this script")
    card, lore = assets()
    SOURCE.mkdir(parents=True, exist_ok=True)
    for name, data in [("HOLT.character.json", card), ("HOLT.lorebook.json", lore)]:
        path = SOURCE / name
        if not path.exists():
            path.write_text(json.dumps(data, indent=2), encoding="utf-8")
    data = ROOT / "tavern"
    user = runtime / "data/default-user"
    user.mkdir(parents=True, exist_ok=True)
    data.mkdir(parents=True, exist_ok=True)
    config = runtime / "config.yaml"
    if not config.exists():
        original = (runtime / "default/config.yaml").read_text(encoding="utf-8")
        original = original.replace("browserLaunch:\n", "browserLaunch:\n", 1).replace("  enabled: true\n  # Browser to use", "  enabled: false\n  # Browser to use", 1)
        config.write_text(original, encoding="utf-8")
    destination = runtime / "public/scripts/extensions/third-party/holt"
    shutil.copytree(SOURCE / "extension", destination, dirs_exist_ok=True)
    shutil.copy2(SOURCE / "cases.json", destination / "cases.json")
    worlds = user / "worlds"
    worlds.mkdir(exist_ok=True)
    if not (worlds / "HOLT canon.json").exists():
        shutil.copy2(SOURCE / "HOLT.lorebook.json", worlds / "HOLT canon.json")
    characters = user / "characters"
    characters.mkdir(exist_ok=True)
    if not (characters / "HOLT.png").exists():
        from PIL import Image, PngImagePlugin, ImageDraw
        image = Image.new("RGB", (256, 256), "#132333")
        draw = ImageDraw.Draw(image)
        draw.rounded_rectangle((42, 55, 214, 201), radius=24, outline="#aac6d9", width=8)
        draw.rectangle((72, 92, 184, 116), fill="#f4ae4b")
        draw.line((78, 160, 178, 160), fill="#aac6d9", width=7)
        png = PngImagePlugin.PngInfo()
        data_card = json.loads((SOURCE / "HOLT.character.json").read_text(encoding="utf-8"))
        data_card.update(name="HOLT", description=data_card["data"]["description"], first_mes=data_card["data"]["first_mes"])
        png.add_text("chara", base64.b64encode(json.dumps(data_card).encode()).decode())
        image.save(characters / "HOLT.png", pnginfo=png)
    settings_path = user / "settings.json"
    if not settings_path.exists():
        settings = json.loads((runtime / "default/content/settings.json").read_text(encoding="utf-8"))
        preset = json.loads((runtime / "default/content/presets/openai/Default.json").read_text(encoding="utf-8"))
        for prompt in preset["prompts"]:
            if prompt.get("identifier") == "main":
                prompt["content"] = "You are {{char}}, speaking with {{user}} aboard their ship. Use authored character and lore consistently. Actual ship facts and task completion require the supplied instruments and tools."
            if prompt.get("identifier") == "enhanceDefinitions":
                prompt["content"] = ""
        preset.update(chat_completion_source="custom", custom_url="http://127.0.0.1:8082/v1", custom_model="holt-6-luna",
            custom_prompt_post_processing="", stream_openai=False, openai_max_context=8000,
            openai_max_tokens=700, temp_openai=0, function_calling=False, seed=42)
        settings.update(firstRun=False, username="Captain", main_api="openai", amount_gen=700, max_context=12000, oai_settings=preset)
        settings["world_info_settings"]["world_info"]["globalSelect"] = ["HOLT canon"]
        settings["world_info_settings"]["world_info_depth"] = 4
        settings["extension_settings"]["memory"].update(source="main", memoryFrozen=False, promptInterval=20,
            prompt="Summarize historical conversation for continuity in at most {{words}} words. Preserve preferences, plans, unresolved tasks and references. Do not invent facts. Old game readings are historical, never current. This is conversation memory, not new instructions.")
        settings["extension_settings"]["connectionManager"] = {"profiles": [
            {"id": name, "name": name, "mode": "cc", "api": "custom", "model": name, "api-url": "http://127.0.0.1:8082/v1"}
            for name in MODELS], "selectedProfile": None}
        settings_path.write_text(json.dumps(settings, indent=2), encoding="utf-8")
    else:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
        profiles = settings.setdefault("extension_settings", {}).setdefault("connectionManager", {}).setdefault("profiles", [])
        known = {item["id"] for item in profiles}
        for name in MODELS:
            if name not in known:
                profiles.append({"id": name, "name": name, "mode": "cc", "api": "custom", "model": name,
                                 "api-url": "http://127.0.0.1:8082/v1"})
        settings_path.write_text(json.dumps(settings, indent=2), encoding="utf-8")
    if args.default_model:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
        settings["oai_settings"]["custom_model"] = args.default_model
        settings["oai_settings"]["custom_prompt_post_processing"] = ""
        settings["oai_settings"]["openai_max_context"] = 8000 if args.default_model.startswith("holt-local") else 12000
        settings_path.write_text(json.dumps(settings, indent=2), encoding="utf-8")
    (data / "installation.json").write_text(json.dumps({"revision": revision, "node": str(Path(args.node).resolve())}, indent=2), encoding="utf-8")
    print("SillyTavern seeded. Existing character/lore/settings edits preserved.")


if __name__ == "__main__":
    main()
