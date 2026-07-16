#![allow(dead_code)]

use crate::gpu_ui::geometry::Rect;

macro_rules! define_html_tags {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// HTML elements defined by the HTML Living Standard.
        #[allow(dead_code)]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum HtmlTag {
            $($variant,)+
            /// A valid custom element or an element unknown to this version of Solara.
            Custom(String),
        }

        #[allow(dead_code)]
        impl HtmlTag {
            pub const STANDARD_NAMES: &'static [&'static str] = &[$($name,)+];

            /// Parses an ASCII case-insensitive HTML tag name.
            ///
            /// Unknown names are retained because HTML permits custom elements and
            /// requires user agents to preserve unknown elements in the DOM.
            pub fn from_name(name: &str) -> Self {
                let normalized = name.to_ascii_lowercase();
                match normalized.as_str() {
                    $($name => Self::$variant,)+
                    _ => Self::Custom(normalized),
                }
            }

            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $name,)+
                    Self::Custom(name) => name,
                }
            }

            pub fn is_metadata(&self) -> bool {
                matches!(
                    self,
                    Self::Base
                        | Self::Link
                        | Self::Meta
                        | Self::Noscript
                        | Self::Script
                        | Self::Style
                        | Self::Template
                        | Self::Title
                )
            }

            pub fn is_void(&self) -> bool {
                matches!(
                    self,
                    Self::Area
                        | Self::Base
                        | Self::Br
                        | Self::Col
                        | Self::Embed
                        | Self::Hr
                        | Self::Img
                        | Self::Input
                        | Self::Link
                        | Self::Meta
                        | Self::Source
                        | Self::Track
                        | Self::Wbr
                )
            }
        }
    };
}

define_html_tags! {
    A => "a",
    Abbr => "abbr",
    Address => "address",
    Area => "area",
    Article => "article",
    Aside => "aside",
    Audio => "audio",
    B => "b",
    Base => "base",
    Bdi => "bdi",
    Bdo => "bdo",
    Blockquote => "blockquote",
    Body => "body",
    Br => "br",
    Button => "button",
    Canvas => "canvas",
    Caption => "caption",
    Cite => "cite",
    Code => "code",
    Col => "col",
    Colgroup => "colgroup",
    Data => "data",
    Datalist => "datalist",
    Dd => "dd",
    Del => "del",
    Details => "details",
    Dfn => "dfn",
    Dialog => "dialog",
    Div => "div",
    Dl => "dl",
    Dt => "dt",
    Em => "em",
    Embed => "embed",
    Fieldset => "fieldset",
    Figcaption => "figcaption",
    Figure => "figure",
    Footer => "footer",
    Form => "form",
    H1 => "h1",
    H2 => "h2",
    H3 => "h3",
    H4 => "h4",
    H5 => "h5",
    H6 => "h6",
    Head => "head",
    Header => "header",
    Hgroup => "hgroup",
    Hr => "hr",
    Html => "html",
    I => "i",
    Iframe => "iframe",
    Img => "img",
    Input => "input",
    Ins => "ins",
    Kbd => "kbd",
    Label => "label",
    Legend => "legend",
    Li => "li",
    Link => "link",
    Main => "main",
    Map => "map",
    Mark => "mark",
    Menu => "menu",
    Meta => "meta",
    Meter => "meter",
    Nav => "nav",
    Noscript => "noscript",
    Object => "object",
    Ol => "ol",
    Optgroup => "optgroup",
    Option => "option",
    Output => "output",
    P => "p",
    Picture => "picture",
    Pre => "pre",
    Progress => "progress",
    Q => "q",
    Rp => "rp",
    Rt => "rt",
    Ruby => "ruby",
    S => "s",
    Samp => "samp",
    Script => "script",
    Search => "search",
    Section => "section",
    Select => "select",
    Selectedcontent => "selectedcontent",
    Slot => "slot",
    Small => "small",
    Source => "source",
    Span => "span",
    Strong => "strong",
    Style => "style",
    Sub => "sub",
    Summary => "summary",
    Sup => "sup",
    Table => "table",
    Tbody => "tbody",
    Td => "td",
    Template => "template",
    Textarea => "textarea",
    Tfoot => "tfoot",
    Th => "th",
    Thead => "thead",
    Time => "time",
    Title => "title",
    Tr => "tr",
    Track => "track",
    U => "u",
    Ul => "ul",
    Var => "var",
    Video => "video",
    Wbr => "wbr",
}

#[derive(Clone, Debug)]
pub enum Inline {
    Text(String),
    Bold(String),
    Italic(String),
}

#[derive(Clone, Debug)]
pub enum InputType {
    Checkbox,
    Text,
    Password,
    Time,
    Date,
    Month,
    Week,
    DateTimeLocal,
    Radio,
}

#[derive(Clone, Debug)]
pub enum ButtonType {
    Submit,
    Reset,
    Button,
}

#[derive(Clone, Debug)]
pub enum SvgChild {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fill: [f32; 4],
        stroke: [f32; 4],
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: [f32; 4],
    },
    Path {
        points: Vec<(f32, f32)>,
        stroke: [f32; 4],
    },
}

#[derive(Clone, Debug)]
pub enum ElementKind {
    /// An HTML element that does not need a specialized renderer yet.
    #[allow(dead_code)]
    Element {
        tag: HtmlTag,
        children: Vec<HtmlNode>,
    },
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    HorizontalRule,
    Link {
        href: String,
        text: String,
    },
    OrderedList {
        items: Vec<String>,
    },
    UnorderedList {
        items: Vec<String>,
    },
    Details {
        summary: String,
        summary_checkbox: bool,
        children: Vec<HtmlNode>,
    },
    Div {
        children: Vec<HtmlNode>,
    },
    Form {
        children: Vec<HtmlNode>,
    },
    Label {
        text: String,
        control: Box<HtmlNode>,
    },
    Input {
        input_type: InputType,
        name: String,
        value: String,
        checked: bool,
        label: Option<String>,
    },
    Select {
        options: Vec<String>,
        selected: usize,
    },
    Textarea {
        name: String,
        value: String,
        rows: u32,
    },
    Button {
        label: String,
        button_type: ButtonType,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Svg {
        width: f32,
        height: f32,
        children: Vec<SvgChild>,
    },
    Canvas {
        width: f32,
        height: f32,
    },
    Iframe {
        width: f32,
        height: f32,
        children: Vec<HtmlNode>,
    },
    Image {
        width: f32,
        height: f32,
        alt: String,
    },
    Dialog {
        children: Vec<HtmlNode>,
        floating: bool,
    },
    Progress {
        value: f32,
        max: f32,
    },
    Meter {
        value: f32,
        label: String,
    },
    Slider {
        value: f32,
        label: String,
    },
    Search {
        value: String,
        width: f32,
    },
    Color,
    Footer {
        text: String,
    },
    PlainText {
        text: String,
    },
}

#[derive(Clone, Debug)]
pub struct HtmlNode {
    pub id: u32,
    pub kind: ElementKind,
    pub bounds: Rect,
    pub open: bool,
    /// CSS class attribute (e.g. `class="plain-demo"`).
    pub class: Option<String>,
    /// CSS id attribute (e.g. `id="main"`).
    pub id_attr: Option<String>,
    /// Inline `style=""` attribute text.
    pub style_attr: Option<String>,
    /// Index into the owning RustQJSDom artifact's computed-style table.
    pub style_ref: Option<usize>,
}

impl HtmlNode {
    pub fn new(id: u32, kind: ElementKind) -> Self {
        Self {
            id,
            kind,
            bounds: Rect::default(),
            open: false,
            class: None,
            id_attr: None,
            style_attr: None,
            style_ref: None,
        }
    }

    #[allow(dead_code)]
    pub fn element(id: u32, tag_name: &str, children: Vec<HtmlNode>) -> Self {
        Self::new(
            id,
            ElementKind::Element {
                tag: HtmlTag::from_name(tag_name),
                children,
            },
        )
    }

    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style_attr = Some(style.into());
        self
    }
}

impl ElementKind {
    pub fn css_tag_name(&self) -> &str {
        match self {
            ElementKind::Element { tag, .. } => tag.as_str(),
            ElementKind::Heading { level, .. } => match level {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                _ => "h6",
            },
            ElementKind::Paragraph { .. } => "p",
            ElementKind::HorizontalRule => "hr",
            ElementKind::Link { .. } => "a",
            ElementKind::OrderedList { .. } => "ol",
            ElementKind::UnorderedList { .. } => "ul",
            ElementKind::Details { .. } => "details",
            ElementKind::Div { .. } => "div",
            ElementKind::Form { .. } => "form",
            ElementKind::Label { .. } => "label",
            ElementKind::Input { .. } => "input",
            ElementKind::Select { .. } => "select",
            ElementKind::Textarea { .. } => "textarea",
            ElementKind::Button { .. } => "button",
            ElementKind::Table { .. } => "table",
            ElementKind::Svg { .. } => "svg",
            ElementKind::Canvas { .. } => "canvas",
            ElementKind::Iframe { .. } => "iframe",
            ElementKind::Image { .. } => "img",
            ElementKind::Dialog { .. } => "dialog",
            ElementKind::Progress { .. } => "progress",
            ElementKind::Meter { .. } => "meter",
            ElementKind::Slider { .. } => "slider",
            ElementKind::Search { .. } => "search",
            ElementKind::Color => "color",
            ElementKind::Footer { .. } => "footer",
            ElementKind::PlainText { .. } => "p",
        }
    }
}

pub fn inline_width(inlines: &[Inline]) -> f32 {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(t) | Inline::Bold(t) | Inline::Italic(t) => {
                t.chars().count() as f32 * crate::gpu_ui::text::char_width_default()
            }
        })
        .sum()
}

pub fn inline_to_string(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(t) | Inline::Bold(t) | Inline::Italic(t) => t.as_str(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ElementKind, HtmlTag};

    #[test]
    fn parses_every_standard_tag_name() {
        for name in HtmlTag::STANDARD_NAMES {
            let tag = HtmlTag::from_name(name);
            assert_eq!(tag.as_str(), *name);
            assert!(!matches!(tag, HtmlTag::Custom(_)));
        }
    }

    #[test]
    fn tag_parsing_is_ascii_case_insensitive() {
        assert_eq!(HtmlTag::from_name("H1"), HtmlTag::H1);
        assert_eq!(HtmlTag::from_name("TextArea"), HtmlTag::Textarea);
    }

    #[test]
    fn preserves_custom_elements() {
        let tag = HtmlTag::from_name("My-Widget");
        assert_eq!(tag, HtmlTag::Custom("my-widget".into()));
        assert_eq!(tag.as_str(), "my-widget");
    }

    #[test]
    fn constructs_generic_elements_from_tag_names() {
        let node = super::HtmlNode::element(1, "ARTICLE", Vec::new());
        assert_eq!(node.kind.css_tag_name(), "article");
    }

    #[test]
    fn classifies_metadata_and_void_elements() {
        assert!(HtmlTag::Meta.is_metadata());
        assert!(HtmlTag::Meta.is_void());
        assert!(HtmlTag::Style.is_metadata());
        assert!(!HtmlTag::Style.is_void());
        assert!(!HtmlTag::Article.is_metadata());
        assert!(!HtmlTag::Article.is_void());
    }

    #[test]
    fn maps_all_heading_levels_to_css_tags() {
        for level in 1..=6 {
            let heading = ElementKind::Heading {
                level,
                text: String::new(),
            };
            assert_eq!(heading.css_tag_name(), format!("h{level}"));
        }
    }
}
