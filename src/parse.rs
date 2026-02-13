/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use crate::Selector;
use crate::result::{Error, ErrorKind, IntoResultExt, Result};
use log::warn;
use selectors::parser::{Component, RelativeSelector};
use selectors::visitor::SelectorVisitor;
use std::ffi::OsStr;
use std::fs::{self, DirEntry};
use std::fs::ReadDir;
use std::io;
use std::path::{Path, PathBuf};
use scraper::Html;
use serde::Serialize;

mod cssparser;

pub fn websites_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites")
}

pub fn get_all_documents_and_selectors(websites_path: &Path) -> Result<impl Iterator<Item = Result<(String, Html, Vec<Selector>)>>> {
    let websites_dir = fs::read_dir(&websites_path).into_result(Some(websites_path.to_path_buf()))?; 
    let websites = get_websites_dirs(websites_dir);
    Ok(
        websites.filter_map(|r|
            r.into_result(Some(websites_path.to_path_buf()))
            .and_then(|path|
                get_document_and_selectors(&path)
            ).transpose()
        )
    )
}

pub fn get_document_and_selectors(
    website_path: &Path
) -> Result<Option<(String, Html, Vec<Selector>)>> {
    if !website_path.is_dir() {
        warn!("ignoring {} because it is not a directory", website_path.display());
        return Ok(None);
    }
    let document = match parse_website(website_path) {
        Ok(Some(html)) => html,
        Ok(None) => return Ok(None),
        Err(e) => return Err(e),
    };
    let style_tag_selector = scraper::Selector::parse("style").unwrap();
    let style_tags = document.select(&style_tag_selector);
    let selectors_from_style_tags = style_tags.filter_map(|elt| {
        match parse_stylesheet(&elt.inner_html()) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!("error parsing a style tag from website {}: {}. Skipping.", website_path.display(), e);
                None
            }
        }
    }).flatten();
    let stylesheets: Vec<CssFile> = get_stylesheet_paths(&document);
    let selectors_from_stylesheets = stylesheets.into_iter()
        .filter_map(|f| {
            match parse_css_file(&website_path, &f) {
                Ok(v) => Some(v),
                Err(e) => {
                    warn!("error parsing CSS file {}: {}. Skipping.", f.0.display(), e);
                    None
                },
            }
        })
        .flatten();
    let selectors = selectors_from_style_tags.chain(selectors_from_stylesheets).collect();
    let website_name = website_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap()
        .to_owned();
    Ok(Some((website_name, document, selectors)))
}

pub fn get_websites_dirs(websites: ReadDir) -> impl Iterator<Item = io::Result<PathBuf>> {
    websites.map(|website| {
        website.map(|d| d.path())
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
            warn!("ignoring {}, no html file found", website.display());
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
            warn!("Found no href attribute in link element: {}. Skipping.", elt.html());
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
        .filter(|selector| !selector_has_pseudo_class(selector))
        .collect();
    Ok(res)
}

fn selector_has_pseudo_class(selector: &Selector) -> bool {
    struct Visitor {
        found: bool,
    }

    impl SelectorVisitor for Visitor {
        type Impl = style::selector_parser::SelectorImpl;

        fn visit_simple_selector(&mut self, component: &Component<Self::Impl>) -> bool {
            use Component::*;
            if matches!(
                component,
                Negation(..)
                    | Root
                    | Empty
                    | Scope
                    | ImplicitScope
                    | ParentSelector
                    | Nth(..)
                    | NthOf(..)
                    | NonTSPseudoClass(..)
                    | Host(..)
                    | Where(..)
                    | Is(..)
                    | Has(..)
            ) {
                self.found = true;
                return false;
            }
            true
        }

        fn visit_relative_selector_list(
            &mut self,
            list: &[RelativeSelector<Self::Impl>],
        ) -> bool {
            for relative in list {
                if !relative.selector.visit(self) {
                    return false;
                }
            }
            true
        }
    }

    let mut visitor = Visitor { found: false };
    selector.visit(&mut visitor);
    visitor.found
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};
    use crate::result::IntoResultExt;
    use crate::parse::{CssFile, get_main_html, get_stylesheet_paths, parse_main_html};
    use test_log::test;

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
