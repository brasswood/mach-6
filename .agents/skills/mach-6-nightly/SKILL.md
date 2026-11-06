---
name: mach-6-nightly
description: "Run mach-6 nightly benchmarks or retrieve or analyze reports. Use when running the middle-level benchmark suite on nightly, finding a nightly report, downloading its JSON, or analyzing a report's data."
---

# Mach-6 Nightly

## Middle-level Benchmarking on Nightly
Make sure to push the branch you're benchmarking (and submodules!) to GitHub first. Then, read the `nightlies` skill. Use `nightlies sync [--wait]` to make Nightly sync with GitHub, then use `nightlies start` to start the branch. If the nightly server is down or not working, fall back to running `cargo bench` locally.

The nightly server is designed to run a fixed `nightly.sh` per branch. There is no clean way to pass parameters to the nightly run; the best you can do is to modify `nightly.sh` and push that. This could be done, for example, on a separate branch whose purpose is to configure a different nightly run.

## Retrieving and Analyzing Reports
