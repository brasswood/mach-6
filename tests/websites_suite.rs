/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::PathBuf;
use mach_6::{Algorithm, result::{IntoResultExt, Result}};
use insta;
use test_log::test;

#[test]
fn does_all_websites() -> Result<()> {
    let websites_path = mach_6::parse::websites_path();
    let results = mach_6::do_all_websites(&websites_path, Algorithm::Naive)?;
    insta::with_settings!({ snapshot_path => websites_path.join("snapshots")}, {
        for web_result in results {
            let (website, match_result, _stats) = web_result?;
            insta::assert_yaml_snapshot!(website, match_result);
        }
        Ok(())
    })
}

fn compare_with_naive(algorithm: Algorithm) -> Result<bool> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let websites = workspace.join("websites");
    let equality_failures_alg = workspace.join(format!("tests/equality_failures/{algorithm}"));
    let mut failed = false;
    match std::fs::remove_dir_all(&equality_failures_alg) {
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => (),
        other => other.into_result(Some(equality_failures_alg.clone()))?
    };
    std::fs::create_dir_all(&equality_failures_alg).into_result(Some(equality_failures_alg.clone()))?;

    let results1 = mach_6::do_all_websites(&websites, Algorithm::Naive)?;
    let results2 = mach_6::do_all_websites(&websites, algorithm)?;
    for (result1, result2) in results1.zip(results2) {
        let (name1, matches1, _stats) = result1?;
        let (name2, matches2, _stats) = result2?;
        let website1 = (name1, matches1);
        let website2 = (name2, matches2);
        if website1 != website2 {
            for (algorithm, website) in [(Algorithm::Naive, website1), (algorithm, website2)] {
                let website_folder = equality_failures_alg.join(&website.0);
                std::fs::create_dir_all(&website_folder).into_result(Some(website_folder.clone()))?;
                let yaml_path = website_folder.join(format!("{web}.{alg}.yaml", web=website.0, alg=algorithm));
                let f = std::fs::File::create(&yaml_path).into_result(Some(yaml_path))?;
                serde_yml::to_writer(f, &website.1).unwrap(); // TODO: make a mach_6::Result and propagate instead of unwrapping
            }
            failed = true;
        }
    }
    if !failed {
        std::fs::remove_dir(&equality_failures_alg).into_result(Some(equality_failures_alg.clone()))?;
    }
    Ok(!failed)

}

#[test]
fn all_algorithms_correct() -> Result<()> {
    let mut succeeded = true;
    for algorithm in [Algorithm::WithSelectorMap, Algorithm::WithBloomFilter, Algorithm::WithStyleSharing] {
        succeeded &= compare_with_naive(algorithm)?;
    }
    assert!(succeeded);
    Ok(())
}

#[test]
fn statistics_dont_change() -> Result<()> {
    let websites_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    for algorithm in [Algorithm::WithSelectorMap, Algorithm::WithBloomFilter, Algorithm::WithStyleSharing] {
        let results1 = mach_6::do_all_websites(&websites_path, algorithm)?;
        let results2 = mach_6::do_all_websites(&websites_path, algorithm)?;
        for (result1, result2) in results1.zip(results2) {
            let (_, _, stats1) = result1?;
            let (_, _, stats2) = result2?;
            assert_eq!(stats1, stats2);
        }
    }
    Ok(())
}