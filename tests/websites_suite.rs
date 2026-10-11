/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::{fmt::Write as _, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use html5ever::{LocalName, QualName, ns};
use mach_6::{Algorithm, match_selectors, parse::{ParsedWebsite, get_document_and_selectors, get_websites_dirs, websites_path}, result::{Error, IntoResultExt, Result}, structs::{borrowed::DocumentMatches, element_id, owned::OwnedDocumentMatches, ser::{DebugSerDocumentMatches, SerDocumentMatches}, set::SetDocumentMatches}};
use insta;
use rayon::prelude::*;
use scraper::{ElementRef, Html, Node};
use selectors::matching::TimingStats;
use style::Atom;
use test_log::test;

fn website_paths_for_tests() -> Result<Vec<Result<PathBuf>>> {
    let websites = websites_path();
    match std::env::var("MACH6_WEBSITE_FILTER") {
        Ok(filter) => {
            let website = websites.join(&filter);
            if !website.is_dir() {
                return Err(Error::other(format!("MACH6_WEBSITE_FILTER={filter:?} did not resolve to a website directory at {}", website.display())));
            }
            Ok(vec![Ok(website)])
        }
        Err(std::env::VarError::NotPresent) => Ok(get_websites_dirs(&websites)?.collect()),
        Err(std::env::VarError::NotUnicode(filter)) => {
            Err(Error::other(format!("MACH6_WEBSITE_FILTER was not valid unicode: {filter:?}")))
        }
    }
}

fn annotated_html(document: &Html) -> String {
    let mut debug_document = Html::parse_document(&document.html());
    let attr_name = QualName::new(None, ns!(), LocalName::from("data-mach6-id"));
    let element_ids: Vec<_> = debug_document
        .tree
        .nodes()
        .filter_map(ElementRef::wrap)
        .map(|element| (element.id(), element_id(element)))
        .collect();
    for (node_id, id) in element_ids {
        let mut node = debug_document.tree.get_mut(node_id).expect("node should still exist");
        let Node::Element(element) = node.value() else {
            continue;
        };
        element.attrs.push((attr_name.clone(), Atom::from(id.to_string())));
    }
    debug_document.html()
}

fn compare_with_naive(
    website_name: &str,
    input: &ParsedWebsite,
    naive_result: &DocumentMatches,
    ser_naive_result: &SerDocumentMatches,
    debug_naive_result: &DebugSerDocumentMatches,
    algorithm: Algorithm,
    equality_failures_alg_path: &Path
) -> Result<bool> {
    let (_name, result, _stats) = mach_6::do_website(input, algorithm, Some(naive_result));
    let ser_result = SerDocumentMatches::from(&result);
    if ser_result != *ser_naive_result {
        let website_folder = equality_failures_alg_path.join(website_name);
        std::fs::create_dir_all(&website_folder).into_result(Some(website_folder.clone()))?;
        let annotated_html_path = website_folder.join(format!("{website_name}.debug.html"));
        std::fs::write(&annotated_html_path, annotated_html(input.document()))
            .into_result(Some(annotated_html_path))?;
        for (algorithm, ser_result, debug_result) in [(Algorithm::Naive, ser_naive_result, debug_naive_result), (algorithm, &ser_result, &DebugSerDocumentMatches::from(&result))] {
            let yaml_path = website_folder.join(format!("{website_name}.{algorithm}.yaml"));
            let debug_yaml_path = website_folder.join(format!("{website_name}.{algorithm}.debug.yaml"));
            let f = std::fs::File::create(&yaml_path).into_result(Some(yaml_path))?;
            let f_debug = std::fs::File::create(&debug_yaml_path).into_result(Some(debug_yaml_path))?;
            serde_yml::to_writer(f, &ser_result).unwrap(); // TODO: make a mach_6::Result and propagate instead of unwrapping
            serde_yml::to_writer(f_debug, debug_result).unwrap();
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

    let website_paths = website_paths_for_tests()?;
    let algorithms = [
        Algorithm::WithStyleSharing,
        Algorithm::WithFailCaches,
        Algorithm::WithBlessList,
        Algorithm::WithIsConversion,
        Algorithm::WithDistribution,
        Algorithm::Mach7,
    ]
    .map(|alg| (alg, AtomicBool::new(false)));
    // start with a clean slate
    for (algorithm, _) in &algorithms {
        let path = equality_failures_alg(*algorithm);
        match std::fs::remove_dir_all(&path) {
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => (),
            other => other.into_result(Some(path.clone()))?
        };
        std::fs::create_dir_all(&path).into_result(Some(path))?;
    }
    let naive_flag = AtomicBool::new(false);
    let _: Vec<()> = website_paths
        .into_par_iter()
        .map(|path| {
            // 1.1. Compute naive result
            let Some(website) = get_document_and_selectors(&path?)? else { return Ok(()); };
            let naive_selectors = website
                .get_matcher(mach_6::Optimizations::from_none())
                .get_selectors();
            let naive_result = match_selectors(website.document(), &naive_selectors);
            let set_naive_result = SetDocumentMatches::from(OwnedDocumentMatches::from(&naive_result));
            let ser_naive_result = SerDocumentMatches::from(&set_naive_result);
            let debug_naive_result = DebugSerDocumentMatches::from(&set_naive_result);
            // 1.2. Check naive result with insta
            let naive_ok = std::panic::catch_unwind(|| {
                insta::with_settings!({ snapshot_path => websites_path().join("snapshots")}, {
                    insta::assert_yaml_snapshot!(website.name.as_str(), ser_naive_result);
                });
            }).is_ok();
            if !naive_ok {
                naive_flag.store(true, Ordering::Relaxed);
            }
            // 2. Check algorithms against naive result
            for (algorithm, flag) in &algorithms {
                // Here's the bit that does the actual work
                if !compare_with_naive(
                    &website.name,
                    &website,
                    &naive_result,
                    &ser_naive_result,
                    &debug_naive_result,
                    *algorithm,
                    &equality_failures_alg(*algorithm)
                )? {
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
    let mut msg = String::new();
    let mut should_panic = false;
    if naive_flag.into_inner() {
        should_panic = true;
        writeln!(&mut msg, "Some insta snapshots have changed. See {} for details.", websites_path().display()).unwrap();
    }
    if algorithms.into_iter().any(|(_, flag)| flag) {
        should_panic = true;
        writeln!(&mut msg, "Some algorithms are incorrect. See {} for details.", equality_failures_rel.display()).unwrap();
    }
    if should_panic {
        panic!("{}", msg);
    }
    Ok(())
}

#[test]
fn statistics_dont_change() -> Result<()> {
    let website_paths = website_paths_for_tests()?;
    let _: Vec<_> = website_paths
        .into_par_iter()
        .map(|path| {
            let Some(website) = get_document_and_selectors(&path?)? else { return Ok(()); };
            for algorithm in [
                Algorithm::WithStyleSharing,
                Algorithm::WithFailCaches,
                Algorithm::WithBlessList,
                Algorithm::WithIsConversion,
                Algorithm::WithDistribution,
                /* Algorithm::Mach7 just produces default statistics*/
            ] {
                let (_, _, mut stats1) = mach_6::do_website(&website, algorithm, None);
                let (_, _, mut stats2) = mach_6::do_website(&website, algorithm, None);
                // Ignore timing info, which we expect to change between runs.
                stats1.times = TimingStats::default();
                stats2.times = TimingStats::default();
                assert_eq!(stats1, stats2);
            }
            Ok(())
        })
        .collect::<Result<_>>()?;
    Ok(())
}

#[test]
fn fail_cache_hits_preserve_matching_for_shared_prefixes() {
    let directory = tempfile::tempdir().unwrap();
    let repeated_selector = std::iter::repeat("[data-present] .d")
        .take(512)
        .collect::<Vec<_>>()
        .join(", ");
    let fixture = r#"<!doctype html>
            <style>
                .a .b { color: red }
                .a .c { color: blue }
                [data-present] .d { color: green }
                __REPEATED_SELECTOR__ { color: lime }
                [data-present] .e { color: black }
            </style>
            <div class="a"><div class="between"><span class="b c"></span></div></div>
            <div class="x y"><span class="d e"></span></div>
            <div><span class="d e"></span></div>"#
        .replace("__REPEATED_SELECTOR__", &repeated_selector);
    std::fs::write(
        directory.path().join("fixture.html"),
        fixture,
    )
    .unwrap();
    let website = get_document_and_selectors(directory.path())
        .unwrap()
        .expect("the regression fixture should parse");

    let baseline = mach_6::do_website(&website, Algorithm::Naive, None).1;
    let (_, with_fail_caches, stats) =
        mach_6::do_website(&website, Algorithm::WithFailCaches, None);
    assert!(stats.counts.fail_cache_rejects > 0, "fixture must exercise a cache hit");

    assert_eq!(
        SerDocumentMatches::from(&baseline),
        SerDocumentMatches::from(&with_fail_caches),
        "cache hits must not hide a matching ancestor or change the matched selector set",
    );
}

#[test]
fn fail_cache_hits_preserve_sibling_and_ancestor_backtracking() {
    let directory = tempfile::tempdir().unwrap();
    let repeated_selector = vec![".a + .b ~ .c .d"; 64].join(", ");
    let fixture = r#"<style>
.a + .b > .x { color: red }
.a + .b ~ .c .d { color: blue }
.a + .b ~ .c > .e { color: green }
.a + .b ~ .c .f { color: lime }
__REPEATED_SELECTOR__ { color: blue }
</style>
<div><i class="a"></i><i class="b"><i class="x"></i></i><i class="b"><i class="x"></i></i><i class="c"><i class="d"></i></i></div>
<div><i class="b"></i><i class="c"><i class="d first-d"></i><i class="d second-d"></i></i></div>
<div><i class="a"></i><i class="b"></i><i class="c"><i class="b"></i><i class="c"><i class="e first-e"></i><i class="e second-e"></i><i class="f"></i></i></i></div>"#
        .replace("__REPEATED_SELECTOR__", &repeated_selector);
    std::fs::write(directory.path().join("fixture.html"), fixture).unwrap();
    let website = get_document_and_selectors(directory.path())
        .unwrap()
        .expect("fixture should parse");

    let baseline = mach_6::do_website(&website, Algorithm::Naive, None).1;
    let (_, with_fail_caches, stats) =
        mach_6::do_website(&website, Algorithm::WithFailCaches, None);
    let baseline = SerDocumentMatches::from(&baseline);
    assert!(stats.counts.fail_cache_rejects > 0, "{stats:?}");
    let matches = |class, selector| baseline.0.values().any(|element| {
        element.html.contains(&format!("class=\"{class}\"")) && element.selectors.contains(selector)
    });
    assert!(matches("d", ".a + .b ~ .c .d"));
    assert!(matches("f", ".a + .b ~ .c .f"));
    assert!(!matches("e", ".a + .b ~ .c > .e"));

    assert_eq!(baseline, SerDocumentMatches::from(&with_fail_caches));
}
