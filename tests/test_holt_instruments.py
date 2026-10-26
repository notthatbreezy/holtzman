import sqlite3
import unittest
from datetime import datetime, timedelta, timezone

from client.instruments import InstrumentHistory, snapshot


class InstrumentContextTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 26, 4, 0, tzinfo=timezone.utc)
        self.db = sqlite3.connect(":memory:")
        self.history = InstrumentHistory(self.db)

    def tearDown(self):
        self.db.close()

    def facts(self, at=None, fuel=39.4, flags=None, system="Sol"):
        return {"location": {"system": system, "near": "Test Port"},
                "ship": {"ident": "TEST", "type": "panthermkii"},
                "status": {"timestamp": (at or self.now).isoformat(),
                           "fuel": {"FuelMain": fuel}, "flags": flags or []}}

    def test_units_and_independent_flags_are_explicit(self):
        context = snapshot(self.facts(flags=["FsdMassLocked"]), self.now)
        self.assertEqual(context["fuel_main_tonnes"], 39.4)
        self.assertIsNone(context["fuel_percentage"])
        self.assertTrue(context["mass_locked"])
        self.assertFalse(context["docked"])

    def test_observed_duration_resets_after_a_gap(self):
        for seconds in (0, 10, 20, 60, 70):
            at = self.now + timedelta(seconds=seconds)
            self.history.observe(self.facts(at), at)
        context = self.history.context(self.facts(), self.now + timedelta(seconds=70))
        self.assertEqual(context["history"]["system_observed_seconds"], 10)

    def test_net_fuel_rate_requires_continuous_stable_window(self):
        for seconds in range(0, 121, 10):
            at = self.now + timedelta(seconds=seconds)
            self.history.observe(self.facts(at, fuel=40 - seconds / 120), at)
        end = self.now + timedelta(seconds=120)
        context = self.history.context(self.facts(end, fuel=39), end)["history"]
        self.assertEqual(context["fuel_main_net_decrease_tonnes_per_minute"], .5)
        self.history.events([{"event": "FSDJump", "timestamp": (self.now + timedelta(seconds=60)).isoformat(),
                              "StarSystem": "Sol", "FuelUsed": 1.2}], end)
        context = self.history.context(self.facts(end, fuel=39), end)["history"]
        self.assertNotIn("fuel_main_net_decrease_tonnes_per_minute", context)
        self.assertEqual(context["recent_jumps"][0]["FuelUsed"], 1.2)
        self.assertEqual(context["current_system_seconds_since_journal_arrival"], 60)

    def test_refuelling_and_stale_readings_do_not_get_burn_rates(self):
        for seconds in range(0, 121, 10):
            at = self.now + timedelta(seconds=seconds)
            self.history.observe(self.facts(at, fuel=40 + seconds / 120), at)
        end = self.now + timedelta(seconds=120)
        context = self.history.context(self.facts(end, fuel=41), end)["history"]
        self.assertNotIn("fuel_main_net_decrease_tonnes_per_minute", context)

    def test_journal_anchors_are_not_reused_after_undocking_or_revisiting(self):
        self.history.events([{"event": "Docked", "timestamp": (self.now - timedelta(minutes=5)).isoformat(), "StationName": "Test Port"},
                             {"event": "Undocked", "timestamp": (self.now - timedelta(minutes=1)).isoformat()}], self.now)
        context = self.history.context(self.facts(flags=["Docked"]), self.now)["history"]
        self.assertNotIn("current_state_journal_utc", context)
        # A new tracker over the same durable database retains journal history.
        self.assertEqual(InstrumentHistory(self.db).db.execute("SELECT count(*) FROM instrument_events").fetchone()[0], 2)


if __name__ == "__main__":
    unittest.main()
