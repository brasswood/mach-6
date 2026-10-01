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
