/// Contains structures/functions which are insignificant other than to interface with Stylo
use selectors::matching;
use style::media_queries::Device;
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::queries::values::PrefersColorScheme;
use style::style_resolver::{PrimaryStyle, ResolvedStyle};
use style::servo::media_queries::FontMetricsProvider;
use style::values::computed::{CSSPixelLength, font::GenericFontFamily, font::QueryFontMetricsFlags, Length};
use style::Atom;
use style::context::{RegisteredSpeculativePainter, RegisteredSpeculativePainters};

#[derive(Debug)]
struct TestFontMetricsProvider;

impl FontMetricsProvider for TestFontMetricsProvider {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &Font,
        _base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        style::font_metrics::FontMetrics {
            x_height: Some(CSSPixelLength::new(1.0)),
            zero_advance_measure: Some(CSSPixelLength::new(1.0)),
            cap_height: Some(CSSPixelLength::new(1.0)),
            ic_width: Some(CSSPixelLength::new(1.0)),
            ascent: CSSPixelLength::new(1.0),
            script_percent_scale_down: None,
            script_script_percent_scale_down: None,
        } // TODO: Idk
    }

    fn base_size_for_generic(&self, _generic: GenericFontFamily) -> Length {
        CSSPixelLength::new(1.0)
    }
}

pub fn mock_device() -> Device {
    let default_font = Font::initial_values();
    Device::new(
        MediaType::screen(),
        matching::QuirksMode::NoQuirks,
        euclid::Size2D::new(1200.0, 800.0),
        euclid::Scale::new(1.0),
        Box::new(TestFontMetricsProvider),
        ComputedValues::initial_values_with_font_override(default_font),
        PrefersColorScheme::Light,
    )
}

pub fn default_style() -> PrimaryStyle {
    let default_font = Font::initial_values();
    let style = ComputedValues::initial_values_with_font_override(default_font);
    PrimaryStyle {
        style: ResolvedStyle(style),
        reused_via_rule_node: false,
        may_have_starting_style: false,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MyRegisteredSpeculativePainters;
impl RegisteredSpeculativePainters for MyRegisteredSpeculativePainters {
    /// Look up a speculative painter
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        panic!("Oh, WOW. We actually used RegisteredSpeculativePainters and I have to do something.");
    }
}
