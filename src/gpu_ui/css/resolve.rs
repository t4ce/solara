use style::color::AbsoluteColor;
use style::properties::{PropertyDeclaration, PropertyDeclarationBlock, parse_style_attribute};
use style::stylesheets::{CssRuleType, UrlExtraData};
use style::values::specified::border::BorderSideWidth;
use style::values::specified::color::ColorPropertyValue;
use style::values::specified::font::{FontSize as SpecifiedFontSize, FontSizeKeyword};
use style::values::specified::{Color as SpecifiedColor, LengthPercentage, NoCalcLength};
use style_traits::ToCss;
use style_traits::values::CssWriter;
use url::Url;

#[derive(Clone, Debug, Default)]
pub struct ResolvedStyle {
    pub color: Option<[f32; 4]>,
    pub background_color: Option<[f32; 4]>,
    pub font_size: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<[f32; 4]>,
}

impl ResolvedStyle {
    pub fn merge(&mut self, other: &ResolvedStyle) {
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.background_color.is_some() {
            self.background_color = other.background_color;
        }
        if other.font_size.is_some() {
            self.font_size = other.font_size;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
    }

    pub fn from_declarations(css: &str) -> Self {
        Self::from_block(&parse_declarations(css))
    }

    pub fn from_block(block: &PropertyDeclarationBlock) -> Self {
        let mut out = ResolvedStyle::default();
        for (decl, importance) in block.declaration_importance_iter() {
            if importance.important() {
                apply_declaration(&mut out, decl);
            }
        }
        for (decl, importance) in block.declaration_importance_iter() {
            if !importance.important() {
                apply_declaration(&mut out, decl);
            }
        }
        out
    }
}

pub fn parse_declarations(css: &str) -> PropertyDeclarationBlock {
    let url = Url::parse("https://solara.local/").expect("valid base url");
    let url_data = UrlExtraData::from(url);
    parse_style_attribute(
        css,
        &url_data,
        None,
        style::context::QuirksMode::NoQuirks,
        CssRuleType::Style,
    )
}

fn apply_declaration(out: &mut ResolvedStyle, decl: &PropertyDeclaration) {
    match decl {
        PropertyDeclaration::Color(value) => {
            out.color = color_value_to_rgba(value);
        }
        PropertyDeclaration::BackgroundColor(value) => {
            out.background_color = specified_color_to_rgba(value);
        }
        PropertyDeclaration::FontSize(size) => {
            out.font_size = specified_font_size_px(size);
        }
        PropertyDeclaration::BorderTopWidth(width)
        | PropertyDeclaration::BorderRightWidth(width)
        | PropertyDeclaration::BorderBottomWidth(width)
        | PropertyDeclaration::BorderLeftWidth(width) => {
            out.border_width = border_side_width_px(width);
        }
        PropertyDeclaration::BorderTopColor(color)
        | PropertyDeclaration::BorderRightColor(color)
        | PropertyDeclaration::BorderBottomColor(color)
        | PropertyDeclaration::BorderLeftColor(color) => {
            out.border_color = specified_color_to_rgba(color);
        }
        _ => {}
    }
}

fn color_value_to_rgba(value: &ColorPropertyValue) -> Option<[f32; 4]> {
    specified_color_to_rgba(&value.0)
}

fn specified_color_to_rgba(value: &SpecifiedColor) -> Option<[f32; 4]> {
    let absolute = value.resolve_to_absolute().ok()?;
    Some(abs_color_to_rgba(absolute))
}

fn abs_color_to_rgba(color: AbsoluteColor) -> [f32; 4] {
    let srgb = color.into_srgb_legacy();
    let comps = srgb.raw_components();
    [comps[0], comps[1], comps[2], srgb.alpha]
}

fn specified_font_size_px(size: &SpecifiedFontSize) -> Option<f32> {
    match size {
        SpecifiedFontSize::Length(LengthPercentage::Length(len)) => no_calc_length_px(len),
        SpecifiedFontSize::Keyword(info) => Some(keyword_font_size_px(info.kw)),
        SpecifiedFontSize::Smaller => Some(12.0),
        SpecifiedFontSize::Larger => Some(18.0),
        _ => None,
    }
}

fn keyword_font_size_px(keyword: FontSizeKeyword) -> f32 {
    match keyword {
        FontSizeKeyword::XXSmall => 9.0,
        FontSizeKeyword::XSmall => 10.0,
        FontSizeKeyword::Small => 13.0,
        FontSizeKeyword::Medium => 16.0,
        FontSizeKeyword::Large => 18.0,
        FontSizeKeyword::XLarge => 24.0,
        FontSizeKeyword::XXLarge => 32.0,
        FontSizeKeyword::XXXLarge => 48.0,
        FontSizeKeyword::None => 16.0,
    }
}

fn border_side_width_px(width: &BorderSideWidth) -> Option<f32> {
    let mut css = String::new();
    width.to_css(&mut CssWriter::new(&mut css)).ok()?;
    match css.as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        s if s.ends_with("px") => s.trim_end_matches("px").parse().ok(),
        _ => None,
    }
}

fn no_calc_length_px(len: &NoCalcLength) -> Option<f32> {
    match len {
        NoCalcLength::Absolute(abs) => Some(abs.to_px()),
        _ => None,
    }
}
