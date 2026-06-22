use scraper::{ElementRef, Html, Node};

use super::node::{ElementKind, HtmlNode, HtmlTag, InputType, SvgChild};

pub fn parse_html(source: &str) -> Vec<HtmlNode> {
    let document = Html::parse_document(source);
    let mut parser = NodeParser { next_id: 1 };
    vec![parser.parse_element(document.root_element())]
}

struct NodeParser {
    next_id: u32,
}

impl NodeParser {
    fn parse_element(&mut self, element: ElementRef<'_>) -> HtmlNode {
        let tag_name = element.value().name();
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
                let iframe_children = element
                    .attr("srcdoc")
                    .map(|source| {
                        let document = Html::parse_document(source);
                        vec![self.parse_element(document.root_element())]
                    })
                    .unwrap_or_default();
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
                children: self.parse_children(element),
            },
        };

        let id = self.take_id();
        let mut node = HtmlNode::new(id, kind);
        node.class = element.attr("class").map(str::to_string);
        node.id_attr = element.attr("id").map(str::to_string);
        node.style_attr = element.attr("style").map(str::to_string);
        node.open = element.attr("open").is_some();
        node
    }

    fn parse_children(&mut self, element: ElementRef<'_>) -> Vec<HtmlNode> {
        let mut children = Vec::new();
        for child in element.children() {
            match child.value() {
                Node::Element(_) => {
                    if let Some(element) = ElementRef::wrap(child) {
                        children.push(self.parse_element(element));
                    }
                }
                Node::Text(text) => {
                    let normalized = normalize_text(text);
                    if !normalized.is_empty() {
                        let id = self.take_id();
                        children.push(HtmlNode::new(
                            id,
                            ElementKind::PlainText { text: normalized },
                        ));
                    }
                }
                _ => {}
            }
        }
        children
    }

    fn parse_input(&self, element: ElementRef<'_>) -> ElementKind {
        let input_type = match element
            .attr("type")
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
            checked: element.attr("checked").is_some(),
            label: None,
        }
    }

    fn parse_svg(&mut self, element: ElementRef<'_>) -> ElementKind {
        let mut svg_children = Vec::new();
        for child in element.child_elements() {
            match child.value().name() {
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
                    points: parse_path_points(child.attr("d").unwrap_or_default()),
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

fn attribute(element: ElementRef<'_>, name: &str) -> String {
    element.attr(name).unwrap_or_default().to_string()
}

fn number_attribute(element: ElementRef<'_>, name: &str, default: f32) -> f32 {
    element
        .attr(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn color_attribute(element: ElementRef<'_>, name: &str, default: [f32; 4]) -> [f32; 4] {
    element
        .attr(name)
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

fn text_content(element: ElementRef<'_>) -> String {
    normalize_text(&element.text().collect::<Vec<_>>().join(" "))
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::parse_html;
    use crate::gpu_ui::html::node::{ElementKind, HtmlNode, HtmlTag};

    #[test]
    fn converts_every_demo_html_element_to_a_node() {
        let nodes = parse_html(include_str!("../../../docs/demoui.html"));
        let mut tags = Vec::new();
        collect_tags(&nodes, &mut tags);
        for tag in HtmlTag::STANDARD_NAMES {
            assert!(tags.contains(tag), "missing node for <{tag}>");
        }
    }

    #[test]
    fn preserves_common_attributes_and_text() {
        let nodes = parse_html("<main id='app' class='page wide' style='color:red'>Hello</main>");
        let main = find_tag(&nodes, "main").unwrap();
        assert_eq!(main.id_attr.as_deref(), Some("app"));
        assert_eq!(main.class.as_deref(), Some("page wide"));
        assert_eq!(main.style_attr.as_deref(), Some("color:red"));
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
            if let ElementKind::Element { children, .. } = &node.kind
                && let Some(found) = find_tag(children, tag)
            {
                return Some(found);
            }
        }
        None
    }
}
