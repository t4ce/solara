use crate::gpu_ui::css::resolve::{ResolvedStyle, parse_declarations};
use crate::gpu_ui::html::HtmlNode;

#[derive(Clone, Debug)]
struct CssRule {
    selectors: Vec<SimpleSelector>,
    declarations: ResolvedStyle,
}

#[derive(Clone, Debug)]
enum SimpleSelector {
    Tag(String),
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

fn selector_matches(sel: &SimpleSelector, node: &HtmlNode, tag: &str) -> bool {
    match sel {
        SimpleSelector::Tag(name) => tag == name,
        SimpleSelector::Class(class) => node
            .class
            .as_deref()
            .is_some_and(|classes| classes.split_ascii_whitespace().any(|value| value == class)),
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
    input.split(',').filter_map(parse_one_selector).collect()
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
    Some(SimpleSelector::Tag(s.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::{CssEngine, parse_one_selector, selector_matches};
    use crate::gpu_ui::html::HtmlNode;

    #[test]
    fn tag_selectors_support_all_and_custom_elements() {
        let article = HtmlNode::element(1, "article", Vec::new());
        let article_selector = parse_one_selector("ARTICLE").unwrap();
        let custom_selector = parse_one_selector("my-widget").unwrap();

        assert!(selector_matches(
            &article_selector,
            &article,
            article.kind.css_tag_name(),
        ));
        assert!(!selector_matches(
            &custom_selector,
            &article,
            article.kind.css_tag_name(),
        ));
    }

    #[test]
    fn resolves_default_styles_and_multiple_classes() {
        let mut button = HtmlNode::element(1, "button", Vec::new());
        button.class = Some("control primary".into());
        let css = CssEngine::from_css(
            "button { background-color: #ffffff; border-width: 2px; } .primary { color: #123456; }",
        );
        let resolved = css.resolve(&button);

        assert!(resolved.background_color.is_some());
        assert_eq!(resolved.border_width, Some(2.0));
        assert!(resolved.color.is_some());
    }
}
