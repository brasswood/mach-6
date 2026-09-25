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

# Run historical parity profiles on the four comparison websites
cargo bench --bench all_websites --features measure_fail_cache_fill -- \
  --profile benches/all_websites/profiles/parity/03-style-sharing.json \
  --profile benches/all_websites/profiles/parity/04-is-conversion.json \
  --profile benches/all_websites/profiles/parity/05-none-bucket.json \
  --profile benches/all_websites/profiles/parity/06-common-pseudo-bucket.json \
  --profile benches/all_websites/profiles/parity/07-common-pseudo-bloom-hash.json \
  --profile benches/all_websites/profiles/parity/08-distribution.json \
  --profile benches/all_websites/profiles/parity/09-edge-child-bloom.json \
  --profile benches/all_websites/profiles/parity/10-eager-fail-caches.json \
  --profile benches/all_websites/profiles/parity/11-universal-tail-bless-lists.json \
  --profile benches/all_websites/profiles/parity/12-lazy-fail-cache-prefixes.json \
  cnn.com amazon.com youtube.com shopify.com

# Build reports index
python3 scripts/generate_reports_index.py
