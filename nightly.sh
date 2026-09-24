#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# clean the benchmarks directory
rm -r target/all_websites_report

# Restrict this harness, which only accepts Cargo's automatic --bench argument.
parity_websites_tmp=$(mktemp -d)
mv websites "$parity_websites_tmp/all"
mkdir websites
for site in cnn.com amazon.com google.com shopify.com; do
    ln -s "$parity_websites_tmp/all/$site" "websites/$site"
done
restore_websites() {
    rm -rf websites
    mv "$parity_websites_tmp/all" websites
    rmdir "$parity_websites_tmp"
}
trap restore_websites EXIT

# Run benchmarks
cargo bench
