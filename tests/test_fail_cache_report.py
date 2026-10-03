import copy
import unittest

from scripts.check_fail_cache_report import TARGET_SELECTORS, validate_report


def make_profile_report() -> dict:
    instrumentation = {
        "caches": [
            {"element_index": 0, "insertions": 9, "final_size": 8},
            {"element_index": 1, "insertions": 8, "final_size": 8},
            {"element_index": 2, "insertions": 0, "final_size": 0},
        ],
        "prefixes": [
            {
                "prefix_index": index,
                "prefix_occurrences": 4,
                "slow_rejecting_prefix_occurrences": 2,
                "hashings": 1,
                "internments": 1,
                "insertions": 2 if index < 8 else 1,
            }
            for index in range(9)
        ],
    }

    def variant(label: str, variant_id: int, slow_rejects: int, fail_cache_rejects: int, mean_cycles: int) -> dict:
        optimized = label in {"03-lazy-fail-caches", "07-lazy-fail-caches+"}
        counts = {
            "slow_accepts": 5,
            "slow_rejects": slow_rejects,
            "fail_cache_rejects": fail_cache_rejects,
            "filled_fail_caches": 1 if optimized else None,
            "total_fail_caches": 3 if optimized else None,
        }
        if optimized:
            counts["fail_cache_instrumentation"] = copy.deepcopy(instrumentation)
        selectors = {
            "means_cycles": {selector: 50 if optimized else 100 for selector in TARGET_SELECTORS},
            "stddevs_cycles": {},
            "slow_reject_counts": {selector: 2 if optimized else 4 for selector in TARGET_SELECTORS},
        }
        return {
            "variant_id": variant_id,
            "summary": {"mean_cycles": mean_cycles, "counts": counts, "times": []},
            "selector_slow_rejects_summary": selectors,
            "samples": {"times": [], "selector_slow_rejects_cycles": None},
        }

    return {
        "metadata": {
            "variants": [
                {"id": 0, "label": "01-baseline", "optimizations": {}},
                {"id": 1, "label": "03-lazy-fail-caches", "optimizations": {}},
                {"id": 2, "label": "05-baseline+", "optimizations": {}},
                {"id": 3, "label": "07-lazy-fail-caches+", "optimizations": {}},
            ]
        },
        "websites": [
            {
                "website": "fixture.test",
                "variants": [
                    variant("01-baseline", 0, 20, 0, 1000),
                    variant("03-lazy-fail-caches", 1, 10, 10, 900),
                    variant("05-baseline+", 2, 20, 0, 1200),
                    variant("07-lazy-fail-caches+", 3, 10, 10, 1100),
                ],
            }
        ],
    }


class FailCacheReportTests(unittest.TestCase):
    def test_both_profile_comparisons_pass_semantic_invariants(self) -> None:
        comparisons = [
            ("05-baseline+", "07-lazy-fail-caches+"),
            ("01-baseline", "03-lazy-fail-caches"),
        ]
        failures, skipped, timing, observations, _ = validate_report(
            make_profile_report(), comparisons, require_all_targets=True
        )
        self.assertEqual(failures, {})
        self.assertEqual(skipped, {comparison: [] for comparison in comparisons})
        self.assertEqual(timing[comparisons[0]][0][1:], (1200, 1100))
        self.assertEqual(timing[comparisons[1]][0][1:], (1000, 900))
        self.assertEqual(observations, {})

    def test_invalidated_semantic_and_storage_expectations_fail(self) -> None:
        report = make_profile_report()
        optimized = report["websites"][0]["variants"][1]
        counts = optimized["summary"]["counts"]
        counts["slow_accepts"] += 1
        counts["slow_rejects"] += 11
        counts["fail_cache_instrumentation"]["caches"][0]["final_size"] = 7
        optimized["summary"]["mean_cycles"] = 1200
        del optimized["selector_slow_rejects_summary"]["slow_reject_counts"]

        failures, _, _, _, _ = validate_report(report, [("01-baseline", "03-lazy-fail-caches")])
        self.assertIn(1, failures)
        self.assertIn(2, failures)
        self.assertIn(3, failures)
        self.assertIn(6, failures)
        self.assertIn(9, failures)

        report = make_profile_report()
        report["websites"][0]["variants"][1]["summary"]["counts"]["fail_cache_rejects"] = 1
        failures, _, _, _, _ = validate_report(report, [("01-baseline", "03-lazy-fail-caches")])
        self.assertIn(4, failures)

        report = make_profile_report()
        optimized = report["websites"][0]["variants"][1]
        optimized["selector_slow_rejects_summary"]["slow_reject_counts"][TARGET_SELECTORS[0]] += 1
        failures, _, _, _, _ = validate_report(report, [("01-baseline", "03-lazy-fail-caches")])
        self.assertIn(9, failures)

    def test_eight_insertions_fill_capacity_without_counting_as_wrapped(self) -> None:
        report = make_profile_report()
        optimized = report["websites"][0]["variants"][1]
        instrumentation = optimized["summary"]["counts"]["fail_cache_instrumentation"]
        self.assertEqual(instrumentation["caches"][1]["insertions"], 8)
        self.assertEqual(instrumentation["caches"][1]["final_size"], 8)
        self.assertEqual(optimized["summary"]["counts"]["filled_fail_caches"], 1)

        failures, _, _, _, _ = validate_report(
            report, [("01-baseline", "03-lazy-fail-caches")]
        )
        self.assertNotIn(6, failures)

    def test_hashing_bound_uses_slow_rejecting_selector_occurrences(self) -> None:
        report = make_profile_report()
        instrumentation = report["websites"][0]["variants"][1]["summary"]["counts"]["fail_cache_instrumentation"]
        instrumentation["prefixes"][0]["prefix_occurrences"] = 11
        instrumentation["prefixes"][0]["slow_rejecting_prefix_occurrences"] = 11
        instrumentation["prefixes"][0]["hashings"] = 11
        instrumentation["prefixes"][1]["slow_rejecting_prefix_occurrences"] = 6
        instrumentation["prefixes"][1]["hashings"] = 7
        selector = TARGET_SELECTORS[0]
        report["websites"][0]["variants"][1]["selector_slow_rejects_summary"]["slow_reject_counts"][selector] = 4

        failures, _, _, observations, _ = validate_report(
            report, [("01-baseline", "03-lazy-fail-caches")]
        )
        self.assertNotIn(7, failures)
        self.assertIn(75, failures)
        self.assertIn(8, failures)
        self.assertTrue(any("aggregate prefix hashings 25 > element-cache insertions 17" in issue for issue in failures[8]))
        self.assertIn(10, failures)

        report = make_profile_report()
        instrumentation = report["websites"][0]["variants"][1]["summary"]["counts"]["fail_cache_instrumentation"]
        del instrumentation["prefixes"][0]["slow_rejecting_prefix_occurrences"]
        failures, _, _, _, _ = validate_report(
            report, [("01-baseline", "03-lazy-fail-caches")]
        )
        self.assertIn(75, failures)

    def test_havel_hakimi_rejects_repeated_prefixes_within_virtual_cache(self) -> None:
        report = make_profile_report()
        instrumentation = report["websites"][0]["variants"][1]["summary"]["counts"]["fail_cache_instrumentation"]
        instrumentation["prefixes"][0]["insertions"] = 9
        for prefix in instrumentation["prefixes"][1:8]:
            prefix["insertions"] = 1

        failures, _, _, _, _ = validate_report(
            report, [("01-baseline", "03-lazy-fail-caches")]
        )
        self.assertNotIn(7, failures)
        self.assertIn(77, failures)

    def test_missing_target_selector_is_incomplete_or_a_full_suite_failure(self) -> None:
        report = make_profile_report()
        for variant in report["websites"][0]["variants"]:
            variant["selector_slow_rejects_summary"]["means_cycles"].pop(TARGET_SELECTORS[-1])
            variant["selector_slow_rejects_summary"]["slow_reject_counts"].pop(TARGET_SELECTORS[-1])

        pair = [("01-baseline", "03-lazy-fail-caches")]
        failures, skipped, _, _, _ = validate_report(report, pair)
        self.assertNotIn(10, failures)
        self.assertIn(TARGET_SELECTORS[-1], skipped[pair[0]])

        failures, _, _, _, _ = validate_report(report, pair, require_all_targets=True)
        self.assertIn(10, failures)

    def test_legacy_report_includes_interning_in_overall_time(self) -> None:
        site = make_profile_report()["websites"][0]
        baseline, optimized = site["variants"][:2]
        legacy = {
            "websites": [
                {
                    "website": site["website"],
                    "summary": {
                        "baseline": copy.deepcopy(baseline["summary"]),
                        "fail_caches": copy.deepcopy(optimized["summary"]),
                        "fail_cache_preprocessing": {"mean_interning_cycles": 25},
                    },
                    "selector_slow_rejects_summary": {
                        "baseline": baseline["selector_slow_rejects_summary"],
                        "fail_caches": optimized["selector_slow_rejects_summary"],
                    },
                }
            ]
        }
        _, _, timing, _, _ = validate_report(
            legacy, [("Baseline", "Interning + Fail Caches")]
        )
        self.assertEqual(timing[("Baseline", "Interning + Fail Caches")][0][1:], (1000, 925))


if __name__ == "__main__":
    unittest.main()
