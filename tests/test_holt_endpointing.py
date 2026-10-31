import concurrent.futures
import unittest
from client.endpointing import TurnWorker

class EndpointWorkerTests(unittest.TestCase):
    def test_delayed_observation_does_not_count_as_slow_inference(self):
        now=[0.0];worker=TurnWorker(lambda _pcm:.9,clock=lambda:now[0])
        try:
            self.assertFalse(worker.check(b"",.5))
            worker.pending[0].result(timeout=1)
            now[0]=.25
            self.assertTrue(worker.check(b"",.5));self.assertEqual(worker.slow,0)
        finally:worker.close()

    def test_failure_backoff_recovers_and_clears_fixed_fallback(self):
        now=[0.0];calls=[0]
        def predict(_pcm):
            calls[0]+=1
            if calls[0]==1:raise ValueError("model absent")
            return .9
        worker=TurnWorker(predict,clock=lambda:now[0])
        try:
            worker.check(b"",.5)
            try:worker.pending[0].result(timeout=1)
            except ValueError:pass
            self.assertIsNone(worker.check(b"",.5));self.assertEqual(worker.reason,"model unavailable")
            now[0]=301;worker.check(b"",.5);worker.pending[0].result(timeout=1)
            self.assertTrue(worker.check(b"",.5));self.assertIsNone(worker.reason)
        finally:worker.close()

    def test_cancelled_or_stale_checks_do_not_poison_the_model(self):
        worker=TurnWorker(lambda _pcm:.9)
        try:
            future=concurrent.futures.Future();future.cancel()
            worker.pending=(future,worker.clock(),worker.generation)
            worker.cancel();self.assertFalse(worker.check(b"",.5));self.assertIsNone(worker.reason)
        finally:worker.close()
