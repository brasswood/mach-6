/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::PathBuf;
use mach_6::{Algorithm, Result};
use insta;

#[test]
fn does_all_websites() -> Result<()> {
    let websites_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let results = mach_6::do_all_websites(&websites_path, Algorithm::Naive)?;
    insta::with_settings!({ snapshot_path => websites_path.join("snapshots")}, {
        for web_result in results {
            let (website, match_result) = web_result?;
            insta::assert_yaml_snapshot!(website, match_result);
        }
        Ok(())
    })
}

#[test]
fn selector_map_correct() -> Result<()> {
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");

    let results1 = mach_6::do_all_websites(&websites, Algorithm::Naive)?;
    let results2 = mach_6::do_all_websites(&websites, Algorithm::WithSelectorMap)?;

    for (result1, result2) in results1.zip(results2) {
        let website1 = result1?;
        let website2 = result2?;
        assert_eq!(website1, website2);
    }
    Ok(())
}