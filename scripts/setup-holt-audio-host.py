"""Configure this PC's dedicated Voicemeeter Banana bridge, not SKIPPY or OBS.

Run after installing/rebooting Banana. No Windows default devices are changed.
Use --select-holt-devices only after SKIPPY's microphone stream is ready.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import ipaddress
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from client.state import ROOT, load_settings, save_settings


class Remote:
    def __init__(self):
        self.dll = ctypes.WinDLL(r"C:\Program Files (x86)\VB\Voicemeeter\VoicemeeterRemote64.dll")
        self.dll.VBVMR_SetParameterFloat.argtypes = [ctypes.c_char_p, ctypes.c_float]
        self.dll.VBVMR_GetParameterFloat.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_float)]
        self.dll.VBVMR_SetParameterStringW.argtypes = [ctypes.c_char_p, ctypes.c_wchar_p]
        self.dll.VBVMR_GetParameterStringW.argtypes = [ctypes.c_char_p, ctypes.c_wchar_p]
        self.dll.VBVMR_GetVoicemeeterType.argtypes = [ctypes.POINTER(ctypes.c_long)]
        result = self.dll.VBVMR_Login()
        if result < 0:
            raise RuntimeError(f"Voicemeeter login failed ({result})")
        self.logged_in = True
        if result == 1:
            if self.dll.VBVMR_RunVoicemeeter(2) != 0:
                self.close()
                raise RuntimeError("Could not start Voicemeeter Banana")
        for _ in range(300):
            self.dll.VBVMR_IsParametersDirty()
            kind = ctypes.c_long()
            if self.dll.VBVMR_GetVoicemeeterType(ctypes.byref(kind)) == 0 and kind.value == 2:
                break
            time.sleep(.1)
        else:
            self.close()
            raise RuntimeError("Banana is not ready; close Standard/Potato and start Banana")

    def close(self):
        if getattr(self, "logged_in", False):
            # SetParameter queues changes for the mixer; allow its control loop to
            # consume them before releasing this API client.
            time.sleep(.3)
            self.dll.VBVMR_Logout()
            self.logged_in = False

    def set(self, key, value):
        function = self.dll.VBVMR_SetParameterStringW if isinstance(value, str) else self.dll.VBVMR_SetParameterFloat
        argument = value if isinstance(value, str) else ctypes.c_float(value)
        result = function(key.encode("ascii"), argument)
        if result != 0:
            raise RuntimeError(f"Voicemeeter rejected {key} ({result})")

    def get(self, key, string=False):
        self.dll.VBVMR_IsParametersDirty()
        value = ctypes.create_unicode_buffer(512) if string else ctypes.c_float()
        result = (self.dll.VBVMR_GetParameterStringW(key.encode("ascii"), value) if string
                  else self.dll.VBVMR_GetParameterFloat(key.encode("ascii"), ctypes.byref(value)))
        if result != 0:
            raise RuntimeError(f"Could not read {key} ({result})")
        return value.value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peer", default="192.168.50.41", help="SKIPPY's Ethernet IPv4 address")
    parser.add_argument("--clock-device", default="Speakers (Senary Audio)", help="Stable local hardware output for A1's audio clock; receives no HOLT mix")
    parser.add_argument("--apply", action="store_true", help="Replace mixer routing with the dedicated HOLT bridge; back up the current profile first")
    parser.add_argument("--select-holt-devices", action="store_true", help="Switch saved HOLT devices to the bridge for next Start; current worker is untouched")
    args = parser.parse_args()
    peer = str(ipaddress.IPv4Address(args.peer))
    directory = ROOT / "audio-bridge"
    directory.mkdir(parents=True, exist_ok=True)
    remote = Remote()
    try:
        if args.apply:
            backup = directory / ("before-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + ".xml")
            remote.set("Command.Save", str(backup.resolve()))
            for _ in range(40):
                if backup.exists() and backup.stat().st_size > 0:
                    break
                time.sleep(.1)
            else:
                raise RuntimeError("Mixer backup was not saved; routing was not changed")
            remote.set("vban.Enable", 0)
            # Separate buses: remote mic -> B1; HOLT voice -> A2. No feedback path.
            for strip in range(5):
                for bus in ("A1", "A2", "A3", "B1", "B2"):
                    remote.set(f"Strip[{strip}].{bus}", 0)
                remote.set(f"Strip[{strip}].Mute", 0)
                remote.set(f"Strip[{strip}].Gain", 0)
            for strip in range(3):
                remote.set(f"Strip[{strip}].device.mme", "")
            for bus in range(5):
                remote.set(f"Bus[{bus}].Mute", 0)
                remote.set(f"Bus[{bus}].Gain", 0)
                remote.set(f"Bus[{bus}].mode.normal", 1)
            remote.set("Strip[0].Label", "SKIPPY microphone")
            remote.set("Strip[0].B1", 1)
            remote.set("Strip[3].Label", "HOLT voice to SKIPPY")
            remote.set("Strip[3].A2", 1)
            # Shared MME clock output avoids taking exclusive ownership of local speakers.
            remote.set("Bus[0].device.mme", args.clock_device)
            remote.set("Bus[1].device.mme", "")
            remote.set("Bus[2].device.mme", "")
            remote.set("Option.sr", 48000)
            for index in range(8):
                remote.set(f"vban.instream[{index}].on", 0)
                remote.set(f"vban.outstream[{index}].on", 0)
            for key, value in {
                "vban.instream[0].name": "SKIPPY_MIC", "vban.instream[0].ip": peer,
                "vban.instream[0].port": 6980, "vban.instream[0].route": 0,
                "vban.instream[0].quality": 2,
                "vban.outstream[0].name": "HOLT_VOICE", "vban.outstream[0].ip": peer,
                "vban.outstream[0].port": 6980, "vban.outstream[0].route": 1,
                "vban.outstream[0].sr": 48000, "vban.outstream[0].channel": 2,
                "vban.outstream[0].bit": 1, "vban.outstream[0].quality": 2,
                "vban.instream[0].on": 1, "vban.outstream[0].on": 1,
                "vban.Enable": 1,
            }.items():
                remote.set(key, value)
            time.sleep(2)
            profile = directory / "holt-host.xml"
            remote.set("Command.Save", str(profile.resolve()))
            time.sleep(.5)
            print("Mixer profile saved:", profile)
        checks = {key: remote.get(key) for key in (
            "Strip[0].B1", "Strip[0].A1", "Strip[0].A2", "Strip[3].A2", "Strip[3].B1",
            "vban.Enable", "vban.instream[0].on", "vban.outstream[0].on",
            "vban.instream[0].route", "vban.outstream[0].route",
            "vban.instream[0].port", "vban.outstream[0].port")}
        checks.update({key: remote.get(key, string=True) for key in (
            "vban.instream[0].name", "vban.instream[0].ip",
            "vban.outstream[0].name", "vban.outstream[0].ip", "Bus[0].device.name")})
        if args.apply:
            expected = {"Strip[0].B1": 1, "Strip[0].A1": 0, "Strip[0].A2": 0,
                "Strip[3].A2": 1, "Strip[3].B1": 0, "vban.Enable": 1,
                "vban.instream[0].route": 0, "vban.outstream[0].route": 1,
                "vban.instream[0].ip": peer, "vban.outstream[0].ip": peer,
                "vban.instream[0].port": 6980, "vban.outstream[0].port": 6980}
            if any(checks[key] != value for key, value in expected.items()):
                raise RuntimeError("Routing readback did not match; do not switch HOLT devices")
            if not checks["Bus[0].device.name"]:
                raise RuntimeError("A1's hardware clock device is not open; do not switch HOLT devices")
        if args.select_holt_devices:
            from client.audio import devices
            microphones = [name for name, _ in devices("input") if name.startswith(("Voicemeeter Out B1 (", "Voicemeeter Output (")) and name.endswith("[Windows WASAPI]")]
            speakers = [name for name, _ in devices("output") if name.startswith("Voicemeeter Input (") and name.endswith("[Windows WASAPI]")]
            if len(microphones) != 1 or len(speakers) != 1:
                raise RuntimeError("Could not uniquely identify the WASAPI bridge devices; reboot/inspect before changing HOLT")
            settings = load_settings()
            backup = directory / "before-device-selection.json"
            if not backup.exists():
                backup.write_text(json.dumps(settings, indent=2), encoding="utf-8")
            settings.update(microphone=microphones[0], speaker=speakers[0])
            save_settings(settings)
            print("HOLT devices saved for next Start:", microphones[0], "->", speakers[0])
        (directory / "host-status.json").write_text(json.dumps(checks, indent=2), encoding="utf-8")
        print(json.dumps(checks, indent=2))
    finally:
        remote.close()


if __name__ == "__main__":
    main()
