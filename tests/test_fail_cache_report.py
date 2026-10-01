import unittest

from scripts.check_fail_cache_report import TARGET_SELECTORS, validate_report


def make_profile_report() -> dict:
    instrumentation = {
        "caches": [
            {"element_index": 0, "insertions": 9, "final_size": 9},
            {"element_index": 1, "insertions": 8, "final_size": 8},
            {"element_index": 2, "insertions": 0, "final_size": 8},
        ],
        "prefixes": [
            {"prefix_index": 0, "prefix_occurrences": 2, "hashings": 2, "internments": 1, "insertions": 2},
            {"prefix_index": 1, "prefix_occurrences": 1, "hashings": 1, "internments": 0, "insertions": 1},
        ],
    }
    before = {"mean_cycles": 1000, "counts": {"slow_accepts": 5, "slow_rejects": 10, "fail_cache_rejects": 0}}
    after = {"mean_cycles": 950, "counts": {
        "slow_accepts": 5, "slow_rejects": 8, "fail_cache_rejects": 2,
        "filled_fail_caches": 1, "fail_cache_instrumentation": instrumentation,
    }}
    selectors = lambda cycles, count: {
        "means_cycles": {selector: cycles for selector in TARGET_SELECTORS},
        "slow_reject_counts": {selector: count for selector in TARGET_SELECTORS},
    }
    return {
        "metadata": {"variants": [{"id": 0, "label": "baseline"}, {"id": 1, "label": "fail-caches"}]},
        "websites": [{"website": "fixture.test", "variants": [
            {"variant_id": 0, "summary": before, "selector_slow_rejects_summary": selectors(100, 4)},
            {"variant_id": 1, "summary": after, "selector_slow_rejects_summary": selectors(50, 2)},
        ]}],
    }

class FailCacheReportTests(unittest.TestCase):
    def test_profile_report_passes_every_invariant(self) -> None:
        failures, skipped = validate_report(
            make_profile_report(), [("baseline", "fail-caches")], require_all_targets=True
        )
        self.assertEqual((failures, skipped), ({}, {("baseline", "fail-caches"): []}))
