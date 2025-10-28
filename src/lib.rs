/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::fs::DirEntry;
use std::fs::ReadDir;
use std::hash::DefaultHasher;
use std::hash::Hasher as _;
use std::path::PathBuf;
use std::io;
use std::fs;
use std::path::Path;
use scraper::Html;
use scraper::Selector;
use std::hash::Hash;
use std::result;
use thiserror::Error;
use serde::Serialize;

pub mod cssparser;

pub fn do_all_websites(websites: &Path) -> Result<impl Iterator<Item = Result<DocumentMatches>>> {
    Ok(get_documents_and_selectors(websites)?
        .map(|r| {
            r.map(|(_, h, s)| match_selectors(&h, s))
        })
    )
}

pub fn get_documents_and_selectors(websites_path: &Path) -> Result<impl Iterator<Item = Result<(String, Html, Vec<Selector>)>>> {
    let websites_dir = fs::read_dir(&websites_path).map_err(|e| Error::with_io_error(e, Some(websites_path.to_path_buf())))?; 
    let websites = get_websites_dirs(websites_dir);
    let documents = websites.map(|r: io::Result<PathBuf>| {
        r.map_err(|e| Error::with_io_error(e, Some(websites_path.to_path_buf())))
            .and_then(|d: PathBuf| parse_website(&d).map(|html: Html| (d, html)))
    });
    let documents_selectors = documents.map(|r: Result<(PathBuf, Html)>| {
        r.map(|(base, document): (PathBuf, Html)| {
            let stylesheets: Vec<CssFile> = get_stylesheet_paths(&document);
            let selectors= stylesheets.into_iter()
                .filter_map(|f| {
                    match parse_stylesheet(&base, &f) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            eprintln!("WARNING: error parsing CSS file {}: {}. Skipping.", f.0.display(), e);
                            None
                        },
                    }
                })
                .flatten()
                .collect();
            (base.file_name().unwrap().to_str().unwrap().to_owned(), document, selectors)
        })
    });
    Ok(documents_selectors)
}

fn get_websites_dirs(websites: ReadDir) -> impl Iterator<Item = io::Result<PathBuf>> {
    websites.map(|website| {
        let website = website?;
        let website_path = website.path();
        if !website_path.is_dir() {
            Err(io::Error::new(io::ErrorKind::NotADirectory, format!("Error: Expected {} to be a directory", website_path.display())))
        } else {
            Ok(website_path)
        }
    })
}

fn parse_website(website: &Path)-> Result<Html> {
    let main = get_main_html(website)?;
    parse_main_html(main)
}

#[derive(Error, Debug)]
pub struct Error {
    pub path: Option<PathBuf>,
    pub error: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    Io(io::Error),
    NotOneHtmlFile(Vec<HtmlFile>),
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
            ErrorKind::NotOneHtmlFile(v) => {
                writeln!(f, "the number of html files for website {} was not one:", self.path.as_ref().unwrap().display())?;
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
            ErrorKind::NotOneHtmlFile(v) => f(&v),
            _ => false,
        }
    }

    pub fn with_io_error(error: io::Error, path: Option<PathBuf>) -> Self {
        Self {
            path,
            error: ErrorKind::Io(error),
        }
    }
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct HtmlFile(PathBuf);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct CssFile(PathBuf);

fn get_main_html(website: &Path) -> Result<HtmlFile> {
    let err_map = |e: io::Error| Error::with_io_error(e, Some(website.to_path_buf()));
    let files = fs::read_dir(website).map_err(err_map)?;
    let f = |entry: DirEntry| {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("html") {
            Some(HtmlFile(PathBuf::from(path)))
        } else {
            None
        }
    };
    let mut found: Vec<HtmlFile> = files.collect::<io::Result<Vec<_>>>().map_err(err_map)?.into_iter().filter_map(f).collect();
    if found.len() == 1 {
        Ok(found.pop().unwrap())
    } else {
        Err(Error{path: Some(website.to_path_buf()), error: ErrorKind::NotOneHtmlFile(found) })  
    }
}

fn parse_main_html(HtmlFile(website): HtmlFile) -> Result<Html> {
    let contents = fs::read_to_string(&website).map_err(|e| Error::with_io_error(e, Some(website)))?;
    Ok(Html::parse_document(&contents))
}

/// Returns the relative paths of stylesheets referenced by the given document.
fn get_stylesheet_paths(document: &Html) -> Vec<CssFile> {
    let selector = Selector::parse(r#"link[rel="stylesheet"]"#).unwrap();
    document.select(&selector).filter_map(|elt| {
        let Some(path) = elt.attr("href") else {
            eprintln!("WARNING: Found no href attribute in link element: {}. Skipping.", elt.html());
            return None;
        };
        Some(CssFile(PathBuf::from(path)))
    }).collect()
}

fn parse_stylesheet(base: &Path, CssFile(stylesheet_path): &CssFile) -> Result<Vec<Selector>> {
    let full_path = base.join(stylesheet_path);
    let css = fs::read_to_string(&full_path).map_err(|e| Error::with_io_error(e, Some(full_path)))?;
    let res = cssparser::get_all_selectors(&css)
        .into_iter()
        .filter_map(|r| r.ok().flatten())
        .collect();
    Ok(res)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Element(ego_tree::NodeId);

impl From<scraper::ElementRef<'_>> for Element {
    fn from(value: scraper::ElementRef) -> Self {
        Self(value.id())
    }
}

impl Serialize for Element {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        serializer.serialize_u64(hasher.finish())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectorMatches {
    selector: Selector,
    matches: Vec<Element>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentMatches(Vec<SelectorMatches>);

pub fn match_selectors<I>(document: &Html, selectors: I) -> DocumentMatches
where
    I: IntoIterator<Item = Selector>,
{
    let ret = selectors.into_iter().map(|selector| {
        let matches = document.select(&selector)
            .map(Element::from)
            .collect();
        SelectorMatches{selector, matches}
    }).collect();
    DocumentMatches(ret)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};
    use crate::{get_main_html, get_stylesheet_paths, parse_main_html, CssFile, Error};

    /// In all of these tests:
    ///   - Err() represents an unexpected error occurring during the test
    ///   - panic represents a test failure


    #[test]
    fn ensures_main_html_exists() -> super::Result<()> {
        let website_dir = tempfile::tempdir().map_err(|e| Error::with_io_error(e, None))?;
        let website_path = website_dir.path();
        let is_err = get_main_html(website_path).is_err_and(|e| {
            e.is_html_and(Vec::is_empty)
        });
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn ensures_main_html_is_file() -> std::io::Result<()> {
        let website_dir = tempfile::tempdir()?;
        let website_path = website_dir.path();
        fs::create_dir(website_path.join("index.html"))?;
        let main_res = get_main_html(website_path);
        let is_err = main_res.is_err_and(|e| {
            e.is_html_and(|v| v.is_empty())
        });
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn parses_main_html() -> super::Result<()> {
        let website_dir = tempfile::tempdir().map_err(|e| Error::with_io_error(e, None))?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), "<html><body><h1>Hello, World!</h1></body></html>")
            .map_err(|e| Error::with_io_error(e, Some(website_path.to_path_buf())))?;
        println!("{:?}", website_path);
        let main_html = get_main_html(website_path)?;
        parse_main_html(main_html).unwrap();
        Ok(())
    }

    #[test]
    fn gets_stylesheet_paths() -> super::Result<()> {
        let website_dir = tempfile::tempdir().map_err(|e| Error::with_io_error(e, None))?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"></head><body><h1>Hello, World!</h1></body></html>"#)
            .map_err(|e| Error::with_io_error(e, Some(website_path.to_path_buf())))?;
        let main_html = get_main_html(website_path)?;
        let document = parse_main_html(main_html)?;
        let mut stylesheets = get_stylesheet_paths(&document);
        let mut expected: Vec<_> = vec!["style1.css", "style2.css"]
            .into_iter()
            .map(|s| CssFile(PathBuf::from(s)))
            .collect();
        stylesheets.sort();
        expected.sort();
        assert_eq!(stylesheets, expected);
        Ok(())
    }

    #[test]
    fn excludes_non_stylesheet_paths() -> super::Result<()> {
        let website_dir = tempfile::tempdir().map_err(|e| Error::with_io_error(e, None))?;
        let website_path = website_dir.path();
        let index_html_path = website_path.join("index.html");
        fs::write(&index_html_path, r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"><link rel="prerender" href="boogeyman"></head><body><h1>Hello, World!</h1></body></html>"#).map_err(|e| Error::with_io_error(e, Some(index_html_path)))?;
        let main_html = get_main_html(website_path)?;
        let document = parse_main_html(main_html)?;
        let mut stylesheets = get_stylesheet_paths(&document);
        let mut expected: Vec<_> = vec!["style1.css", "style2.css"]
            .into_iter()
            .map(|s| CssFile(PathBuf::from(s)))
            .collect();
        stylesheets.sort();
        expected.sort();
        assert_eq!(stylesheets, expected);
        Ok(())
    }
}