# AGENTS.md

## Project Summary
This is a project for trying to improve selector matching times. More info coming later.

This repository contains a benchmark harness in the `benches` folder which benchmarks the code and generates an HTML report on how selector matching performs for the benchmarked websites.

## How to Work

When making changes that the user has specified at some low level that's aware of the code (e.g., "refactor this," "add this parameter," "implement this method"), work in the current working tree. In this mode, let the user commit changes unless they ask you to. Also work in this mode when doing collaborative programming with the user. Here, collaborative programming means workshopping changes with the user and then implementing them.

When autonomously working on big tasks or high level goals (e.g., "add this feature"), work in `../mach-6-worktree` on a branch or branches prefixed by `ai-`. The exception to this is tasks on the HTML report. If something has already been designed in the Rust code in previous chat messages, and the user asks for a change only on the HTML/JS report, then you should make the change in the working tree, even if that change is specified at a high level. If, however, changing the report is part of a high-level task you are working on that also significantly affects the Rust codebase, then you should do it in the separate branch(es).

When working in a separate branch or branches, you can create new branches prefixed with `ai-` or work on existing ones prefixed with `ai-` as you see fit. You should also create commits yourself. As usual, organize your changes into reviewable, defensible commits.

## Benchmarking

If you need to run benchmarks, the nightly server is the preferred place to do it. Make sure to push the branch you're benchmarking to Github first, then use `uvx nightlies` to interface with the nightly server. If the nightly server is not working, fall back to running `cargo bench` locally.
