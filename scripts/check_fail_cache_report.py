#!/usr/bin/env python3
"""Check fail-cache correctness invariants in an all-websites JSON report."""

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


def parse_comparison(spec: str) -> Comparison:
    """Parse BASELINE:OPTIMIZED, allowing colons in neither label."""
    baseline, separator, optimized = spec.partition(":")
    if not separator or not baseline.strip() or not optimized.strip():
        raise argparse.ArgumentTypeError("comparison must be BASELINE:OPTIMIZED")
    return baseline.strip(), optimized.strip()


def resolve_pair(
    report: dict[str, Any], website: dict[str, Any], comparison: Comparison
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any], int]:
    """Return summaries, selector stats, and optimized total cycles."""
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
    baseline_id, optimized_id = labels[baseline_label], labels[optimized_label]
    variants = {entry["variant_id"]: entry for entry in website["variants"]}
    baseline, optimized = variants[baseline_id], variants[optimized_id]
    return (
        baseline["summary"], optimized["summary"],
        baseline["selector_slow_rejects_summary"],
        optimized["selector_slow_rejects_summary"], optimized["summary"]["mean_cycles"],
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
        for number in (5, 6, 7, 8):
            record_failure(failures, number, context, "detailed fail-cache instrumentation is missing")
        return None

    caches = instrumentation["caches"]
    insertions = [cache["insertions"] for cache in caches]
    overflowed = sum(value > 8 for value in insertions)
    if sum(insertions) < 8 * overflowed:
        record_failure(failures, 5, context, f"{sum(insertions)} insertions for {overflowed} overflowed caches")
    for cache in caches:
        if cache["final_size"] != max(cache["insertions"], 8):
            record_failure(failures, 6, context, f"cache {cache['element_index']} has inconsistent final size")
    if counts.get("filled_fail_caches") != overflowed:
        record_failure(failures, 6, context, f"filled count {counts.get('filled_fail_caches')} != {overflowed} overflowed caches")
    return instrumentation


def validate_prefixes(
    instrumentation: dict[str, Any], context: str, failures: dict[int, list[str]]
) -> None:
    prefixes = instrumentation["prefixes"]
    caches = instrumentation["caches"]
    # Lazy IDs are hashed only when matching reaches their selector occurrence.
    for prefix in prefixes:
        index = prefix["prefix_index"]
        if prefix["hashings"] < prefix["internments"]:
            record_failure(failures, 7, context, f"prefix {index} hashes fewer times than it is interned")
        if prefix["hashings"] > prefix["prefix_occurrences"]:
            record_failure(failures, 7, context, f"prefix {index} hashes more times than it occurs in eligible selectors")

    # A hashed prefix may match successfully and therefore never be inserted.
    hashings = sum(prefix["hashings"] for prefix in prefixes)
    prefix_insertions = sum(prefix["insertions"] for prefix in prefixes)
    insertions = sum(cache["insertions"] for cache in caches)
    # Prefix and element overflow thresholds are independent; only total inserts reconcile.
    if prefix_insertions != insertions:
        record_failure(failures, 7, context, f"prefix insertion total {prefix_insertions} != cache insertion total {insertions}")
    if hashings >= insertions:
        record_failure(failures, 8, context, f"{hashings} aggregate prefix hashings >= {insertions} cache insertions")


def validate_comparison_metrics(
    baseline: dict[str, Any], optimized: dict[str, Any],
    baseline_selectors: dict[str, Any], optimized_selectors: dict[str, Any],
    optimized_cycles: int, context: str, failures: dict[int, list[str]],
) -> None:
    before, after = baseline["counts"], optimized["counts"]
    if optimized_cycles > baseline["mean_cycles"]:
        record_failure(failures, 1, context, f"total cycles increased {baseline['mean_cycles']} -> {optimized_cycles}")
    if after["slow_accepts"] != before["slow_accepts"]:
        record_failure(failures, 2, context, f"slow accepts changed {before['slow_accepts']} -> {after['slow_accepts']}")
    if after["slow_rejects"] > before["slow_rejects"]:
        record_failure(failures, 3, context, f"slow rejects increased {before['slow_rejects']} -> {after['slow_rejects']}")
    decrease = before["slow_rejects"] - after["slow_rejects"]
    reject_increase = after["fail_cache_rejects"] - before["fail_cache_rejects"]
    if decrease > 0 and reject_increase != decrease:
        record_failure(failures, 4, context, f"slow rejects decreased by {decrease}, fail-cache rejects increased by {reject_increase}")
    for label, selector_stats in (("baseline", baseline_selectors), ("optimized", optimized_selectors)):
        if "slow_reject_counts" not in selector_stats:
            record_failure(failures, 9, context, f"{label} per-selector slow-reject counts are missing")


def validate_target_totals(
    totals: dict[str, dict[str, Any]], context: str, failures: dict[int, list[str]],
    require_all: bool,
) -> list[str]:
    skipped = []
    for selector, values in totals.items():
        if not values["present"]:
            if require_all:
                record_failure(failures, 10, context, f"target selector is absent: {selector}")
            else:
                skipped.append(selector)
            continue
        before = (values["baseline_cycles"], values["baseline_count"])
        after = (values["optimized_cycles"], values["optimized_count"])
        if after[0] >= before[0] or after[1] >= before[1]:
            record_failure(failures, 10, context, f"target selector did not reduce both time and rejects: {selector}: {before} -> {after}")
    return skipped


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
    report: dict[str, Any], comparisons: list[Comparison], require_all_targets: bool = False
) -> tuple[dict[int, list[str]], dict[Comparison, list[str]]]:
    failures: dict[int, list[str]] = {}
    skipped_targets: dict[Comparison, list[str]] = {}
    websites = report["websites"]
    if not websites:
        raise ValueError("benchmark report contains no websites")
    for comparison in comparisons:
        totals = new_target_totals()
        for website in websites:
            context = f"{comparison[0]} -> {comparison[1]}, {website['website']}"
            baseline, optimized, before_selectors, after_selectors, cycles = resolve_pair(
                report, website, comparison
            )
            validate_comparison_metrics(
                baseline, optimized, before_selectors, after_selectors, cycles, context, failures
            )
            instrumentation = validate_cache_storage(optimized, context, failures)
            if instrumentation is not None:
                validate_prefixes(instrumentation, context, failures)
            accumulate_target_totals(totals, before_selectors, after_selectors)
        skipped_targets[comparison] = validate_target_totals(
            totals, f"{comparison[0]} -> {comparison[1]}", failures, require_all_targets
        )
    return failures, skipped_targets


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="all_websites report JSON file")
    parser.add_argument(
        "--compare", required=True, action="append", type=parse_comparison,
        metavar="BASELINE:OPTIMIZED", help="variant labels to compare; may be repeated",
    )
    parser.add_argument(
        "--require-all-target-selectors", action="store_true",
        help="fail if any of the five targeted selectors is absent from the report",
    )
    args = parser.parse_args()
    try:
        report = json.loads(args.report.read_text())
        failures, skipped = validate_report(report, args.compare, args.require_all_target_selectors)
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        parser.error(f"cannot validate report: {error}")
    for number in range(1, 11):
        issues = failures.get(number, [])
        print(f"[{number}] {'FAIL' if issues else 'PASS'}" + (f" ({len(issues)} issues)" if issues else ""))
        for issue in issues:
            print(f"  - {issue}")
        if number == 10:
            for comparison, selectors in skipped.items():
                if selectors:
                    print(f"  - {comparison[0]} -> {comparison[1]}: {len(selectors)} targets absent from this report subset")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
