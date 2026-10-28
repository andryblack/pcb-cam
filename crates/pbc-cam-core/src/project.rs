//! Project container.

use std::path::PathBuf;

use crate::layer::{Layer, LayerId, SourceFormat};
use crate::units::Unit;

/// Default name of an unsaved project.
pub const UNTITLED_PROJECT: &str = "Untitled";

/// A PCB CAM project: a named collection of source layers and its preferred
/// display units.
#[derive(Debug, Clone)]
pub struct Project {
    name: String,
    layers: Vec<Layer>,
    units: Unit,
    next_layer_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        Self::new(UNTITLED_PROJECT)
    }
}

impl Project {
    /// Create an empty project with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let name = name.trim();
        Self {
            name: if name.is_empty() {
                UNTITLED_PROJECT.to_owned()
            } else {
                name.to_owned()
            },
            layers: Vec::new(),
            units: Unit::Millimeter,
            next_layer_id: 1,
        }
    }

    /// Project display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the project. Empty names are ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if !name.is_empty() {
            self.name = name.to_owned();
        }
    }

    /// All layers, in display order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Look up a layer by identifier.
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id() == id)
    }

    /// Look up a layer mutably by identifier.
    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|layer| layer.id() == id)
    }

    /// Add a source layer and return its new identifier.
    ///
    /// An empty `name` is replaced with a generated format-based name.
    pub fn add_layer(
        &mut self,
        name: impl Into<String>,
        format: SourceFormat,
        source: Option<PathBuf>,
    ) -> LayerId {
        let id = LayerId::new(self.next_layer_id);
        self.next_layer_id += 1;

        let mut name = name.into();
        if name.trim().is_empty() {
            name = format!("{} {}", format.label(), self.layers.len() + 1);
        }

        self.layers.push(Layer::new(id, name, format, source));
        id
    }

    /// Remove a layer. Returns `true` when a layer was removed.
    pub fn remove_layer(&mut self, id: LayerId) -> bool {
        let before = self.layers.len();
        self.layers.retain(|layer| layer.id() != id);
        self.layers.len() != before
    }

    /// Show or hide a layer. Returns `true` when the layer exists.
    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> bool {
        if let Some(layer) = self.layer_mut(id) {
            layer.set_visible(visible);
            true
        } else {
            false
        }
    }

    /// Preferred display unit.
    pub const fn units(&self) -> Unit {
        self.units
    }

    /// Set the preferred display unit.
    pub fn set_units(&mut self, units: Unit) {
        self.units = units;
    }

    /// Whether the project contains no layers.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_project_is_untitled_in_millimetres() {
        let project = Project::default();
        assert_eq!(project.name(), UNTITLED_PROJECT);
        assert_eq!(project.units(), Unit::Millimeter);
        assert!(project.is_empty());
    }

    #[test]
    fn blank_name_falls_back_to_untitled() {
        let project = Project::new("   ");
        assert_eq!(project.name(), UNTITLED_PROJECT);
    }

    #[test]
    fn layers_get_stable_unique_ids() {
        let mut project = Project::new("Board");
        let first = project.add_layer("Top copper", SourceFormat::Gerber, None);
        let second = project.add_layer("", SourceFormat::Excellon, None);

        assert_ne!(first, second);
        assert_eq!(project.layers().len(), 2);
        assert_eq!(project.layer(first).unwrap().name(), "Top copper");
        assert_eq!(project.layer(second).unwrap().name(), "Excellon 2");
    }

    #[test]
    fn layer_visibility_can_be_toggled() {
        let mut project = Project::default();
        let id = project.add_layer("Drill", SourceFormat::Excellon, None);

        assert!(project.layer(id).unwrap().visible());
        assert!(project.set_layer_visible(id, false));
        assert!(!project.layer(id).unwrap().visible());
        assert!(!project.set_layer_visible(crate::LayerId::new(99), false));
    }

    #[test]
    fn removing_a_layer_keeps_the_others() {
        let mut project = Project::default();
        let first = project.add_layer("A", SourceFormat::Gerber, None);
        let second = project.add_layer("B", SourceFormat::Gerber, None);

        assert!(project.remove_layer(first));
        assert!(!project.remove_layer(first));
        assert_eq!(project.layers().len(), 1);
        assert_eq!(project.layer(second).unwrap().name(), "B");
    }
}
