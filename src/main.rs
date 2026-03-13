/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::{collections::HashMap, path::PathBuf};
use clap::Parser;
use mach_6::{
    Algorithm,
    parse::get_document_and_selectors,
    result::Result,
    structs::{ser::SerDocumentMatches, set::SetDocumentMatches},
};
use serde_yml;
use selectors::matching::Statistics;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The directory of website folders
    #[arg(long, conflicts_with = "website")]
    websites: Option<PathBuf>,

    /// A single website folder containing one html file
    #[arg(long, conflicts_with = "websites")]
    website: Option<PathBuf>,

    /// Which matching algorithm to run
    #[arg(long, value_enum, default_value_t = Algorithm::Naive)]
    algorithm: Algorithm,
}

fn main() -> mach_6::result::Result<()> {
    env_logger::builder().filter_level(log::LevelFilter::Warn).init();
    let Args {
        websites,
        website,
        algorithm,
    } = Args::parse();
    let result: Result<Vec<(String, SetDocumentMatches, Statistics)>> = if let Some(website) = website {
        Ok(get_document_and_selectors(&website)?
            .map(|website| vec![mach_6::do_website(&website, algorithm)])
            .unwrap_or_default())
    } else {
        let websites = websites.unwrap_or_else(|| PathBuf::from("websites"));
        mach_6::do_all_websites(&websites, algorithm)?.collect()
    };
    let result: HashMap<String, SerDocumentMatches> = result?
        .into_iter()
        .map(|(name, matches, _stats)| (name, SerDocumentMatches::from(matches)))
        .collect();
    println!("{}", serde_yml::to_string(&result).unwrap());
    Ok(())
}
