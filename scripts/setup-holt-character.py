"""Update the standalone HOLT card and Captain persona through local SillyTavern APIs.

No inference, game commands, API credentials, or HOLT service changes.
"""

import argparse
import copy
import json
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

import httpx


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "integrations/sillytavern"
PERSONA = "holt-captain.png"


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true", help="Apply the assets to local SillyTavern")
    parser.add_argument("--editor-closed", action="store_true",
                        help="Confirm ST browser tabs are closed so stale autosaves cannot overwrite API writes")
    args = parser.parse_args()
    card = read_json(ASSETS / "HOLT.standalone.character.json")
    persona = read_json(ASSETS / "HOLT-Captain.persona.json")
    canon = read_json(ASSETS / "HOLT.lorebook.json")
    assert card["data"]["name"] == "HOLT" and card["data"]["extensions"]["world"] == "HOLT canon"
    assert len(canon["entries"]) == 8
    assert persona["personas"][PERSONA] == "Commander Brown"
    if not args.install:
        print("Assets valid. Use --install to update the existing HOLT character and add its Captain persona.")
        return
    if not args.editor_closed:
        parser.error("Close SillyTavern browser tabs, then use --install --editor-closed. "
                     "An open editor can overwrite API updates with stale character/persona data.")

    with httpx.Client(base_url="http://127.0.0.1:8000", trust_env=False, timeout=30) as client:
        csrf = client.get("/csrf-token")
        csrf.raise_for_status()
        client.headers.update({"X-CSRF-Token": csrf.json()["token"], "Origin": "http://127.0.0.1:8000"})

        def post(path, body):
            response = client.post(path, json=body)
            response.raise_for_status()
            if not response.content:
                return None
            return response.json() if "json" in response.headers.get("content-type", "") else response.text

        def settings():
            return json.loads(post("/api/settings/get", {})["settings"])

        characters = post("/api/characters/all", {})
        matches = [x for x in characters if x.get("name", "").casefold() == "holt"]
        if len(matches) != 1:
            raise RuntimeError("Expected exactly one existing HOLT character; no character was changed.")
        avatar = matches[0]["avatar"]
        before_character = post("/api/characters/get", {"avatar_url": avatar})
        initial = settings()
        worlds = post("/api/worldinfo/list", {})
        names = {x["file_id"] for x in worlds}
        if "Elite Dangerous" not in names:
            raise RuntimeError("Install the Elite Dangerous universe book first.")
        old_canon = post("/api/worldinfo/get", {"name": "HOLT canon"}) if "HOLT canon" in names else None
        if old_canon and old_canon.get("entries") and old_canon != canon:
            raise RuntimeError("Existing personal lore differs from the seed. Reconcile it before replacing it.")

        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        backup = ROOT / ".local/holt/lore-research/st-backups" / ("HOLT-before-" + stamp + ".json")
        backup.parent.mkdir(parents=True, exist_ok=True)
        # Save only character/persona/lore fields. Provider settings and secrets are excluded.
        before = {"character": before_character, "personal_lore": old_canon,
                  "user_avatar": initial.get("user_avatar"), "username": initial.get("username"),
                  "persona_fields": {k: v for k, v in initial.get("power_user", {}).items()
                                     if "persona" in k},
                  "world_info_settings": initial.get("world_info_settings", {})}
        backup.write_text(json.dumps(before, ensure_ascii=False, indent=2), encoding="utf-8")

        if old_canon != canon:
            post("/api/worldinfo/edit", {"name": "HOLT canon", "data": canon})
            assert post("/api/worldinfo/get", {"name": "HOLT canon"}) == canon

        avatars = post("/api/avatars/get", {})
        if PERSONA not in avatars:
            source_avatar = initial.get("user_avatar", "user-default.png")
            image = client.get("/User%20Avatars/" + quote(source_avatar, safe=""))
            if image.status_code == 404:
                image = client.get("/img/user-default.png")
            image.raise_for_status()
            uploaded = client.post("/api/avatars/upload", data={"overwrite_name": PERSONA},
                                   files={"avatar": ("captain.png", image.content, "image/png")})
            uploaded.raise_for_status()
            assert uploaded.json()["path"] == PERSONA

        # Write both v1 mirrors and v2 data; preserve the existing image, chat and extension fields.
        update = copy.deepcopy(card)
        update["avatar"] = avatar
        for field in ("name", "description", "personality", "scenario", "first_mes", "mes_example"):
            update[field] = card["data"][field]
        post("/api/characters/merge-attributes", update)

        fresh = settings()
        expected = copy.deepcopy(fresh)
        power = expected.setdefault("power_user", {})
        descriptor = copy.deepcopy(persona["persona_descriptions"][PERSONA])
        descriptor["connections"] = [{"type": "character", "id": avatar}]
        power.setdefault("personas", {})[PERSONA] = persona["personas"][PERSONA]
        descriptors = power.setdefault("persona_descriptions", {})
        # One HOLT-specific Captain should win selection, instead of an older blank
        # Commander Brown connection taking precedence over the populated persona.
        for key, other in descriptors.items():
            if key != PERSONA and "connections" in other:
                other["connections"] = [connection for connection in other["connections"]
                                        if not (connection.get("type") == "character"
                                                and connection.get("id") == avatar)]
        descriptors[PERSONA] = descriptor
        # Select the Captain now and keep ST's active persona cache consistent.
        # Its general default persona is deliberately preserved for unrelated chats.
        expected["user_avatar"] = PERSONA
        expected["username"] = persona["personas"][PERSONA]
        for field in ("description", "position", "depth", "role", "lorebook"):
            power["persona_description" + ("_" + field if field != "description" else "")] = descriptor[field]
        world = expected.setdefault("world_info_settings", {}).setdefault("world_info", {})
        char_lore = world.setdefault("charLore", [])
        character_key = Path(avatar).stem
        row = next((x for x in char_lore if x.get("name") == character_key), None)
        if row is None:
            row = {"name": character_key, "extraBooks": []}
            char_lore.append(row)
        if "Elite Dangerous" not in row["extraBooks"]:
            row["extraBooks"].append("Elite Dangerous")
        response = post("/api/settings/save", expected)
        if response.get("result") != "ok":
            raise RuntimeError("SillyTavern did not confirm the persona/settings write")

        actual_settings = settings()
        # Compare the entire read-back in memory, without logging unrelated configuration.
        assert actual_settings == expected, "Settings read-back differs; check for concurrent UI edits"
        actual = post("/api/characters/get", {"avatar_url": avatar})
        for field, value in card["data"].items():
            if field != "extensions":
                assert actual["data"][field] == value, "Character field mismatch: " + field
        for field in ("name", "description", "personality", "scenario", "first_mes", "mes_example"):
            assert actual[field] == card["data"][field], "Character editor mirror mismatch: " + field
        for field, value in card["data"]["extensions"].items():
            assert actual["data"]["extensions"][field] == value, "Character extension mismatch: " + field
        after_characters = post("/api/characters/all", {})
        assert {x["avatar"]: x for x in characters if x["avatar"] != avatar} == {
            x["avatar"]: x for x in after_characters if x["avatar"] != avatar}, "Other character changed"
        assert PERSONA in post("/api/avatars/get", {})
        print(json.dumps({"character": actual["name"], "version": actual["data"]["character_version"],
                          "primary_lore": "HOLT canon", "additional_lore": row["extraBooks"],
                          "persona": persona["personas"][PERSONA], "persona_bound_to": avatar,
                          "persona_selected": actual_settings["user_avatar"] == PERSONA,
                          "character_and_settings_verified": True, "backup": str(backup),
                          "next_step": "Reopen SillyTavern and select HOLT. The editor should show the populated card."}, indent=2))


if __name__ == "__main__":
    main()
