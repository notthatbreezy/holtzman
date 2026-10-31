import unittest
from unittest.mock import Mock
import numpy as np
from client.audio import Segmenter

VOICE=(np.ones(480,dtype=np.int16)*1000).tobytes()
QUIET=bytes(960)
def segmenter(mode="hold",**kwargs):
    s=Segmenter(mode=mode,**kwargs);s.vad=Mock();s.vad.is_speech.side_effect=lambda frame,_:frame==VOICE;return s

class SegmenterTests(unittest.TestCase):
    def test_hold_discards_button_up_speech(self):
        s=segmenter()
        self.assertTrue(all(s.feed(VOICE,False) is None for _ in range(80)))

    def test_short_held_command_has_preroll_release_tail_and_provenance(self):
        s=segmenter()
        for _ in range(10):s.feed(QUIET,False)
        for _ in range(3):self.assertIsNone(s.feed(VOICE,True))
        for _ in range(4):self.assertIsNone(s.feed(QUIET,False))
        pcm=s.feed(QUIET,False)
        self.assertIsNotNone(pcm);self.assertTrue(s.last_held)
        self.assertEqual(len(pcm)//960,18)

    def test_hybrid_converts_a_voice_segment_to_held(self):
        s=segmenter("hybrid")
        for _ in range(12):s.feed(VOICE,False)
        s.feed(VOICE,True)
        for _ in range(40):self.assertIsNone(s.feed(QUIET,True))
        output=None
        for _ in range(5):output=s.feed(QUIET,False)
        self.assertIsNotNone(output);self.assertTrue(s.last_held)

    def test_off_ignores_ptt_and_uses_fixed_silence(self):
        s=segmenter("off")
        for _ in range(14):s.feed(VOICE,True)
        output=None
        for _ in range(29):output=s.feed(QUIET,True)
        self.assertIsNotNone(output);self.assertFalse(s.last_held)

    def test_maximum_hold_does_not_rearm_without_release(self):
        s=segmenter(max_hold_s=1)
        completed=[s.feed(VOICE,True) for _ in range(80)]
        self.assertEqual(sum(a is not None for a in completed),1)
        s.feed(QUIET,False)
        for _ in range(3):s.feed(VOICE,True)
        self.assertIsNotNone([s.feed(QUIET,False) for _ in range(5)][-1])
