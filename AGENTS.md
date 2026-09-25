# AGENTS.md

## Project Summary
This is a project for trying to improve selector matching times. More info coming later.

This repository contains a benchmark harness in the `benches` folder which benchmarks the code and generates an HTML report on how selector matching performs for the benchmarked websites.

## Benchmarking

If you need to run benchmarks, the nightly server is the preferred place to do it. Make sure to push the branch you're benchmarking to Github first, then use `uvx nightlies` to interface with the nightly server. If the nightly server is not working, fall back to running `cargo bench` locally.
