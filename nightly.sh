#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH
export NIGHTLY=1

# Get npm dependencies (i.e., typescript)
npm ci

# Get the websites suite
git submodule update --init
rm -rf websites/google.com
unzip -q websites/google.com.zip -d websites
trap 'rm -rf websites/google.com' EXIT

# clean the benchmarks directory
rm -r target/all_websites_report

# Run optimization profiles on all websites
cargo bench --bench all_websites -- \
  --profile benches/all_websites/profiles/01-style-sharing-selector-map-bloom-filter.json \
  --profile benches/all_websites/profiles/02-style-sharing-selector-map-bloom-filter-fail-caches.json \
  --profile benches/all_websites/profiles/03-style-sharing-selector-map-bloom-filter-fail-caches-lazy-prefixes.json \
  --profile benches/all_websites/profiles/04-style-sharing-selector-map-bloom-filter-fail-caches-lazy-prefixes-bless-lists.json

# Build reports index
python3 scripts/generate_reports_index.py
