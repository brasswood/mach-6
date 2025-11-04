#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# Run benchmarks
cargo bench

# make report/index.html accessible in the criterion/ directory
ln -s target/criterion/report/index.html target/criterion/