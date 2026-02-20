/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::{path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use mach_6::{Algorithm, parse::{ParsedWebsite, get_document_and_selectors, get_websites_dirs, websites_path}, result::{IntoResultExt, Result}, structs::ser::SerDocumentMatches};
use insta;
use rayon::prelude::*;
use test_log::test;

#[test]
fn does_all_websites() -> Result<()> {
    let website_paths: Vec<_> = get_websites_dirs(&websites_path())?.collect();
    let _: Vec<_> = website_paths
        .into_par_iter()
        .map(|path| {
            let Some(website) = get_document_and_selectors(&path?)? else { return Ok(()); };
            let (name, match_result, _stats) = mach_6::do_website(&website, Algorithm::Naive);
            insta::with_settings!({ snapshot_path => websites_path().join("snapshots")}, {
                insta::assert_yaml_snapshot!(name, match_result);
            });
            Ok(())
        })
        .collect::<Result<_>>()?;
    Ok(())
}

fn compare_with_naive(input: &ParsedWebsite, algorithm: Algorithm, equality_failures_alg_path: &Path) -> Result<bool> {
    let (name1, matches1, _stats) = mach_6::do_website(input, Algorithm::Naive);
    let (name2, matches2, _stats) = mach_6::do_website(input, algorithm);
    let website1 = (name1, SerDocumentMatches::from(matches1));
    let website2 = (name2, SerDocumentMatches::from(matches2));
    if website1 != website2 {
        for (algorithm, website) in [(Algorithm::Naive, website1), (algorithm, website2)] {
            let website_folder = equality_failures_alg_path.join(&website.0);
            std::fs::create_dir_all(&website_folder).into_result(Some(website_folder.clone()))?;
            let yaml_path = website_folder.join(format!("{web}.{alg}.yaml", web=website.0, alg=algorithm));
            let f = std::fs::File::create(&yaml_path).into_result(Some(yaml_path))?;
            serde_yml::to_writer(f, &website.1).unwrap(); // TODO: make a mach_6::Result and propagate instead of unwrapping
        }
        Ok(false)
    } else {
        Ok(true)
    }
}

#[test]
fn all_algorithms_correct() -> Result<()> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let equality_failures_rel = PathBuf::from("tests/equality_failures");
    let equality_failures_alg = |algorithm: Algorithm| -> PathBuf {
        workspace.join(&equality_failures_rel).join(format!("{algorithm}"))
    };

    let website_paths: Vec<_> = get_websites_dirs(&websites_path())?.collect();
    let algorithms = [Algorithm::WithSelectorMap, Algorithm::WithBloomFilter, Algorithm::WithStyleSharing, Algorithm::Mach7].map(|alg| (alg, AtomicBool::new(false)));
    // start with a clean slate
    for (algorithm, _) in &algorithms {
        let path = equality_failures_alg(*algorithm);
        match std::fs::remove_dir_all(&path) {
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => (),
            other => other.into_result(Some(path.clone()))?
        };
        std::fs::create_dir_all(&path).into_result(Some(path))?;
    }
    let _: Vec<()> = website_paths
        .into_par_iter()
        .map(|path| {
            let Some(website) = get_document_and_selectors(&path?)? else { return Ok(()); };
            for (algorithm, flag) in &algorithms {
                // Here's the bit that does the actual work
                if !compare_with_naive(&website, *algorithm, &equality_failures_alg(*algorithm))? {
                    flag.store(true, Ordering::Relaxed);
                }
            }
            Ok(())
        })
        .collect::<Result<_>>()?;
    // clean up, leaving only failures
    let algorithms = algorithms.map(|(alg, flag)| (alg, flag.into_inner()));
    for (algorithm, flag) in &algorithms {
        if !flag {
            let path = equality_failures_alg(*algorithm);
            std::fs::remove_dir(&path).into_result(Some(path))?;
        }
    }
    if algorithms.into_iter().any(|(_, flag)| flag) {
        panic!("Some algorithms are incorrect. See {} for details.", equality_failures_rel.display());
    }
    Ok(())
}

#[test]
fn statistics_dont_change() -> Result<()> {
    let website_paths: Vec<_> = get_websites_dirs(&websites_path())?.collect();
    let _: Vec<_> = website_paths
        .into_par_iter()
        .map(|path| {
            let Some(website) = get_document_and_selectors(&path?)? else {return Ok(()); };
            for algorithm in [Algorithm::WithSelectorMap, Algorithm::WithBloomFilter, Algorithm::WithStyleSharing, Algorithm::Mach7] {
                let (_, _, mut stats1) = mach_6::do_website(&website, algorithm);
                let (_, _, mut stats2) = mach_6::do_website(&website, algorithm);
                // Ignore timing info, which we expect to change between runs.
                stats1.time_spent_slow_rejecting = None;
                stats2.time_spent_slow_rejecting = None;
                assert_eq!(stats1, stats2);
            }
            Ok(())
        })
        .collect::<Result<_>>()?;
    Ok(())
}