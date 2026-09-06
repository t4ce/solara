use super::*;

pub(super) fn children(
    mutator: &mut DocumentMutator<'_>,
    parent: NodeId,
    source: &DomNode,
    path: &str,
    nodes: &mut BTreeMap<String, NodeId>,
) -> Result<(), String> {
    import_children(mutator, parent, source, path, nodes, false)
}

fn import_children(
    mutator: &mut DocumentMutator<'_>,
    parent: NodeId,
    source: &DomNode,
    path: &str,
    nodes: &mut BTreeMap<String, NodeId>,
    inert: bool,
) -> Result<(), String> {
    for (index, child) in source.children.iter().enumerate() {
        let path = format!("{path}.{index}");
        let id = if let Some(tag) = &child.tag_name {
            let name = QualName::new(
                None,
                child
                    .namespace_uri
                    .as_deref()
                    .unwrap_or("http://www.w3.org/1999/xhtml")
                    .into(),
                tag.as_str().into(),
            );
            let attrs = child
                .attrs
                .iter()
                .map(|attr| Attribute {
                    name: QualName::new(
                        attr.prefix.as_deref().map(Into::into),
                        attr.namespace.as_deref().unwrap_or("").into(),
                        attr.name.as_str().into(),
                    ),
                    value: attr.value.clone(),
                })
                .collect();
            mutator.create_element(name, attrs)
        } else {
            match child.node_name.as_str() {
                "#text" => mutator.create_text_node(child.value.as_deref().unwrap_or("")),
                "#comment" => mutator.create_comment_node(child.data.as_deref().unwrap_or("")),
                // Blitz currently uses standards mode and does not retain doctypes.
                "#documentType" => continue,
                kind => return Err(format!("unsupported artifact node {kind:?} at {path}")),
            }
        };
        nodes.insert(path.clone(), id);
        // Build descendants while detached, then mount the completed subtree.
        // This lets Blitz process style elements with their complete source.
        import_children(mutator, id, child, &path, nodes, inert)?;
        if let Some(content) = &child.content {
            let fragment = mutator.template_contents(id);
            let content_path = format!("{path}.content");
            nodes.insert(content_path.clone(), fragment);
            import_children(mutator, fragment, content, &content_path, nodes, true)?;
        }
        if inert {
            // At the pinned revision, append_children queues <style> processing
            // even for detached template contents. These freshly created nodes
            // are never live, so wire only their tree links. Mounting them later
            // through append_children performs Blitz's normal activation.
            let parent_node = mutator
                .doc
                .get_node_mut(parent)
                .ok_or("missing inert parent")?;
            if parent_node.flags.is_in_document() {
                return Err("template content cannot be attached directly to a live parent".into());
            }
            parent_node.children.push(id);
            mutator
                .doc
                .get_node_mut(id)
                .ok_or("missing inert child")?
                .parent = Some(parent);
        } else {
            mutator.append_children(parent, &[id]);
        }
    }
    Ok(())
}
