/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::collections::BTreeMap;

use mach_6::{
    Optimizations,
    parse::{get_all_documents_and_selectors, websites_path},
};

fn main() -> mach_6::result::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let mut report = BTreeMap::new();
    for website in get_all_documents_and_selectors(&websites_path())? {
        let website = website?;
        let matcher = website.get_matcher(Optimizations::from_none());
        report.insert(website.name, matcher.get_universal_bucket_selectors());
    }

    for (website, selectors) in report {
        println!("{website}:");
        if selectors.is_empty() {
            println!("  (none)");
        }
        for selector in selectors {
            println!("  {selector}");
        }
    }
    Ok(())
}
