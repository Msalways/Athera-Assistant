import unittest

from evals.verify_bridge_parity import normalized_events


class BridgeParityTests(unittest.TestCase):
    def test_normalization_keeps_worker_relationships_and_removes_run_ids(self) -> None:
        events = normalized_events(
            {
                "events": [
                    {
                        "event_id": "abc:1",
                        "run_id": "abc",
                        "task_id": "abc",
                        "worker_id": "worker-uuid",
                        "kind": "worker_started",
                    },
                    {
                        "event_id": "abc:2",
                        "run_id": "abc",
                        "task_id": "abc",
                        "worker_id": "worker-uuid",
                        "kind": "worker_terminal",
                    },
                ]
            }
        )
        self.assertEqual(events[0]["run_id"], "run")
        self.assertEqual(events[0]["worker_id"], "worker-1")
        self.assertEqual(events[1]["worker_id"], "worker-1")
        self.assertNotIn("event_id", events[0])


if __name__ == "__main__":
    unittest.main()
