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

# Run historical parity profiles on the comparison websites
cargo bench --bench all_websites -- \
  --profile benches/all_websites/profiles/01-selector-map.json \
  --profile benches/all_websites/profiles/02-bloom-filter.json \
  --profile benches/all_websites/profiles/03-style-sharing.json \
  --profile benches/all_websites/profiles/04-is-conversion.json \
  --profile benches/all_websites/profiles/05-none-bucket.json \
  --profile benches/all_websites/profiles/06-common-pseudo-bucket.json \
  --profile benches/all_websites/profiles/07-common-pseudo-bloom-hash.json \
  --profile benches/all_websites/profiles/08-distribution.json \
  --profile benches/all_websites/profiles/09-edge-child-bloom.json \
  --profile benches/all_websites/profiles/10-eager-fail-caches.json \
  --profile benches/all_websites/profiles/11-universal-tail-bless-lists.json \
  --profile benches/all_websites/profiles/12-lazy-fail-cache-prefixes.json \
  cnn.com amazon.com google.com shopify.com

# Build reports index
python3 scripts/generate_reports_index.py
