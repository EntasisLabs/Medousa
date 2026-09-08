import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "coder_usage", Path(__file__).resolve().parents[1] / "analyze-coder-usage.py"
)
usage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(usage)


def row(tokens, outcome="completed"):
    return {
        "stream_turn_id": 7,
        "tools_invoked": ["cognition_store_write"],
        "inference": {
            "schema_version": 1, "outcome": outcome, "request": {},
            "provider_model": "test-model", "elapsed_ms": 20,
            "tokens": tokens, "generated": {"tool_arguments_chars": 12},
        },
    }


class UsageReportTests(unittest.TestCase):
    def test_missing_usage_does_not_become_zero_or_a_complete_total(self):
        report = usage.summarize([
            {"kind": "finalized"},
            row({"input": 1000, "cache_read": 800, "output": 200, "reasoning": 150}),
            row({}, "interrupted"),
        ])["7"]
        self.assertEqual(report["tokens"]["input"], {
            "reported_sum": 1000, "reported_requests": 1,
            "missing_requests": 1, "total": None,
        })
        self.assertIsNone(report["cache_read_fraction"])
        self.assertEqual(report["tokens"]["output"]["reported_sum"], 200)
        self.assertEqual(report["outcomes"]["interrupted"], 1)

    def test_known_zero_and_weighted_ratio(self):
        report = usage.summarize([
            row({"input": 1000, "cache_read": 800, "cache_write": 0}),
            row({"input": 100, "cache_read": 0, "cache_write": 0}),
        ])["7"]
        self.assertEqual(report["cache_read_fraction"], 800 / 1100)
        self.assertEqual(report["tokens"]["cache_write"]["total"], 0)

    def test_invalid_count_or_schema_fails(self):
        for value in [-1, True, "100"]:
            with self.assertRaises(ValueError):
                usage.summarize([row({"input": value})])
        record = row({})
        record["inference"]["schema_version"] = 3
        with self.assertRaises(ValueError):
            usage.summarize([record])

    def test_mixed_versions_report_partial_generated_coverage(self):
        old = row({})
        old["inference"]["generated"]["tool_calls"] = 2
        new = row({})
        new["inference"].update(schema_version=2, generated={
            "tool_calls": 1, "intent_chars": 14, "requested_batch_operations": 4,
        })
        cancelled = row({}, "interrupted")
        cancelled["inference"].update(schema_version=2, generated={
            "tool_calls": 0, "intent_chars": None, "requested_batch_operations": None,
        })
        report = usage.summarize([old, new, cancelled])["7"]
        self.assertEqual(report["generated"]["intent_chars"], 14)
        self.assertEqual(report["generated_coverage"]["intent_chars"], 1)
        self.assertEqual(report["generated"]["requested_batch_operations"], 4)
        self.assertEqual(report["tool_calls_per_completed_request"], 1.5)
        new["inference"]["generated"]["intent_chars"] = -1
        with self.assertRaises(ValueError):
            usage.summarize([new])

    def test_truncated_ledger_is_not_silently_undercounted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ledger.jsonl"
            path.write_text('{"kind":"finalized"}\n{"inference":', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "line 2"):
                list(usage.read_records(path))


if __name__ == "__main__":
    unittest.main()
