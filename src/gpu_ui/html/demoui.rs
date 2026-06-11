use crate::gpu_ui::html::node::{
    ButtonType, ElementKind, HtmlNode, Inline, InputType, SvgChild,
};

pub fn build_demoui_document() -> Vec<HtmlNode> {
    let mut id = 1u32;
    let mut next = || {
        let current = id;
        id += 1;
        current
    };

    vec![
        HtmlNode::new(
            next(),
            ElementKind::Paragraph {
                inlines: vec![Inline::Text("Navigation:".into())],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Link {
                href: "/helloworld".into(),
                text: "Open Hello World page".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Link {
                href: "/svg-demo".into(),
                text: "Open SVG demo page".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Paragraph {
                inlines: vec![Inline::Italic("kursiver (schräger)".into())],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 1,
                text: "HTML Without CSS or JavaScript".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Paragraph {
                inlines: vec![Inline::Bold("and this is bold text".into())],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::OrderedList {
                items: vec!["Coffee".into(), "Tea".into(), "Milk".into()],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::UnorderedList {
                items: vec!["Coffee".into(), "Tea".into(), "Milk".into()],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Details {
                summary: "Project".into(),
                summary_checkbox: true,
                children: vec![
                    HtmlNode::new(
                        next(),
                        ElementKind::Paragraph {
                            inlines: vec![Inline::Text(
                                "A node can contain arbitrary HTML when expanded.".into(),
                            )],
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Details {
                            summary: "src".into(),
                            summary_checkbox: true,
                            children: vec![HtmlNode::new(
                                next(),
                                ElementKind::Div {
                                    children: vec![HtmlNode::new(
                                        next(),
                                        ElementKind::PlainText {
                                            text: "main.ts, widgets/...".into(),
                                        },
                                    )],
                                },
                            )],
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::PlainText {
                            text: "package.json".into(),
                        },
                    ),
                ],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(next(), ElementKind::Color),
        HtmlNode::new(
            next(),
            ElementKind::Svg {
                width: 240.0,
                height: 140.0,
                children: vec![
                    SvgChild::Rect {
                        x: 1.0,
                        y: 1.0,
                        width: 98.0,
                        height: 58.0,
                        fill: [0.965, 0.965, 0.965, 1.0],
                        stroke: [0.133, 0.133, 0.133, 1.0],
                    },
                    SvgChild::Circle {
                        cx: 30.0,
                        cy: 30.0,
                        r: 12.0,
                        fill: [0.086, 0.639, 0.290, 1.0],
                    },
                    SvgChild::Path {
                        points: vec![(55.0, 40.0), (72.0, 20.0), (90.0, 40.0)],
                        stroke: [0.145, 0.388, 0.922, 1.0],
                    },
                ],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Canvas {
                width: 240.0,
                height: 140.0,
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Iframe {
                children: vec![
                    HtmlNode::new(
                        next(),
                        ElementKind::Heading {
                            level: 2,
                            text: "Inside iframe".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Input {
                            input_type: InputType::Text,
                            name: "nested".into(),
                            value: "Nested input".into(),
                            checked: false,
                            label: None,
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Dialog {
                            floating: false,
                            children: vec![
                                HtmlNode::new(
                                    next(),
                                    ElementKind::Paragraph {
                                        inlines: vec![Inline::Text(
                                            "Floating dialog (iframe).".into(),
                                        )],
                                    },
                                ),
                                HtmlNode::new(
                                    next(),
                                    ElementKind::Button {
                                        label: "OK".into(),
                                        button_type: ButtonType::Button,
                                    },
                                ),
                            ],
                        },
                    ),
                ],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Buttons and Inputs".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Search {
                value: "Search...".into(),
                width: 420.0,
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Select {
                options: vec![
                    "One".into(),
                    "Two".into(),
                    "Three".into(),
                    "Four".into(),
                    "Five".into(),
                ],
                selected: 0,
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Date/Time Inputs".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::Time,
                name: "time".into(),
                value: "12:34:56".into(),
                checked: false,
                label: Some("Time:".into()),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::Date,
                name: "date".into(),
                value: "2026-02-01".into(),
                checked: false,
                label: Some("Date (simplified):".into()),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::Month,
                name: "month".into(),
                value: "2026-02".into(),
                checked: false,
                label: Some("Month:".into()),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::Week,
                name: "week".into(),
                value: "2026-W06".into(),
                checked: false,
                label: Some("Week (simplified):".into()),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::DateTimeLocal,
                name: "dt".into(),
                value: "2026-02-01T12:34:56".into(),
                checked: false,
                label: Some("Datetime-local (simplified):".into()),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Tree (Details + Checkbox)".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::PlainText {
                text: "(JONAS BAETHKE HALLO TEXT OKAY FRAGE GUT PROBE DREI NEUN)".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Details {
                summary: "Form".into(),
                summary_checkbox: false,
                children: vec![HtmlNode::new(
                    next(),
                    ElementKind::Form {
                        children: vec![
                            HtmlNode::new(
                                next(),
                                ElementKind::Label {
                                    text: "Name:".into(),
                                    control: Box::new(HtmlNode::new(
                                        next(),
                                        ElementKind::Input {
                                            input_type: InputType::Text,
                                            name: "name".into(),
                                            value: String::new(),
                                            checked: false,
                                            label: None,
                                        },
                                    )),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Label {
                                    text: "Password:".into(),
                                    control: Box::new(HtmlNode::new(
                                        next(),
                                        ElementKind::Input {
                                            input_type: InputType::Password,
                                            name: "password".into(),
                                            value: String::new(),
                                            checked: false,
                                            label: None,
                                        },
                                    )),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Button {
                                    label: "Submit".into(),
                                    button_type: ButtonType::Submit,
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Button {
                                    label: "Reset".into(),
                                    button_type: ButtonType::Reset,
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Button {
                                    label: "A Much Longer Button Label To See How Centering And Wrapping Looks".into(),
                                    button_type: ButtonType::Button,
                                },
                            ),
                        ],
                    },
                )],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Checkboxes and Radio Buttons".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Details {
                summary: "Checkbox form".into(),
                summary_checkbox: false,
                children: vec![HtmlNode::new(
                    next(),
                    ElementKind::Form {
                        children: vec![
                            HtmlNode::new(
                                next(),
                                ElementKind::Input {
                                    input_type: InputType::Checkbox,
                                    name: "subscribe".into(),
                                    value: String::new(),
                                    checked: false,
                                    label: Some("Subscribe to newsletter".into()),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::PlainText {
                                    text: "Favorite color:".into(),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Input {
                                    input_type: InputType::Radio,
                                    name: "color".into(),
                                    value: "Red".into(),
                                    checked: true,
                                    label: Some("Red".into()),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Input {
                                    input_type: InputType::Radio,
                                    name: "color".into(),
                                    value: "Blue".into(),
                                    checked: false,
                                    label: Some("Blue".into()),
                                },
                            ),
                            HtmlNode::new(
                                next(),
                                ElementKind::Input {
                                    input_type: InputType::Radio,
                                    name: "color".into(),
                                    value: "Green".into(),
                                    checked: false,
                                    label: Some("Green".into()),
                                },
                            ),
                        ],
                    },
                )],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Progress and Meter".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Details {
                summary: "Progress section".into(),
                summary_checkbox: false,
                children: vec![
                    HtmlNode::new(
                        next(),
                        ElementKind::PlainText {
                            text: "Download progress:".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Progress {
                            value: 60.0,
                            max: 100.0,
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::PlainText {
                            text: "Skill level:".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Meter {
                            value: 0.7,
                            label: "70%".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::PlainText {
                            text: "Range slider:".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Slider {
                            value: 0.7,
                            label: "70%".into(),
                        },
                    ),
                ],
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Expandable Content".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Details {
                summary: "Click to expand".into(),
                summary_checkbox: false,
                children: vec![HtmlNode::new(
                    next(),
                    ElementKind::Paragraph {
                        inlines: vec![Inline::Text(
                            "This text is hidden until you open it.".into(),
                        )],
                    },
                )],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(
            next(),
            ElementKind::Dialog {
                floating: true,
                children: vec![
                    HtmlNode::new(
                        next(),
                        ElementKind::Paragraph {
                            inlines: vec![Inline::Text(
                                "Floating dialog (drag the border/background).".into(),
                            )],
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Label {
                            text: "Dialog input:".into(),
                            control: Box::new(HtmlNode::new(
                                next(),
                                ElementKind::Input {
                                    input_type: InputType::Text,
                                    name: "dialog".into(),
                                    value: "Drag outside me".into(),
                                    checked: false,
                                    label: None,
                                },
                            )),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Button {
                            label: "OK".into(),
                            button_type: ButtonType::Button,
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Button {
                            label: "Cancel".into(),
                            button_type: ButtonType::Button,
                        },
                    ),
                ],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(
            next(),
            ElementKind::Form {
                children: vec![
                    HtmlNode::new(
                        next(),
                        ElementKind::Heading {
                            level: 2,
                            text: "Table".into(),
                        },
                    ),
                    HtmlNode::new(
                        next(),
                        ElementKind::Table {
                            headers: vec!["Item".into(), "Quantity".into()],
                            rows: vec![
                                vec!["Apples".into(), "3".into()],
                                vec!["Oranges".into(), "5".into()],
                            ],
                        },
                    ),
                ],
            },
        ),
        HtmlNode::new(next(), ElementKind::HorizontalRule),
        HtmlNode::new(
            next(),
            ElementKind::Heading {
                level: 2,
                text: "Text Input + Textarea".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Input {
                input_type: InputType::Text,
                name: "bottomText".into(),
                value: "Single line".into(),
                checked: false,
                label: None,
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Textarea {
                name: "bottomArea".into(),
                value: "Multiline (for now rendered like an input)".into(),
                rows: 3,
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Image {
                width: 240.0,
                height: 240.0,
                alt: "Inline red PNG demo".into(),
            },
        ),
        HtmlNode::new(
            next(),
            ElementKind::Footer {
                text: "Pure HTML demo — browser default styling only".into(),
            },
        ),
    ]
}
