use crate::gpu_ui::geometry::Rect;

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
    Heading { level: u8, text: String },
    Paragraph { inlines: Vec<Inline> },
    HorizontalRule,
    Link { href: String, text: String },
    OrderedList { items: Vec<String> },
    UnorderedList { items: Vec<String> },
    Details {
        summary: String,
        summary_checkbox: bool,
        children: Vec<HtmlNode>,
    },
    Div { children: Vec<HtmlNode> },
    Form { children: Vec<HtmlNode> },
    Label { text: String, control: Box<HtmlNode> },
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
    Canvas { width: f32, height: f32 },
    Iframe { children: Vec<HtmlNode> },
    Image { width: f32, height: f32, alt: String },
    Dialog { children: Vec<HtmlNode>, floating: bool },
    Progress { value: f32, max: f32 },
    Meter { value: f32, label: String },
    Slider { value: f32, label: String },
    Search { value: String, width: f32 },
    Color,
    Footer { text: String },
    PlainText { text: String },
}

#[derive(Clone, Debug)]
pub struct HtmlNode {
    pub id: u32,
    pub kind: ElementKind,
    pub bounds: Rect,
    pub open: bool,
}

impl HtmlNode {
    pub fn new(id: u32, kind: ElementKind) -> Self {
        Self {
            id,
            kind,
            bounds: Rect::default(),
            open: false,
        }
    }
}

pub fn inline_width(inlines: &[Inline]) -> f32 {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(t) | Inline::Bold(t) | Inline::Italic(t) => t.chars().count() as f32 * 8.0,
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
