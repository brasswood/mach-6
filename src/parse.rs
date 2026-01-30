/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use crate::{Result, Selector, IntoResultExt, Error, ErrorKind};
use std::fs::{self, DirEntry};
use std::fs::ReadDir;
use std::io;
use std::path::{Path, PathBuf};
use scraper::Html;
use serde::Serialize;

mod cssparser;

pub fn get_documents_and_selectors(websites_path: &Path) -> Result<impl Iterator<Item = Result<(String, Html, Vec<Selector>)>>> {
    let websites_dir = fs::read_dir(&websites_path).into_result(Some(websites_path.to_path_buf()))?; 
    let websites = get_websites_dirs(websites_dir);
    let documents = websites.filter_map(|r: io::Result<PathBuf>| {
        r.into_result(Some(websites_path.to_path_buf()))
            .and_then(|d: PathBuf| parse_website(&d).map(|html: Option<Html>| html.map(|html| (d, html)))).transpose()
    });
    let documents_selectors = documents.map(|r: Result<(PathBuf, Html)>| {
        r.map(|(base, document): (PathBuf, Html)| {
            let style_tag_selector = scraper::Selector::parse("style").unwrap();
            let style_tags = document.select(&style_tag_selector);
            let selectors_from_style_tags = style_tags.filter_map(|elt| {
                match parse_stylesheet(&elt.inner_html()) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        eprintln!("WARNING: error parsing a style tag from website {}: {}. Skipping.", base.display(), e);
                        None
                    }
                }
            })
            .flatten();

            let stylesheets: Vec<CssFile> = get_stylesheet_paths(&document); // TODO: also get from <style> in main html
            let selectors_from_stylesheets = stylesheets.into_iter()
                .filter_map(|f| {
                    match parse_css_file(&base, &f) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            eprintln!("WARNING: error parsing CSS file {}: {}. Skipping.", f.0.display(), e);
                            None
                        },
                    }
                })
                .flatten();
            let selectors = selectors_from_style_tags.chain(selectors_from_stylesheets).collect();
            (base.file_name().unwrap().to_str().unwrap().to_owned(), document, selectors)
        })
    });
    Ok(documents_selectors)
}

fn get_websites_dirs(websites: ReadDir) -> impl Iterator<Item = io::Result<PathBuf>> {
    websites.filter_map(|website| {
        website.map(|website| {
            let website_path = website.path();
            if website_path.is_dir() {
                Some(website_path)
            } else {
                eprintln!("WARNING: ignoring {} because it is not a directory", website_path.display());
                None
            }
        }).transpose()
    })
}

fn parse_website(website: &Path)-> Result<Option<Html>> {
    let main = get_main_html(website)?;
    main.map(parse_main_html).transpose()
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct HtmlFile(pub PathBuf);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct CssFile(pub PathBuf);

fn get_main_html(website: &Path) -> Result<Option<HtmlFile>> {
    let website_path = Some(website.to_path_buf());
    let files = fs::read_dir(website).into_result(website_path.clone())?;
    let f = |entry: DirEntry| {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("html") {
            Some(HtmlFile(PathBuf::from(path)))
        } else {
            None
        }
    };
    let mut found: Vec<HtmlFile> = files.collect::<io::Result<Vec<_>>>().into_result(website_path)?.into_iter().filter_map(f).collect();
    match found.len() {
        0 => {
            eprintln!("WARNING: ignoring {}, no html file found", website.display());
            Ok(None)
        },
        1 => Ok(Some(found.pop().unwrap())),
        _ => Err(Error{path: Some(website.to_path_buf()), error: ErrorKind::MultipleHtmlFiles(found) })  
    }
}

fn parse_main_html(HtmlFile(website): HtmlFile) -> Result<Html> {
    let contents = fs::read_to_string(&website).into_result(Some(website))?;
    Ok(Html::parse_document(&contents))
}

/// Returns the relative paths of stylesheets referenced by the given document.
fn get_stylesheet_paths(document: &Html) -> Vec<CssFile> {
    let selector = scraper::Selector::parse(r#"link[rel="stylesheet"]"#).unwrap();
    document.select(&selector).filter_map(|elt| {
        let Some(path) = elt.attr("href") else {
            eprintln!("WARNING: Found no href attribute in link element: {}. Skipping.", elt.html());
            return None;
        };
        Some(CssFile(PathBuf::from(path)))
    }).collect()
}

// TODO: returning iterator from these would probably be ideal.
fn parse_css_file(base: &Path, CssFile(stylesheet_path): &CssFile) -> Result<Vec<Selector>> {
    let full_path = base.join(stylesheet_path);
    let css = fs::read_to_string(&full_path).into_result(Some(full_path))?;
    parse_stylesheet(&css)
}

fn parse_stylesheet(css: &str) -> Result<Vec<Selector>> {
    let res = cssparser::get_all_selectors(&css)
        .into_iter()
        .filter_map(|r| {
            r.ok().flatten().map(|sel_list| sel_list.selectors.slice().iter().cloned().collect::<Vec<_>>().into_iter())
        })
        .flatten()
        .collect();
    Ok(res)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};
    use crate::IntoResultExt;
    use crate::parse::{CssFile, get_main_html, get_stylesheet_paths, parse_main_html};

    /// In all of these tests:
    ///   - Err() represents an unexpected error occurring during the test
    ///   - panic represents a test failure


    #[test]
    fn ensures_main_html_exists() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        let is_err = get_main_html(website_path).is_ok_and(|h| h.is_none());
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn ensures_not_multiple_main_html() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        for i in 1..=2 {
            let html_path = website_path.join(format!("{i}.html"));
            fs::File::create_new(&html_path).into_result(Some(html_path))?;
        }
        let is_err = get_main_html(website_path).is_err_and(|e| e.is_html_and(|_| true));
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn ensures_main_html_is_file() -> std::io::Result<()> {
        let website_dir = tempfile::tempdir()?;
        let website_path = website_dir.path();
        fs::create_dir(website_path.join("index.html"))?;
        let main_res = get_main_html(website_path);
        let is_err = matches!(main_res, Ok(None));
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn parses_main_html() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), "<html><body><h1>Hello, World!</h1></body></html>").into_result(Some(website_path.to_path_buf()))?;
        println!("{:?}", website_path);
        let main_html = get_main_html(website_path)?.unwrap();
        parse_main_html(main_html).unwrap();
        Ok(())
    }

    #[test]
    fn gets_stylesheet_paths() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"></head><body><h1>Hello, World!</h1></body></html>"#)
            .into_result(Some(website_path.to_path_buf()))?;
        let main_html = get_main_html(website_path)?.unwrap();
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
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        let index_html_path = website_path.join("index.html");
        fs::write(&index_html_path, r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"><link rel="prerender" href="boogeyman"></head><body><h1>Hello, World!</h1></body></html>"#).into_result(Some(index_html_path))?;
        let main_html = get_main_html(website_path)?.unwrap();
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