/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::PathBuf;
use mach_6::{DocumentMatches, Result};
use insta;

#[test]
fn does_all_websites() -> Result<()> {
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let result: Result<Vec<DocumentMatches>> = mach_6::do_all_websites(websites)?.collect();
    let result = result?;
    insta::assert_yaml_snapshot!(result);
    Ok(())
}