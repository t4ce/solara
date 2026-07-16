use rust_qjs_dom::{DomArtifact, DomEngine, DomNode};

use super::node::{ElementKind, HtmlNode, HtmlTag, InputType, SvgChild};

/// Adapts the renderer-neutral Parse5 DOM into Solara's existing render nodes.
///
/// The artifact remains owned by `Document`; this is deliberately only the
/// handoff into layout and paint, not Solara's canonical DOM representation.
pub fn parse_html(artifact: &DomArtifact, engine: &mut DomEngine) -> Result<Vec<HtmlNode>, String> {
    let html = find_html_element(&artifact.document)
        .ok_or_else(|| "Parse5 artifact does not contain an <html> element".to_string())?;
    let mut parser = NodeParser {
        next_id: 1,
        engine,
        suppress_style_refs: false,
    };
    Ok(vec![parser.parse_element(html)?])
}

fn find_html_element(node: &DomNode) -> Option<&DomNode> {
    if node
        .tag_name
        .as_deref()
        .is_some_and(|tag| tag.eq_ignore_ascii_case("html"))
    {
        return Some(node);
    }
    node.children.iter().find_map(find_html_element)
}

struct NodeParser<'a> {
    next_id: u32,
    engine: &'a mut DomEngine,
    suppress_style_refs: bool,
}

impl NodeParser<'_> {
    fn parse_element(&mut self, element: &DomNode) -> Result<HtmlNode, String> {
        let tag_name = element
            .tag_name
            .as_deref()
            .ok_or_else(|| format!("expected element node, found {:?}", element.node_name))?;
        let kind = match tag_name {
            "hr" => ElementKind::HorizontalRule,
            "input" => self.parse_input(element),
            "textarea" => ElementKind::Textarea {
                name: attribute(element, "name"),
                value: text_content(element),
                rows: number_attribute(element, "rows", 2.0) as u32,
            },
            "canvas" => ElementKind::Canvas {
                width: number_attribute(element, "width", 300.0),
                height: number_attribute(element, "height", 150.0),
            },
            "img" => ElementKind::Image {
                width: number_attribute(element, "width", 240.0),
                height: number_attribute(element, "height", 160.0),
                alt: attribute(element, "alt"),
            },
            "iframe" => {
                let iframe_children = match element.attribute("srcdoc") {
                    Some(source) => {
                        let artifact = self
                            .engine
                            .parse(source, "about:srcdoc")
                            .map_err(|error| format!("failed to parse iframe srcdoc: {error}"))?;
                        let root = find_html_element(&artifact.document).ok_or_else(|| {
                            "iframe Parse5 artifact does not contain an <html> element".to_string()
                        })?;
                        let previous = self.suppress_style_refs;
                        self.suppress_style_refs = true;
                        let parsed = self.parse_element(root);
                        self.suppress_style_refs = previous;
                        vec![parsed?]
                    }
                    None => Vec::new(),
                };
                ElementKind::Iframe {
                    children: iframe_children,
                }
            }
            "svg" => self.parse_svg(element),
            "progress" => ElementKind::Progress {
                value: number_attribute(element, "value", 0.0),
                max: number_attribute(element, "max", 1.0),
            },
            "meter" => ElementKind::Meter {
                value: number_attribute(element, "value", 0.0),
                label: text_content(element),
            },
            "search" => ElementKind::Search {
                value: attribute(element, "value"),
                width: number_attribute(element, "width", 320.0),
            },
            "slider" => ElementKind::Slider {
                value: number_attribute(element, "value", 0.0),
                label: text_content(element),
            },
            "color" => ElementKind::Color,
            _ => ElementKind::Element {
                tag: HtmlTag::from_name(tag_name),
                children: self.parse_children(element)?,
            },
        };

        let id = self.take_id();
        let mut node = HtmlNode::new(id, kind);
        node.class = element.attribute("class").map(str::to_string);
        node.id_attr = element.attribute("id").map(str::to_string);
        node.style_attr = element.attribute("style").map(str::to_string);
        node.style_ref = if self.suppress_style_refs {
            None
        } else {
            element.style_ref
        };
        node.open = element.attribute("open").is_some();
        Ok(node)
    }

    fn parse_children(&mut self, element: &DomNode) -> Result<Vec<HtmlNode>, String> {
        let mut children = Vec::new();
        for child in element.children.iter().chain(
            element
                .content
                .iter()
                .flat_map(|content| content.children.iter()),
        ) {
            if child.is_element() {
                children.push(self.parse_element(child)?);
            } else if child.node_name == "#text" {
                let normalized = normalize_text(child.value.as_deref().unwrap_or_default());
                if !normalized.is_empty() {
                    let id = self.take_id();
                    children.push(HtmlNode::new(
                        id,
                        ElementKind::PlainText { text: normalized },
                    ));
                }
            }
        }
        Ok(children)
    }

    fn parse_input(&self, element: &DomNode) -> ElementKind {
        let input_type = match element
            .attribute("type")
            .unwrap_or("text")
            .to_ascii_lowercase()
            .as_str()
        {
            "checkbox" => InputType::Checkbox,
            "password" => InputType::Password,
            "time" => InputType::Time,
            "date" => InputType::Date,
            "month" => InputType::Month,
            "week" => InputType::Week,
            "datetime-local" => InputType::DateTimeLocal,
            "radio" => InputType::Radio,
            _ => InputType::Text,
        };
        ElementKind::Input {
            input_type,
            name: attribute(element, "name"),
            value: attribute(element, "value"),
            checked: element.attribute("checked").is_some(),
            label: None,
        }
    }

    fn parse_svg(&self, element: &DomNode) -> ElementKind {
        let mut svg_children = Vec::new();
        for child in element.children.iter().filter(|child| child.is_element()) {
            match child.tag_name.as_deref().unwrap_or_default() {
                "rect" => svg_children.push(SvgChild::Rect {
                    x: number_attribute(child, "x", 0.0),
                    y: number_attribute(child, "y", 0.0),
                    width: number_attribute(child, "width", 0.0),
                    height: number_attribute(child, "height", 0.0),
                    fill: color_attribute(child, "fill", [0.85, 0.85, 0.85, 1.0]),
                    stroke: color_attribute(child, "stroke", [0.2, 0.2, 0.2, 1.0]),
                }),
                "circle" => svg_children.push(SvgChild::Circle {
                    cx: number_attribute(child, "cx", 0.0),
                    cy: number_attribute(child, "cy", 0.0),
                    r: number_attribute(child, "r", 0.0),
                    fill: color_attribute(child, "fill", [0.2, 0.5, 0.8, 1.0]),
                }),
                "path" => svg_children.push(SvgChild::Path {
                    points: parse_path_points(child.attribute("d").unwrap_or_default()),
                    stroke: color_attribute(child, "stroke", [0.2, 0.2, 0.2, 1.0]),
                }),
                _ => {}
            }
        }
        ElementKind::Svg {
            width: number_attribute(element, "width", 300.0),
            height: number_attribute(element, "height", 150.0),
            children: svg_children,
        }
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

fn attribute(element: &DomNode, name: &str) -> String {
    element.attribute(name).unwrap_or_default().to_string()
}

fn number_attribute(element: &DomNode, name: &str, default: f32) -> f32 {
    element
        .attribute(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn color_attribute(element: &DomNode, name: &str, default: [f32; 4]) -> [f32; 4] {
    element
        .attribute(name)
        .and_then(parse_hex_color)
        .unwrap_or(default)
}

fn parse_hex_color(value: &str) -> Option<[f32; 4]> {
    let hex = value.strip_prefix('#')?;
    let (red, green, blue) = match hex.len() {
        3 => (
            u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
        ),
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some([
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
        1.0,
    ])
}

fn parse_path_points(path: &str) -> Vec<(f32, f32)> {
    let numbers: Vec<f32> = path
        .split(|character: char| {
            character.is_ascii_alphabetic() || character == ',' || character.is_whitespace()
        })
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    numbers
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

fn text_content(element: &DomNode) -> String {
    let mut text = Vec::new();
    collect_text(element, &mut text);
    normalize_text(&text.join(" "))
}

fn collect_text<'a>(node: &'a DomNode, text: &mut Vec<&'a str>) {
    if let ("#text", Some(value)) = (node.node_name.as_str(), node.value.as_deref()) {
        text.push(value);
    }
    for child in &node.children {
        collect_text(child, text);
    }
    if let Some(content) = &node.content {
        collect_text(content, text);
    }
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use rust_qjs_dom::DomEngine;

    use super::parse_html;
    use crate::gpu_ui::html::node::{ElementKind, HtmlNode};

    fn parse_source(source: &str) -> Vec<HtmlNode> {
        let mut engine = DomEngine::new().expect("QuickJS DOM engine starts");
        let artifact = engine
            .parse(source, "https://solara.test/")
            .expect("Parse5 parses HTML");
        parse_html(&artifact, &mut engine).expect("artifact adapts to Solara nodes")
    }

    #[test]
    fn parses_the_current_demo_through_qjs_parse5() {
        let nodes = parse_source(include_str!("../../../docs/demoui.html"));
        let mut tags = Vec::new();
        collect_tags(&nodes, &mut tags);
        for tag in [
            "html", "body", "details", "svg", "iframe", "input", "dialog",
        ] {
            assert!(tags.contains(&tag), "missing node for <{tag}>");
        }
    }

    #[test]
    fn preserves_common_attributes_and_text() {
        let nodes = parse_source("<main id='app' class='page wide' style='color:red'>Hello</main>");
        let main = find_tag(&nodes, "main").unwrap();
        assert_eq!(main.id_attr.as_deref(), Some("app"));
        assert_eq!(main.class.as_deref(), Some("page wide"));
        assert_eq!(main.style_attr.as_deref(), Some("color:red"));
        assert!(main.style_ref.is_some());
    }

    fn collect_tags<'a>(nodes: &'a [HtmlNode], tags: &mut Vec<&'a str>) {
        for node in nodes {
            tags.push(node.kind.css_tag_name());
            match &node.kind {
                ElementKind::Element { children, .. }
                | ElementKind::Details { children, .. }
                | ElementKind::Div { children }
                | ElementKind::Form { children }
                | ElementKind::Iframe { children }
                | ElementKind::Dialog { children, .. } => collect_tags(children, tags),
                ElementKind::Label { control, .. } => {
                    collect_tags(std::slice::from_ref(control), tags)
                }
                _ => {}
            }
        }
    }

    fn find_tag<'a>(nodes: &'a [HtmlNode], tag: &str) -> Option<&'a HtmlNode> {
        for node in nodes {
            if node.kind.css_tag_name() == tag {
                return Some(node);
            }
            let found = match &node.kind {
                ElementKind::Element { children, .. } => find_tag(children, tag),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
}
