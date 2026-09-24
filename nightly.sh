#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH
export NIGHTLY=1

# Get npm dependencies (i.e., typescript)
npm ci

# Get the websites suite
git submodule update --init

# Google is archived in the controlled snapshot; expose it to the benchmark.
rm -rf websites/google.com
unzip -q websites/google.com.zip -d websites
trap 'rm -rf websites/google.com' EXIT

# clean the benchmarks directory
rm -r target/all_websites_report

# Run benchmarks
cargo bench --features measure_fail_cache_fill -- cnn.com amazon.com google.com shopify.com

# Build reports index
python3 scripts/generate_reports_index.py
