# AGENTS.md

## Project Summary
This is a project for trying to improve selector matching times. More info coming later.

This repository contains a benchmark harness in the `benches` folder which benchmarks the code and generates an HTML report on how selector matching performs for the benchmarked websites.

## How to Work
Treat all requests to edit files as requests to create a commit or series of commits per the global AGENTS.md.

Always commit on a branch prefixed with `ai-`. Never commit on a branch without that prefix. If you are not currently on such a branch, either find a suitable existing branch prefixed with `ai-`, or create a new one.

## Benchmarking

### Middle-level Benchmarking

The binary used in `cargo bench`/the nightly server (introduced next) is designed to provide a "middle-level" report of where time is spent in the program. It times the main stages of selector matching and our optimizations (e.g. querying bloom filter, selector indexing) and generates a report. This is the primary report we use to evaluate our techniques.

The nightly server is the preferred server to run the middle-level suite. To use it, make sure to push the branch you're benchmarking (and submodules!) to Github first, then use `uvx nightlies` to interface with it. If the nightly server is down or not working, fall back to running `cargo bench` locally.

The nightly server is designed to run a fixed routine per branch. There is probably no clean way to pass parameters to the routine; you would probably have to create a different routine on a different branch.

### Low-level Benchmarking

Sometimes, you may want to measure something finer-grained than what the middle-level report provides. For example, you may want to know how time is being spent within one stage.

One option is to run `samply` locally. Another could be to isolate one function/code block and create a binary which times it (i.e. the typical microbenchmarking workflow). You can try other methods that you think are reasonable.
