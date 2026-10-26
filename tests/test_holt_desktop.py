import tempfile
import unittest
from pathlib import Path

from client.state import Memory, load_settings, save_settings
from client.policy import Permit
from client.services import matches
from client.telemetry import reading_age, Watches
from datetime import datetime, timezone, timedelta


class DurableStateTests(unittest.TestCase):
    def test_settings_round_trip_uses_names_not_device_ids(self):
        with tempfile.TemporaryDirectory() as root:
            settings = load_settings(root)
            settings["microphone"] = "Jabra [Windows WASAPI]"
            save_settings(settings, root)
            self.assertEqual(load_settings(root)["microphone"], "Jabra [Windows WASAPI]")

    def test_resume_and_compaction_preserve_original_transcript(self):
        with tempfile.TemporaryDirectory() as root:
            memory = Memory(root)
            session = memory.session
            for n in range(6):
                memory.append(f"question {n}", {"text": f"answer {n}", "delivery": "normal"})
            _, rows = memory.context()
            memory.compact("Captain prefers short trade routes.", rows[1][0])
            memory.close()
            resumed = Memory(root)
            self.assertEqual(resumed.session, session)
            summary, recent = resumed.context()
            self.assertIn("short trade routes", summary)
            self.assertEqual(len(recent), 4)
            self.assertEqual(len(resumed.recent()), 6)
            resumed.new_session()
            self.assertEqual(resumed.recent(), [])
            self.assertEqual(resumed.db.execute("SELECT count(*) FROM turns").fetchone()[0], 6)
            resumed.close()


class ActionBoundaryTests(unittest.TestCase):
    def test_disabled_commands_report_capability_instead_of_promising_action(self):
        permit = Permit("Holt request docking now", enabled=False)
        self.assertFalse(permit.authorized)
        self.assertIn("controls are off", permit.unavailable_reply())

    def test_deferred_compound_docking_is_not_silently_armed(self):
        text = "hey holt how is it going, I'm headed to Herrington City - when I am in range can you request docking (7500 km)?"
        permit = Permit(text, spoken=True, enabled=True)
        self.assertTrue(permit.requested)
        self.assertFalse(permit.authorized)
        self.assertIn("haven't scheduled", permit.unavailable_reply())
        self.assertIn("7,500 metres", permit.unavailable_reply())

    def test_greeting_does_not_block_an_immediate_docking_order(self):
        self.assertTrue(Permit("Hey Holt how is it going, request docking now", spoken=True, enabled=True).authorized)
        self.assertFalse(Permit("Holt how do I request docking", spoken=True, enabled=True).authorized)

    def test_direct_spoken_order_needs_holt_address(self):
        with self.assertRaises(ValueError):
            Permit("Plot a course to Sol", spoken=True, enabled=True).check("plot_route", {"system": "Sol"})
        Permit("HOLT, plot a course to Sol", spoken=True, enabled=True).check("plot_route", {"system": "Sol"})

    def test_casual_negated_ambiguous_and_planning_requests_do_not_arm_controls(self):
        for text in ["Explain how to plot a route to Sol", "Do not plot a course to Sol",
                     "Holt plot a course to Ross, I don't remember the number",
                     "Plan a trade route to Sol", "What happens if we plot a route to Sol?"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                Permit(text, enabled=True).check("plot_route", {"system": "Sol"})

    def test_model_cannot_complete_or_shorten_a_destination(self):
        for requested, proposed in [("Ross", "Ross 248"), ("Ross 248", "Ross"), ("Solitude", "Sol")]:
            with self.subTest(requested=requested), self.assertRaises(ValueError):
                Permit(f"Plot a course to {requested}", enabled=True).check("plot_route", {"system": proposed})

    def test_successful_plot_cannot_be_replayed_with_same_permit(self):
        permit = Permit("Plot a course to Sol", enabled=True)
        permit.check("plot_route", {"system": "Sol"})
        with self.assertRaises(ValueError):
            permit.check("plot_route", {"system": "Sol"})

    def test_calibration_and_flight_keys_are_never_armed(self):
        permit = Permit("Holt request docking", spoken=True, enabled=True)
        permit.observed = True
        for name, arguments in [("record_click_target", {}), ("press_binding", {"binding": "PrimaryFire"})]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                permit.check(name, arguments)

    def test_pid_reuse_does_not_authorize_stopping_a_different_process(self):
        import psutil
        process = psutil.Process()
        self.assertFalse(matches({"pid": process.pid, "created": process.create_time() - 100,
                                  "exe": process.exe()}))


class TelemetryTests(unittest.TestCase):
    def test_age_is_utc_arithmetic_not_model_arithmetic(self):
        now = datetime(2026, 10, 26, 2, 30, tzinfo=timezone.utc)
        result = reading_age({"status": {"timestamp": "2026-10-26T01:07:00Z"}}, now)
        self.assertEqual(result["age_seconds"], 83 * 60)

    def test_old_status_change_is_not_a_heartbeat_failure(self):
        now = datetime.now(timezone.utc)
        facts = {"status": {"timestamp": (now - timedelta(hours=2)).isoformat(), "flags": []},
                 "ship": {"hull_health": .1}}
        self.assertIn("not a game heartbeat", reading_age(facts, now)["note"])
        self.assertEqual(Watches().update(facts, now), [])

    def test_warning_only_on_threshold_transition_and_never_when_docked(self):
        now = datetime.now(timezone.utc)
        facts = {"status": {"timestamp": now.isoformat(), "flags": []}, "ship": {"hull_health": .25}}
        watches = Watches()
        self.assertEqual(watches.update(facts, now)[0]["delivery"], "urgent")
        self.assertEqual(watches.update(facts, now), [])
        facts["ship"]["hull_health"] = .1
        self.assertEqual(watches.update(facts, now)[0]["delivery"], "critical")
        facts["status"]["flags"] = ["Docked"]
        self.assertEqual(watches.update(facts, now), [])


if __name__ == "__main__":
    unittest.main()
