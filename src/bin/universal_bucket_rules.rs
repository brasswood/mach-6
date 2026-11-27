/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::collections::BTreeMap;

use clap::{Parser, ValueEnum};
use mach_6::{
    Optimizations,
    parse::{get_all_documents_and_selectors, websites_path},
};

#[derive(Debug, Parser)]
#[command(about = "Report selectors indexed by universal-bucket rules")]
struct Args {
    #[arg(long, value_enum, default_value = "plain")]
    format: Format,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Plain,
    Json,
}

fn main() -> mach_6::result::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse();

    let mut report = BTreeMap::new();
    for website in get_all_documents_and_selectors(&websites_path())? {
        let website = website?;
        let matcher = website.get_matcher(Optimizations::from_none());
        report.insert(website.name, matcher.get_universal_bucket_selectors());
    }

    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Format::Plain => {
            for (website, selectors) in report {
                println!("{website}:");
                if selectors.is_empty() {
                    println!("  (none)");
                }
                for selector in selectors {
                    println!("  {selector}");
                }
            }
        }
    }
    Ok(())
}
