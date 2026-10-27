import unittest
from unittest.mock import patch

from client.audio import audio_session, refresh_devices


class AudioLifecycleTests(unittest.TestCase):
    def test_start_refreshes_cached_devices_in_order(self):
        with patch("client.audio.sd") as audio:
            refresh_devices()
            self.assertEqual([c[0] for c in audio.mock_calls], ["stop", "_terminate", "_initialize"])

    def test_refresh_cannot_close_active_capture_or_playback(self):
        with patch("client.audio.sd") as audio:
            with audio_session(), audio_session():
                with self.assertRaisesRegex(RuntimeError, "still closing"):
                    refresh_devices()
            audio._terminate.assert_not_called()
            refresh_devices()
            audio._initialize.assert_called_once()

    def test_failed_stream_releases_refresh_guard(self):
        with patch("client.audio.sd") as audio:
            with self.assertRaises(ValueError):
                with audio_session():
                    raise ValueError("device disappeared")
            refresh_devices()
            audio._initialize.assert_called_once()
