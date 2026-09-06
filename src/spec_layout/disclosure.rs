use super::{NodeId, QualName, SpecLayout};

impl SpecLayout {
    fn summary_at(&self, position: Option<[f32; 2]>, scroll_y: f32) -> Option<NodeId> {
        let [x, y] = position?;
        let (width, height) = self.document.viewport().logical_size();
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 || x >= width || y >= height {
            return None;
        }
        let mut id = self.document.hit(x, y + scroll_y)?.node_id;
        loop {
            let node = self.document.get_node(id)?;
            if let Some(el) = node.element_data() {
                let tag = el.name.local.as_ref();
                // Interactive descendants have their own activation behavior.
                if matches!(tag, "button" | "input" | "select" | "textarea" | "label")
                    || (tag == "a" && el.attr(blitz_dom::local_name!("href")).is_some())
                    || (matches!(tag, "audio" | "video")
                        && el.attr(blitz_dom::local_name!("controls")).is_some())
                {
                    return None;
                }
                if tag == "summary" {
                    let parent = self.document.get_node(node.parent?)?;
                    return (parent.element_data()?.name.local.as_ref() == "details"
                        && parent.children.iter().copied().find(|child| {
                            self.document
                                .get_node(*child)
                                .and_then(|n| n.element_data())
                                .is_some_and(|el| el.name.local.as_ref() == "summary")
                        }) == Some(id))
                    .then_some(id);
                }
            }
            id = node.parent?;
        }
    }

    /// A primary press/release activates the first summary through Blitz's
    /// invalidating `open` mutation. UI4 scroll remains a projection offset.
    pub fn pointer_button(
        &mut self,
        position: Option<[f32; 2]>,
        scroll_y: f32,
        down: bool,
    ) -> bool {
        let target = self.summary_at(position, scroll_y);
        if down {
            self.pressed_summary = target.zip(position);
            return false;
        }
        let Some((pressed, start)) = self.pressed_summary.take() else {
            return false;
        };
        let Some(point) = position else {
            return false;
        };
        if target != Some(pressed)
            || (point[0] - start[0]).abs() > 4.0
            || (point[1] - start[1]).abs() > 4.0
        {
            return false;
        }
        let details = self
            .document
            .get_node(pressed)
            .and_then(|n| n.parent)
            .expect("summary parent checked");
        self.document.toggle_details_open(details);
        let element = self
            .document
            .get_node(details)
            .and_then(|n| n.element_data())
            .expect("details checked");
        if element.attr(blitz_dom::local_name!("open")).is_some()
            && let Some(name) = element
                .attr(blitz_dom::local_name!("name"))
                .filter(|name| !name.is_empty())
        {
            let others: Vec<_> = self
                .document
                .tree()
                .iter()
                .filter_map(|(_, node)| {
                    let el = node.element_data()?;
                    (node.id != details
                        && node.flags.is_in_document()
                        && el.name.local.as_ref() == "details"
                        && el.attr(blitz_dom::local_name!("name")) == Some(name)
                        && el.attr(blitz_dom::local_name!("open")).is_some())
                    .then_some(node.id)
                })
                .collect();
            for id in others {
                self.document
                    .mutate()
                    .clear_attribute(id, QualName::new(None, "".into(), "open".into()));
            }
        }
        true
    }
}
