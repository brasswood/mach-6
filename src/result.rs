/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use crate::parse::HtmlFile;
use std::path::PathBuf;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub struct Error {
    pub path: Option<PathBuf>,
    pub error: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    Io(io::Error),
    MultipleHtmlFiles(Vec<HtmlFile>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error {
            ErrorKind::Io(io) => {
                write!(f, "io error: {io}")?;
                if let Some(path) = &self.path {
                    write!(f, " path: {}", path.display())?;
                }
                Ok(())
            },
            ErrorKind::MultipleHtmlFiles(v) => {
                writeln!(f, "website {} has more than one html file:", self.path.as_ref().unwrap().display())?;
                for HtmlFile(h) in v {
                    writeln!(f, "{}", h.display())?;
                }
                Ok(())
            }
        }
    }
}

impl Error {
    pub fn is_io_and(&self, f: impl FnOnce(&io::Error) -> bool) -> bool {
        match &self.error {
            ErrorKind::Io(e) => f(&e),
            _ => false
        }
    }

    pub fn is_html_and(&self, f: impl FnOnce(&Vec<HtmlFile>) -> bool) -> bool {
        match &self.error {
            ErrorKind::MultipleHtmlFiles(v) => f(&v),
            _ => false,
        }
    }
}

pub trait IntoErrorExt<T> {
    fn into_error(self, path: Option<PathBuf>) -> Error;
}

impl<T> IntoErrorExt<T> for io::Error {
    fn into_error(self, path: Option<PathBuf>) -> Error {
        Error {
            path,
            error: ErrorKind::Io(self),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait IntoResultExt<T> {
    fn into_result(self, path: Option<PathBuf>) -> Result<T>;
}

impl<T> IntoResultExt<T> for io::Result<T> {
    fn into_result(self, path: Option<PathBuf>) -> Result<T> {
        self.map_err(|e| <io::Error as IntoErrorExt<T>>::into_error(e, path))
    }
}
