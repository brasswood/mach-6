/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::{fmt::Write as _, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use clap::Parser;
use html5ever::{LocalName, QualName, ns};
use mach_6::{Optimizations, match_selectors, parse::{ParsedWebsite, get_document_and_selectors, get_websites_dirs, websites_path}, result::{Error, IntoResultExt, Result}, structs::{element_id, owned::OwnedDocumentMatches, ser::{DebugSerDocumentMatches, SerDocumentMatches}, set::SetDocumentMatches}};
use insta;
use rayon::prelude::*;
use scraper::{ElementRef, Html, Node};
use selectors::matching::TimingStats;
use style::Atom;

#[derive(Debug, Parser)]
struct Args {
    /// JSON files describing the optimizations to test
    #[arg(long = "profile", value_name = "FILE", action = clap::ArgAction::Append)]
    profiles: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args.profiles) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(profile_paths: Vec<PathBuf>) -> Result<()> {
    let profiles = profile_paths
        .iter()
        .map(|path| mach_6::load_optimizations(path))
        .collect::<Result<Vec<_>>>()?;
    all_profiles_correct(&profiles)?;
    statistics_dont_change(&profiles)
}

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
    ser_naive_result: &SerDocumentMatches,
    debug_naive_result: &DebugSerDocumentMatches,
    profile_id: usize,
    optimizations: Optimizations,
    equality_failures_profile_path: &Path
) -> Result<bool> {
    let (_name, result, _stats) = mach_6::do_website(input, optimizations);
    let ser_result = SerDocumentMatches::from(&result);
    if ser_result != *ser_naive_result {
        let website_folder = equality_failures_profile_path.join(website_name);
        std::fs::create_dir_all(&website_folder).into_result(Some(website_folder.clone()))?;
        let annotated_html_path = website_folder.join(format!("{website_name}.debug.html"));
        std::fs::write(&annotated_html_path, annotated_html(input.document()))
            .into_result(Some(annotated_html_path))?;
        for (label, ser_result, debug_result) in [("naive", ser_naive_result, debug_naive_result), (&format!("profile-{profile_id}"), &ser_result, &DebugSerDocumentMatches::from(&result))] {
            let yaml_path = website_folder.join(format!("{website_name}.{label}.yaml"));
            let debug_yaml_path = website_folder.join(format!("{website_name}.{label}.debug.yaml"));
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

fn all_profiles_correct(profiles: &[Optimizations]) -> Result<()> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let equality_failures_rel = PathBuf::from("tests/equality_failures");
    let equality_failures_profile = |profile_id: usize| -> PathBuf {
        workspace
            .join(&equality_failures_rel)
            .join(format!("profile-{profile_id}"))
    };

    let website_paths = website_paths_for_tests()?;
    let profile_flags: Vec<_> = profiles.iter().map(|_| AtomicBool::new(false)).collect();
    // start with a clean slate
    for profile_id in 0..profiles.len() {
        let path = equality_failures_profile(profile_id);
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
                .get_matcher(Optimizations::default())
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
            // 2. Check profiles against the naive result
            for (profile_id, (optimizations, flag)) in profiles.iter().zip(&profile_flags).enumerate() {
                if !compare_with_naive(
                    &website.name,
                    &website,
                    &ser_naive_result,
                    &debug_naive_result,
                    profile_id,
                    *optimizations,
                    &equality_failures_profile(profile_id),
                )? {
                    flag.store(true, Ordering::Relaxed);
                }
            }
            Ok(())
        })
        .collect::<Result<_>>()?;
    // clean up, leaving only failures
    for (profile_id, flag) in profile_flags.iter().enumerate() {
        if !flag.load(Ordering::Relaxed) {
            let path = equality_failures_profile(profile_id);
            std::fs::remove_dir(&path).into_result(Some(path))?;
        }
    }
    let mut msg = String::new();
    let mut should_panic = false;
    if naive_flag.into_inner() {
        should_panic = true;
        writeln!(&mut msg, "Some insta snapshots have changed. See {} for details.", websites_path().display()).unwrap();
    }
    if profile_flags.iter().any(|flag| flag.load(Ordering::Relaxed)) {
        should_panic = true;
        writeln!(&mut msg, "Some profiles are incorrect. See {} for details.", equality_failures_rel.display()).unwrap();
    }
    if should_panic {
        panic!("{}", msg);
    }
    Ok(())
}

fn statistics_dont_change(profiles: &[Optimizations]) -> Result<()> {
    let website_paths = website_paths_for_tests()?;
    let _: Vec<_> = website_paths
        .into_par_iter()
        .map(|path| {
            let Some(website) = get_document_and_selectors(&path?)? else { return Ok(()); };
            for optimizations in profiles {
                let (_, _, mut stats1) = mach_6::do_website(&website, *optimizations);
                let (_, _, mut stats2) = mach_6::do_website(&website, *optimizations);
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
