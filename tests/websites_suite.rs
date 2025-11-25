/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::PathBuf;
use mach_6::{Algorithm, Result, SetDocumentMatches};
use insta;

#[test]
fn does_all_websites() -> Result<()> {
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let result: Result<Vec<SetDocumentMatches>> = mach_6::do_all_websites(&websites, Algorithm::Naive)?.collect();
    let result = result?;
    insta::assert_yaml_snapshot!(result);
    Ok(())
}

#[test]
fn selector_map_correct() -> Result<()> {
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");

    let result1: Result<Vec<SetDocumentMatches>> = mach_6::do_all_websites(&websites, Algorithm::Naive)?.collect();
    let result1 = result1?;

    let result2: Result<Vec<SetDocumentMatches>> = mach_6::do_all_websites(&websites, Algorithm::WithSelectorMap)?.collect();
    let result2 = result2?;
    assert_eq!(result1, result2);
    Ok(())
}