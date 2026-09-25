#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH
export NIGHTLY=1

# Get npm dependencies (i.e., typescript)
npm ci

# Get the websites suite
git submodule update --init

# clean the benchmarks directory
rm -r target/all_websites_report

# Run pre-fix parity profiles on the comparison websites
cargo bench --bench all_websites -- \
  --profile benches/all_websites/profiles/10-eager-fail-caches.json \
  --profile benches/all_websites/profiles/11-universal-tail-bless-lists.json \
  --profile benches/all_websites/profiles/12-lazy-fail-cache-prefixes.json \
  cnn.com amazon.com youtube.com shopify.com

# Build reports index
python3 scripts/generate_reports_index.py
