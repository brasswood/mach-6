/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::PathBuf;
use clap::Parser;
use mach_6::{Result, DocumentMatches};
use serde_yml;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The directory of websites
    websites: PathBuf,
}

fn main() -> mach_6::Result<()> {
    let Args{ websites } = Args::parse();
    let result: Result<Vec<DocumentMatches>> = mach_6::do_all_websites(websites)?.collect();
    let result = result?;
    println!("{}", serde_yml::to_string(&result).unwrap());
    Ok(())
}
