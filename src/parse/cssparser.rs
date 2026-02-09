/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use cssparser::AtRuleParser;
use cssparser::BasicParseError;
use cssparser::BasicParseErrorKind;
use cssparser::CowRcStr;
use cssparser::ParseError;
use cssparser::ParserState;
use cssparser::StyleSheetParser;
use cssparser::{Parser, ParserInput, QualifiedRuleParser};
use scraper::error::SelectorErrorKind;
use scraper::Selector;

pub type SelectorResult<'i> = Result<Option<Selector>, (ParseError<'i, SelectorErrorKind<'i>>, &'i str)>;

pub fn get_all_selectors(input: &str) -> Vec<SelectorResult<'_>> {
    let mut parser_input = ParserInput::new(input);
    let mut parser = Parser::new(&mut parser_input);
    let mut parser_actions = ParserActions;
    let stylesheet_parser = StyleSheetParser::new(&mut parser, &mut parser_actions);
    stylesheet_parser.collect()
}

struct ParserActions;

fn consume_parser<'i, 't>(parser: &mut Parser<'i, 't>) -> Result<(), ParseError<'i, SelectorErrorKind<'i>>> {
    loop {
        match parser.next() {
            Ok(_) => (),
            Err(BasicParseError { kind:BasicParseErrorKind::EndOfInput, .. }) => { return Ok(()); },
            Err(e) => { return Err(e.into()); },
        }
    }
}

impl<'i> QualifiedRuleParser<'i> for ParserActions {
    type Prelude = Option<Selector>;
    type QualifiedRule = Self::Prelude; // We don't care about the body, we just want selectors
    type Error = SelectorErrorKind<'i>;

    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        // I hate this I hate this I hate this
        let start = input.position();
        consume_parser(input)?;
        let end = input.position();
        let slice = input.slice(start..end);
        // TODO: this can't handle :hover pseudo-class.
        Selector::parse(slice)
            .map(Some)
            .map_err(|e| input.new_custom_error(e))
    }

    fn parse_block<'t>(&mut self, prelude: Self::Prelude, _start: &ParserState, input: &mut Parser<'i, 't>) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        consume_parser(input)?;
        Ok(prelude)
    }
}

impl<'i> AtRuleParser<'i> for ParserActions {
    type Prelude = Option<Selector>;
    type AtRule = Self::Prelude;
    type Error = SelectorErrorKind<'i>;

    fn parse_prelude<'t>(&mut self, _name: CowRcStr<'i>, input: &mut Parser<'i, 't>) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        consume_parser(input)?;
        Ok(None)
    }

    fn rule_without_block(&mut self, _prelude: Self::Prelude, _start: &ParserState) -> Result<Self::AtRule, ()> {
        Ok(None)
    }

    fn parse_block<'t>(&mut self, _prelude: Self::Prelude, _start: &ParserState, input: &mut Parser<'i, 't>) -> Result<Self::AtRule, ParseError<'i, Self::Error>> {
        consume_parser(input)?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;
    use std::fmt::Write as _;
    use std::io;
    use std::fs;
    use std::path::PathBuf;
    use scraper::selector::ToCss as _;
    use super::SelectorResult;
    use test_log::test;

    #[test]
    fn parses_github_rust_scraper_css() -> io::Result<()> {
        let css = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/test_github_rust_scraper.css"))?;
        let rules: Vec<_> = super::get_all_selectors(&css);
        let errs = rules.iter().filter(|r| r.is_err()).count();
        let oks = rules.iter().filter(|r| r.is_ok()).count();
        let mut res = String::new();
        for rule in rules {
            writeln!(&mut res, "{}", MyResult(rule)).unwrap();
        }
        writeln!(&mut res, "Errs: {errs}\nOks: {oks}").unwrap();
        insta::assert_snapshot!(res);
        Ok(())
    }

    struct MyResult<'i>(SelectorResult<'i>);

    impl Display for MyResult<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.0 {
                Ok(None) => write!(f, "OK (no selector)"),
                Ok(Some(selector)) => write!(f, "OK: {}", selector.to_css_string()),
                Err((e, s)) => write!(f, "{e}: {s}"),
            }
        }
    }
}