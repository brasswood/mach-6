#!/usr/bin/env python3
"""Check fail-cache invariants in an all_websites benchmark report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

TARGET_SELECTORS = (
    'body[dir="rtl"].layout-homepage :not([class*="ui"], .material-icons)',
    '[dir="rtl"] .card__label-bull-span',
    ':is(:is(:is(:is([data-qa="TemplatePricingMatrix"]) [role="table"]) > [role="rowgroup"]):last-child) > :first-child',
    ':is([data-qa="TemplateCarouselCarousel"]) [data-qa="TemplateCarouselContainer"]',
    'body.etsy-has-it-design:not(.wt-focus-visible) :is(#gnav-header-inner .wt-tooltip__trigger, #gnav-header-inner [data-id="hamburger"], #gnav-header-inner .simplified-mobile-header-sign-in-icon, #gnav-header-inner [data-search-back-btn], #header-locale-picker-trigger):focus .etsy-icon',
)

Comparison = tuple[str, str]

CHECK_DESCRIPTIONS = {
    1: "After fail caches, overall time should not go up from before fail caches",
    2: "After fail caches, slow accepts should remain the same as before fail caches",
    3: "After fail caches, slow rejects should not increase from before fail caches",
    4: "If slow rejects decrease after fail caches, fail-cache rejects should increase by the same amount",
    5: "Fail-cache insertions should not be less than eight times the number of fail caches overflowed",
    6: "Instrument each fail cache's number of insertions and final size",
    61: "Each fail cache's final size should be min(num insertions, 8)",
    62: "The number of filled fail caches should equal the number whose insertions exceed 8",
    "7 instrumentation": "Instrument the number of times each unique prefix is hashed, interned, and inserted into a fail cache",
    71: "For each unique prefix, hashings should be greater than or equal to internments",
    72: "For each unique prefix, hashings should be less than or equal to its non-unique prefix occurrences from fail-cacheable selectors with more than 0 slow rejects",
    73: "Havel-Hakimi check on duplicate prefixes in fail caches",
    8: "For each website, aggregate hashings should be less than or equal to aggregate fail-cache insertions",
    9: "Per-selector slow-reject counts should be present and sum to total slow rejects",
    10: "Specific selectors should decrease in both slow-rejecting time and slow-reject count",
}


def parse_comparison(spec: str) -> Comparison:
    baseline, separator, optimized = spec.partition(":")
    if not separator or not baseline.strip() or not optimized.strip():
        raise argparse.ArgumentTypeError("comparison must be BASELINE:OPTIMIZED")
    return baseline.strip(), optimized.strip()


def resolve_pair(
    report: dict[str, Any], website: dict[str, Any], comparison: Comparison
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any], int]:
    baseline_label, optimized_label = comparison
    if "summary" in website:
        if baseline_label.lower() != "baseline" or optimized_label.lower() not in {
            "fail_caches", "fail caches", "interning + fail caches"
        }:
            raise ValueError("legacy reports compare Baseline to Interning + Fail Caches")
        baseline = website["summary"]["baseline"]
        optimized = website["summary"]["fail_caches"]
        selectors = website["selector_slow_rejects_summary"]
        total_cycles = optimized["mean_cycles"] + website["summary"][
            "fail_cache_preprocessing"
        ]["mean_interning_cycles"]
        return baseline, optimized, selectors["baseline"], selectors["fail_caches"], total_cycles

    labels = {entry["label"]: entry["id"] for entry in report["metadata"]["variants"]}
    variants = {entry["variant_id"]: entry for entry in website["variants"]}
    baseline = variants[labels[baseline_label]]
    optimized = variants[labels[optimized_label]]
    return (
        baseline["summary"],
        optimized["summary"],
        baseline["selector_slow_rejects_summary"],
        optimized["selector_slow_rejects_summary"],
        optimized["summary"]["mean_cycles"],
    )


def record_failure(
    failures: dict[int, list[str]], number: int, context: str, detail: str
) -> None:
    failures.setdefault(number, []).append(f"{context}: {detail}")


def validate_cache_storage(
    summary: dict[str, Any], context: str, failures: dict[int, list[str]]
) -> dict[str, Any] | None:
    counts = summary["counts"]
    instrumentation = counts.get("fail_cache_instrumentation")
    if instrumentation is None:
        for number in (5, 6, 61, 62, 7, 8, 71, 72, 73):
            record_failure(failures, number, context, "detailed instrumentation is missing")
        return None

    caches = instrumentation["caches"]
    insertions = [cache["insertions"] for cache in caches]
    overflowed = sum(value > 8 for value in insertions)
    if sum(insertions) < 8 * overflowed:
        record_failure(
            failures, 5, context,
            f"{sum(insertions)} insertions for {overflowed} caches with more than 8 insertions",
        )
    if len(caches) != counts.get("total_fail_caches"):
        record_failure(
            failures, 6, context,
            f"reported {len(caches)} element caches but total_fail_caches is {counts.get('total_fail_caches')}",
        )
    for cache in caches:
        if cache["final_size"] != min(cache["insertions"], 8):
            record_failure(
                failures, 61, context,
                f"cache {cache['element_index']} size {cache['final_size']} != min({cache['insertions']}, 8)",
            )
    if counts.get("filled_fail_caches") != overflowed:
        record_failure(
            failures, 62, context,
            f"filled count {counts.get('filled_fail_caches')} != {overflowed} caches whose ring wrapped after more than 8 insertions",
        )
    return instrumentation


def havel_hakimi_accepts(prefix_insertions: list[int], cache_insertions: list[int]) -> bool:
    if any(count < 0 for count in prefix_insertions + cache_insertions):
        return False

    virtual_cache_insertions: list[int] = []
    for count in cache_insertions:
        virtual_cache_insertions.extend([8] * (count // 8))
        if count % 8:
            virtual_cache_insertions.append(count % 8)

    remaining_prefix_insertions = sorted(prefix_insertions, reverse=True)
    for count in sorted(virtual_cache_insertions, reverse=True):
        remaining_prefix_insertions.sort(reverse=True)
        if count > len(remaining_prefix_insertions) or any(
            insertion == 0 for insertion in remaining_prefix_insertions[:count]
        ):
            return False
        for index in range(count):
            remaining_prefix_insertions[index] -= 1
    return all(insertion == 0 for insertion in remaining_prefix_insertions)


def validate_prefixes(
    instrumentation: dict[str, Any], context: str,
    failures: dict[int, list[str]], observations: dict[int, list[str]],
) -> None:
    prefixes = instrumentation["prefixes"]
    caches = instrumentation["caches"]
    for prefix in prefixes:
        index = prefix["prefix_index"]
        hashings = prefix["hashings"]
        internments = prefix["internments"]
        slow_rejecting_occurrences = prefix.get("slow_rejecting_prefix_occurrences")
        if hashings < internments:
            record_failure(failures, 71, context, f"prefix {index} hashes fewer times than it is interned")
        if slow_rejecting_occurrences is None:
            record_failure(failures, 72, context, f"prefix {index} slow-rejecting selector occurrences are missing")
        elif hashings > slow_rejecting_occurrences:
            record_failure(
                failures, 72, context,
                f"prefix {index} hashings {hashings} > slow-rejecting selector occurrences {slow_rejecting_occurrences}",
            )

    prefix_insertions = sum(prefix["insertions"] for prefix in prefixes)
    cache_insertions = sum(cache["insertions"] for cache in caches)
    if prefix_insertions != cache_insertions:
        record_failure(
            failures, 7, context,
            f"prefix insertion total {prefix_insertions} != element-cache insertion total {cache_insertions}",
        )

    if not havel_hakimi_accepts(
        [prefix["insertions"] for prefix in prefixes],
        [cache["insertions"] for cache in caches],
    ):
        record_failure(failures, 73, context, "prefix insertions cannot be assigned to virtual fail caches")

    hashings = sum(prefix["hashings"] for prefix in prefixes)
    if hashings > cache_insertions:
        record_failure(
            failures, 8, context,
            f"aggregate prefix hashings {hashings} > element-cache insertions {cache_insertions}",
        )


def validate_comparison_metrics(
    baseline: dict[str, Any], optimized: dict[str, Any],
    baseline_selectors: dict[str, Any], optimized_selectors: dict[str, Any],
    optimized_cycles: int, context: str, failures: dict[int, list[str]],
    timing_observations: list[tuple[str, int, int]], counts_only: bool,
) -> None:
    before, after = baseline["counts"], optimized["counts"]
    timing_observations.append((context, baseline["mean_cycles"], optimized_cycles))
    if not counts_only and optimized_cycles > baseline["mean_cycles"]:
        record_failure(
            failures, 1, context,
            f"overall time increased {baseline['mean_cycles']} -> {optimized_cycles} cycles",
        )
    if after["slow_accepts"] != before["slow_accepts"]:
        record_failure(failures, 2, context, f"slow accepts changed {before['slow_accepts']} -> {after['slow_accepts']}")
    if after["slow_rejects"] > before["slow_rejects"]:
        record_failure(failures, 3, context, f"slow rejects increased {before['slow_rejects']} -> {after['slow_rejects']}")
    decrease = before["slow_rejects"] - after["slow_rejects"]
    reject_increase = after["fail_cache_rejects"] - before["fail_cache_rejects"]
    if decrease > 0 and reject_increase != decrease:
        record_failure(
            failures, 4, context,
            f"slow rejects decreased by {decrease}, fail-cache rejects increased by {reject_increase}",
        )
    for label, stats, slow_rejects in (
        ("baseline", baseline_selectors, before["slow_rejects"]),
        ("optimized", optimized_selectors, after["slow_rejects"]),
    ):
        counts = stats.get("slow_reject_counts")
        if counts is None:
            record_failure(failures, 9, context, f"{label} per-selector slow-reject counts are missing")
            continue
        per_selector_slow_rejects = sum(counts.values())
        if per_selector_slow_rejects != slow_rejects:
            record_failure(
                failures, 9, context,
                f"{label} per-selector slow-reject counts sum to {per_selector_slow_rejects}, "
                f"but total slow rejects are {slow_rejects}",
            )


def new_target_totals() -> dict[str, dict[str, Any]]:
    return {
        selector: {
            "baseline_cycles": 0, "optimized_cycles": 0,
            "baseline_count": 0, "optimized_count": 0, "present": False,
        }
        for selector in TARGET_SELECTORS
    }


def accumulate_target_totals(
    totals: dict[str, dict[str, Any]], baseline: dict[str, Any], optimized: dict[str, Any]
) -> None:
    for selector, values in totals.items():
        before_counts = baseline.get("slow_reject_counts", {})
        after_counts = optimized.get("slow_reject_counts", {})
        before_times = baseline.get("means_cycles", {})
        after_times = optimized.get("means_cycles", {})
        values["present"] |= (
            selector in before_counts or selector in before_times
            or selector in after_counts or selector in after_times
        )
        values["baseline_count"] += before_counts.get(selector, 0)
        values["optimized_count"] += after_counts.get(selector, 0)
        values["baseline_cycles"] += before_times.get(selector, 0)
        values["optimized_cycles"] += after_times.get(selector, 0)


def validate_report(
    report: dict[str, Any], comparisons: list[Comparison], counts_only: bool = False,
) -> tuple[
    dict[int, list[str]], dict[Comparison, list[str]],
    dict[Comparison, list[tuple[str, int, int]]], dict[int, list[str]],
    dict[Comparison, dict[str, dict[str, Any]]],
]:
    failures: dict[int, list[str]] = {}
    skipped_targets: dict[Comparison, list[str]] = {}
    timing_observations: dict[Comparison, list[tuple[str, int, int]]] = {}
    assumption_observations: dict[int, list[str]] = {}
    target_observations: dict[Comparison, dict[str, dict[str, Any]]] = {}
    websites = report["websites"]
    if not websites:
        raise ValueError("benchmark report contains no websites")

    for comparison in comparisons:
        totals = new_target_totals()
        timing_observations[comparison] = []
        for website in websites:
            context = f"{comparison[0]} -> {comparison[1]}, {website['website']}"
            baseline, optimized, before_selectors, after_selectors, cycles = resolve_pair(
                report, website, comparison
            )
            validate_comparison_metrics(
                baseline, optimized, before_selectors, after_selectors, cycles,
                context, failures, timing_observations[comparison], counts_only,
            )
            instrumentation = validate_cache_storage(optimized, context, failures)
            if instrumentation is not None:
                validate_prefixes(
                    instrumentation, context, failures, assumption_observations,
                )
            accumulate_target_totals(totals, before_selectors, after_selectors)

        skipped = []
        for selector, values in totals.items():
            if not values["present"]:
                skipped.append(selector)
                record_failure(failures, 10, f"{comparison[0]} -> {comparison[1]}", f"target selector is absent: {selector}")
                continue
            before = (values["baseline_cycles"], values["baseline_count"])
            after = (values["optimized_cycles"], values["optimized_count"])
            if (not counts_only and after[0] >= before[0]) or after[1] >= before[1]:
                record_failure(
                    failures, 10, f"{comparison[0]} -> {comparison[1]}",
                    (
                        f"target did not reduce slow rejects: {selector}: {before[1]} -> {after[1]}"
                        if counts_only else
                        f"target did not reduce both time and slow rejects: {selector}: {before} -> {after}"
                    ),
                )
        skipped_targets[comparison] = skipped
        target_observations[comparison] = totals

    return failures, skipped_targets, timing_observations, assumption_observations, target_observations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="all_websites report JSON file")
    parser.add_argument(
        "--compare", required=True, action="append", type=parse_comparison,
        metavar="BASELINE:OPTIMIZED", help="variant labels to compare; may be repeated",
    )
    parser.add_argument(
        "--counts-only", action="store_true",
        help="skip timing properties while continuing to enforce count properties",
    )
    args = parser.parse_args()
    try:
        report = json.loads(args.report.read_text())
        failures, skipped, timing, observations, targets = validate_report(
            report, args.compare, args.counts_only
        )
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        parser.error(f"cannot validate report: {error}")

    for comparison, values in timing.items():
        issues = failures.get(1, [])
        comparison_issues = [issue for issue in issues if issue.startswith(f"{comparison[0]} -> {comparison[1]},")]
        status = "SKIP" if args.counts_only else "FAIL" if comparison_issues else "PASS"
        print(f"[1] {status} {comparison[0]} -> {comparison[1]}: overall time including fail-cache setup")
        if comparison_issues:
            print("  - This is workload-dependent: lookup overhead applies even when a prefix is never reused on the same element.")
        for website, before, after in values:
            print(f"  - {website}: {before} -> {after} cycles ({after - before:+})")

    for number in (2, 3, 4, 5, 6, 7, 8, 9, 10):
        issues = failures.get(number, [])
        if number == 7:
            print(f"[7 instrumentation] {'FAIL' if issues else 'PASS'}")
            for issue in issues:
                print(f"  - {issue}")
            hashing_issues = failures.get(71, [])
            print(f"[7.1] {'FAIL' if hashing_issues else 'PASS'}")
            for issue in hashing_issues:
                print(f"  - {issue}")
            slow_rejecting_issues = failures.get(72, [])
            print(f"[7.2] {'FAIL' if slow_rejecting_issues else 'PASS'}")
            for issue in slow_rejecting_issues:
                print(f"  - {issue}")
            print(f"[7.3] {'FAIL' if failures.get(73) else 'PASS'}")
            for issue in failures.get(73, []):
                print(f"  - {issue}")
        elif number == 6:
            print(f"[6] {'FAIL' if issues else 'PASS'}")
            for issue in issues:
                print(f"  - {issue}")
            size_issues = failures.get(61, [])
            print(f"[6.1] {'FAIL' if size_issues else 'PASS'}")
            for issue in size_issues:
                print(f"  - {issue}")
            filled_issues = failures.get(62, [])
            print(f"[6.2] {'FAIL' if filled_issues else 'PASS'}")
            for issue in filled_issues:
                print(f"  - {issue}")
        elif number == 10:
            incomplete = any(skipped[comparison] for comparison in skipped)
            status = "FAIL" if issues else "INCOMPLETE" if incomplete else "PASS"
            print(f"[{number}] {status}")
            if issues:
                print("  - A fail cache only avoids work when the same prefix is queried again on the same element; a selector's slow rejects need not be reusable.")
            for observation in observations.get(number, []):
                print(f"  - {observation}")
        elif number == 8:
            print(f"[8] {'FAIL' if issues else 'PASS'}")
            for observation in observations.get(number, []):
                print(f"  - {observation}")
        else:
            print(
                f"[{number}] {'FAIL' if issues else 'PASS'}"
                + (f" ({len(issues)} issues)" if issues else "")
            )
        if number not in (6, 7):
            for issue in issues:
                print(f"  - {issue}")
        if number == 10:
            for comparison, selectors in targets.items():
                for selector, values in selectors.items():
                    if values["present"]:
                        print(
                            f"  - {comparison[0]} -> {comparison[1]} {selector}: "
                            f"cycles {values['baseline_cycles']} -> {values['optimized_cycles']}, "
                            f"slow rejects {values['baseline_count']} -> {values['optimized_count']}"
                        )
                if skipped[comparison]:
                    print(f"  - {comparison[0]} -> {comparison[1]}: {len(skipped[comparison])} targets absent from this report subset")

    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
