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
