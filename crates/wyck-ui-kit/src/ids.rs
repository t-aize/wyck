//! Element ids derived from another one, so a widget can name its parts.

use gpui::ElementId;

/// The id of child `n` of an element.
pub fn child_id(id: &ElementId, n: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.clone()), n.to_string().into())
}
