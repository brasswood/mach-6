#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get npm dependencies (i.e., typescript)
npm ci

# Get the websites suite
git submodule update --init

# clean the benchmarks directory
rm -r target/all_websites_report

# Run benchmarks
cargo bench
