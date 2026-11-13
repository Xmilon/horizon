"""The prepared-data coverage gate: what a missing shard result means on each kind of run."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from tests.test_ci_concurrency import ROOT, yaml

STALE = ("snapshot rejected: run age outside 0..3600 seconds; enabled=true; lease=1; now=2; reference=2")


def gate(result, event, reason):
    job = yaml.safe_load((ROOT / ".github/workflows/dyna-ci.yml").read_text())["jobs"]["ignored-tests"]
    script = next(step["run"] for step in job["steps"] if step.get("name") == "Require prepared-data coverage")
    with tempfile.TemporaryDirectory() as folder:
        summary = Path(folder) / "summary"
        env = {**os.environ, "RESULT": result, "ROUTE": "false", "REASON": reason, "CHANGES": "success",
               "FORCED_GITHUB": "false", "GITHUB_EVENT_NAME": event, "GITHUB_STEP_SUMMARY": str(summary)}
        code = subprocess.run(["bash", "-e", "-c", script], env=env, capture_output=True).returncode
        return code, summary.read_text()


class CoverageGateTests(unittest.TestCase):
    def test_a_main_push_that_outwaited_its_routing_snapshot_is_neutral(self):
        code, summary = gate("skipped", "push", STALE)
        self.assertEqual(code, 0)
        self.assertIn("newer main push carries this coverage", summary)

    def test_really_missing_coverage_still_fails(self):
        for result, event, reason in [
            ("skipped", "push", "live lease checked against current time; enabled=false"),
            ("skipped", "push", "snapshot lease checked at run creation; age=12 seconds (limit 3600)"),
            ("skipped", "workflow_dispatch", STALE),
            ("failure", "push", STALE),
            ("cancelled", "workflow_dispatch", "live lease checked against current time"),
        ]:
            with self.subTest(result=result, event=event, reason=reason[:30]):
                self.assertEqual(gate(result, event, reason)[0], 1)

    def test_superseded_main_shards_are_neutral(self):
        self.assertEqual(gate("cancelled", "push", "live lease checked against current time")[0], 0)

    def test_tested_runs_pass(self):
        self.assertEqual(gate("success", "push", "live lease checked against current time")[0], 0)


if __name__ == "__main__":
    unittest.main()
