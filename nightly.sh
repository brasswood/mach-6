#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# clean the benchmarks directory
rm -r target/all_websites_report

# Run benchmarks
cargo bench
