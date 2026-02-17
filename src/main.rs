/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::{collections::HashMap, path::PathBuf};
use clap::Parser;
use mach_6::{Algorithm, result::Result, structs::{ser::SerDocumentMatches, set::SetDocumentMatches}};
use serde_yml;
use selectors::matching::Statistics;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The directory of websites
    websites: PathBuf,
}

fn main() -> mach_6::result::Result<()> {
    env_logger::builder().filter_level(log::LevelFilter::Warn).init();
    let Args{ websites } = Args::parse();
    let result: Result<Vec<(String, SetDocumentMatches, Statistics)>> = mach_6::do_all_websites(&websites, Algorithm::Naive)?.collect();
    let result: HashMap<String, SerDocumentMatches> = result?
        .into_iter()
        .map(|(name, matches, _stats)| (name, SerDocumentMatches::from(matches)))
        .collect();
    println!("{}", serde_yml::to_string(&result).unwrap());
    Ok(())
}
