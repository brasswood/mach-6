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
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let websites = workspace.join("websites");
    let equality_failures = workspace.join("tests/equality_failures");
    std::fs::create_dir_all(&equality_failures).map_err(|e| mach_6::Error::with_io_error(e, Some(equality_failures.clone())))?;

    let results1 = mach_6::do_all_websites(&websites, Algorithm::Naive)?;
    let results2 = mach_6::do_all_websites(&websites, Algorithm::WithSelectorMap)?;

    for (result1, result2) in results1.zip(results2) {
        let website1 = result1?;
        let website2 = result2?;
        if website1 != website2 {
            for (algorithm, website) in [("Naive", website1), ("SelectorMap", website2)] {
                let website_folder = equality_failures.join(&website.0);
                std::fs::create_dir_all(&website_folder).map_err(|e| mach_6::Error::with_io_error(e, Some(website_folder.clone())))?;
                let yaml_path = website_folder.join(format!("{web}.{alg}.yaml", web=website.0, alg=algorithm));
                let f = std::fs::File::create(&yaml_path);
                let f = f.map_err(|e| mach_6::Error::with_io_error(e, Some(yaml_path)))?;
                serde_yml::to_writer(f, &website.1).unwrap(); // I don't wanna mess with it
            }
            panic!();
        }
    }
    Ok(())
}