use crate::gpu_ui::css::resolve::{parse_declarations, ResolvedStyle};
use crate::gpu_ui::html::HtmlNode;

#[derive(Clone, Debug)]
struct CssRule {
    selectors: Vec<SimpleSelector>,
    declarations: ResolvedStyle,
}

#[derive(Clone, Debug)]
enum SimpleSelector {
    Tag(&'static str),
    Class(String),
    Id(String),
}

#[derive(Clone, Debug, Default)]
pub struct CssEngine {
    rules: Vec<CssRule>,
}

impl CssEngine {
    pub fn from_css(css: &str) -> Self {
        Self {
            rules: parse_stylesheet(css),
        }
    }

    pub fn resolve(&self, node: &HtmlNode) -> ResolvedStyle {
        let mut out = ResolvedStyle::default();
        let tag = node.kind.css_tag_name();

        for rule in &self.rules {
            if rule
                .selectors
                .iter()
                .any(|sel| selector_matches(sel, node, tag))
            {
                out.merge(&rule.declarations);
            }
        }

        if let Some(inline) = &node.style_attr {
            out.merge(&ResolvedStyle::from_declarations(inline));
        }

        out
    }
}

fn selector_matches(sel: &SimpleSelector, node: &HtmlNode, tag: Option<&str>) -> bool {
    match sel {
        SimpleSelector::Tag(name) => tag == Some(name.as_ref()),
        SimpleSelector::Class(class) => node.class.as_deref() == Some(class.as_str()),
        SimpleSelector::Id(id) => node.id_attr.as_deref() == Some(id.as_str()),
    }
}

fn parse_stylesheet(css: &str) -> Vec<CssRule> {
    let mut rules = Vec::new();
    let stripped = strip_comments(css);

    for chunk in stripped.split('}') {
        let Some((selectors, body)) = chunk.split_once('{') else {
            continue;
        };
        let selectors = parse_selectors(selectors.trim());
        if selectors.is_empty() {
            continue;
        }
        let block = parse_declarations(body.trim());
        let declarations = ResolvedStyle::from_block(&block);
        rules.push(CssRule {
            selectors,
            declarations,
        });
    }

    rules
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn parse_selectors(input: &str) -> Vec<SimpleSelector> {
    input
        .split(',')
        .filter_map(parse_one_selector)
        .collect()
}

fn parse_one_selector(input: &str) -> Option<SimpleSelector> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(id) = s.strip_prefix('#') {
        return Some(SimpleSelector::Id(id.trim().to_string()));
    }
    if let Some(class) = s.strip_prefix('.') {
        return Some(SimpleSelector::Class(class.trim().to_string()));
    }
    Some(SimpleSelector::Tag(tag_name_to_static(s)))
}

fn tag_name_to_static(tag: &str) -> &'static str {
    match tag {
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "p" => "p",
        "a" => "a",
        "button" => "button",
        "details" => "details",
        "summary" => "summary",
        "footer" => "footer",
        "input" => "input",
        "label" => "label",
        "div" => "div",
        "form" => "form",
        "hr" => "hr",
        "ol" => "ol",
        "ul" => "ul",
        "li" => "li",
        "table" => "table",
        "dialog" => "dialog",
        "canvas" => "canvas",
        "iframe" => "iframe",
        "img" => "img",
        "svg" => "svg",
        "textarea" => "textarea",
        "select" => "select",
        "progress" => "progress",
        "meter" => "meter",
        "search" => "search",
        "color" => "color",
        _ => "div",
    }
}
