/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::io::{self, ErrorKind};
use std::env;
use std::fs;
use tempfile::NamedTempFile;
use mach_6::{self, Result};

#[test]
fn ensures_websites_is_dir() -> io::Result<()> {
    // create a file
    let websites_file = NamedTempFile::new_in(env::current_dir()?)?;
    match mach_6::do_all_websites(websites_file.path()) {
        Err(e) if e.kind() == ErrorKind::NotADirectory => Ok(()),
        Err(e) => panic!("expected NotADirectory error, got {e}"),
        Ok(_) => panic!("expected NotADirectory error, got Ok"),
    }
}

#[test]
fn ensures_each_website_is_dir() -> Result<()> {
    let websites_dir = tempfile::tempdir()?;
    let websites_path = websites_dir.path();
    for i in 0..10 {
        let website_path = websites_path.join(format!("{i}"));
        if i == 5 {
            fs::File::create_new(website_path)?;
        } else {
            fs::create_dir(website_path)?;
        }
    }
    let mut res = mach_6::do_all_websites(websites_path)?;
    assert!(res.nth(5).unwrap().is_err_and(|e| e.is_io_and(|e| e.kind() == ErrorKind::NotADirectory)));
    Ok(())
}