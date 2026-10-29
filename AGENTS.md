# AGENTS.md

## Project Summary
This is a project for trying to improve selector matching times.

The primary entry points are `./src/lib.rs:match_selectors*`. These functions take an `Html` tree and a list of `Selector`s, and produce a data structure that records, for each element, which selectors match it. (Currently, the list of `Selector`s is passed to `match_selectors_with_style_sharing*` inside of a `MatchingContext`.)

Tests are in `./tests/`. `./tests/websites_suite.rs` compares the matching results of each selector matching algorithm on the corpus of websites in `websites/`. It is the primary correctness test for our algorithms/optimizations. The naive `match_selectors` algorithm is the reference.

This repository contains a benchmark harness in the `benches` folder which benchmarks the code and generates an HTML report on how selector matching performs for the benchmarked websites.

## Priorities
Correctness is top priority. `cargo test` in the `mach-6` superproject should pass.

Performance of the matcher is a close second. Write as performant code as possible in the matcher.

Simple, understandable code is the third priority, but do not sacrifice major performance just to achieve this.

## Benchmarking

### Middle-level Benchmarking

The binary used in `cargo bench`/the nightly server (introduced next) is designed to provide a "middle-level" report of where time is spent in the program. It times the main stages of selector matching and our optimizations (e.g. querying bloom filter, selector indexing) and generates a report. This is the primary report we use to evaluate our techniques.

The nightly server is the preferred server to run the middle-level suite. To use it, make sure to push the branch you're benchmarking (and submodules!) to Github first, then read the `nightlies` skill to learn how to interface with it, or use `uvx nightlies` if the skill is not available. If the nightly server is down or not working, fall back to running `cargo bench` locally.

The nightly server is designed to run a fixed routine per branch. There is probably no clean way to pass parameters to the routine; you would probably have to create a different routine on a different branch.

### Low-level Benchmarking

Sometimes, you may want to measure something finer-grained than what the middle-level report provides. For example, you may want to know how time is being spent within one stage.

One option is to run `samply` locally. Another could be to isolate one function/code block and create a binary which times it (i.e. the typical microbenchmarking workflow). You can try other methods that you think are reasonable.
