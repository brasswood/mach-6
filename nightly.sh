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

# Run representative parity profiles on the comparison websites
cargo bench --bench all_websites -- \
  --profile benches/all_websites/profiles/03-style-sharing.json \
  --profile benches/all_websites/profiles/08-distribution.json \
  cnn.com amazon.com youtube.com shopify.com

# Build reports index
python3 scripts/generate_reports_index.py
