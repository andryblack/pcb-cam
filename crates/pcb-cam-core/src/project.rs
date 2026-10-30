//! Project container.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::geometry::{Bounds, LayerGeometry, Point};
use crate::layer::{Layer, LayerId, SourceFormat, suggest_color};
use crate::operation::{Operation, OperationId, OperationKind};
use crate::units::Unit;

/// Default name of an unsaved project.
pub const UNTITLED_PROJECT: &str = "Untitled";

/// Persistent mirror axes used by layer flips.
///
/// `x` is the vertical axis (mirrors left/right); `y` is the horizontal axis
/// (mirrors top/bottom). Both are created on the first flip and then stay in
/// place until the user moves them explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlipAxes {
    /// X coordinate of the vertical axis.
    pub x: f64,
    /// Y coordinate of the horizontal axis.
    pub y: f64,
}

impl FlipAxes {
    /// Create axes at the given coordinates.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The pivot shared by both axis mirrors.
    pub fn pivot(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Whether both axis coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A PCB CAM project: source layers and an ordered task timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    name: String,
    layers: Vec<Layer>,
    operations: Vec<Operation>,
    units: Unit,
    gcode_origin: Point,
    #[serde(default)]
    flip_axes: Option<FlipAxes>,
    next_layer_id: u64,
    next_operation_id: u64,
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
            operations: Vec::new(),
            units: Unit::Millimeter,
            gcode_origin: Point::default(),
            flip_axes: None,
            next_layer_id: 1,
            next_operation_id: 1,
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

    /// Board-space point that becomes X0 Y0 in the exported G-code.
    pub fn gcode_origin(&self) -> Point {
        self.gcode_origin
    }

    /// Move the G-code zero point in board space.
    pub fn set_gcode_origin(&mut self, origin: Point) {
        if origin.x.is_finite() && origin.y.is_finite() {
            self.gcode_origin = origin;
        }
    }

    /// All source layers, in display order.
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

    /// Add a parsed source layer and return its new identifier.
    ///
    /// An empty `name` is replaced with a generated format-based name.
    pub fn add_layer(
        &mut self,
        name: impl Into<String>,
        source: Option<PathBuf>,
        color: Option<[u8; 3]>,
        geometry: LayerGeometry,
    ) -> LayerId {
        let id = LayerId::new(self.next_layer_id);
        self.next_layer_id += 1;

        let format = match geometry {
            LayerGeometry::Gerber(_) => SourceFormat::Gerber,
            LayerGeometry::Excellon(_) => SourceFormat::Excellon,
        };
        let mut name = name.into();
        if name.trim().is_empty() {
            name = format!("{} {}", format.label(), self.layers.len() + 1);
        }
        let color = color.unwrap_or_else(|| suggest_color(&name, format, self.layers.len()));

        self.layers
            .push(Layer::new(id, name, source, color, geometry));
        id
    }

    /// Remove a layer and any tasks that depend on it.
    ///
    /// Returns `true` when a layer was removed.
    pub fn remove_layer(&mut self, id: LayerId) -> bool {
        let before = self.layers.len();
        self.layers.retain(|layer| layer.id() != id);
        if self.layers.len() == before {
            return false;
        }

        self.operations
            .retain(|operation| operation.source_layer() != id);
        true
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

    /// Bounding box of all layers, or only visible ones.
    pub fn bounds(&self, visible_only: bool) -> Option<Bounds> {
        self.layers
            .iter()
            .filter(|layer| !visible_only || layer.visible())
            .filter_map(Layer::bounds)
            .reduce(Bounds::union)
    }

    /// Centre used to create the flip axes on the first flip.
    ///
    /// A board-outline layer wins when one is present, so flipping a single
    /// copper or drill layer stays registered with the rest of the board.
    /// Otherwise the bounding box of all layers is used.
    pub fn mirror_center(&self) -> Option<Point> {
        self.layers
            .iter()
            .filter(|layer| layer.is_board_outline())
            .filter_map(Layer::bounds)
            .reduce(Bounds::union)
            .or_else(|| self.bounds(false))
            .map(Bounds::center)
    }

    /// Persistent mirror axes, created by the first flip operation.
    pub const fn flip_axes(&self) -> Option<FlipAxes> {
        self.flip_axes
    }

    /// Return the project's mirror axes, creating them from the board centre on
    /// first use.
    pub fn ensure_flip_axes(&mut self) -> Option<FlipAxes> {
        if let Some(axes) = self.flip_axes {
            return Some(axes);
        }
        let center = self.mirror_center()?;
        let axes = FlipAxes::new(center.x, center.y);
        self.flip_axes = Some(axes);
        Some(axes)
    }

    /// Move the mirror axes and re-apply every flipped layer around the new
    /// axes.
    ///
    /// Returns `false` for non-finite coordinates.
    pub fn set_flip_axes(&mut self, axes: FlipAxes) -> bool {
        if !axes.is_finite() {
            return false;
        }
        if let Some(old) = self.flip_axes
            && old != axes
        {
            let old_pivot = old.pivot();
            let new_pivot = axes.pivot();
            for layer in &mut self.layers {
                layer.rebase_flips(old_pivot, new_pivot);
            }
        }
        self.flip_axes = Some(axes);
        true
    }

    /// Mirror selected layers around the project's [`Self::flip_axes`].
    ///
    /// The axes are created from the board centre on first use and then stay
    /// fixed. Returns the number of layers that were flipped.
    pub fn flip_layers(&mut self, ids: &[LayerId], horizontal: bool, vertical: bool) -> usize {
        if ids.is_empty() || (!horizontal && !vertical) {
            return 0;
        }
        let Some(axes) = self.ensure_flip_axes() else {
            return 0;
        };
        let pivot = axes.pivot();
        let mut flipped = 0;
        for layer in &mut self.layers {
            if ids.contains(&layer.id()) {
                layer.flip(horizontal, vertical, pivot);
                flipped += 1;
            }
        }
        flipped
    }

    /// Number of visible layers.
    pub fn visible_layer_count(&self) -> usize {
        self.layers.iter().filter(|layer| layer.visible()).count()
    }

    /// Total number of drawing primitives, holes and slots.
    pub fn object_count(&self) -> usize {
        self.layers.iter().map(Layer::object_count).sum()
    }

    /// Operations in execution order.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Look up an operation.
    pub fn operation(&self, id: OperationId) -> Option<&Operation> {
        self.operations
            .iter()
            .find(|operation| operation.id() == id)
    }

    /// Look up an operation mutably.
    pub fn operation_mut(&mut self, id: OperationId) -> Option<&mut Operation> {
        self.operations
            .iter_mut()
            .find(|operation| operation.id() == id)
    }

    /// Add an operation to the end of the timeline.
    pub fn add_operation(&mut self, name: impl Into<String>, kind: OperationKind) -> OperationId {
        let id = OperationId::new(self.next_operation_id);
        self.next_operation_id += 1;

        let mut name = name.into();
        if name.trim().is_empty() {
            name = format!("Operation {}", self.operations.len() + 1);
        }
        self.operations.push(Operation::new(id, name, kind));
        id
    }

    /// Point every task that uses `from` at another machining tool.
    pub fn remap_tool(&mut self, from: crate::ToolId, to: crate::ToolId) {
        for operation in &mut self.operations {
            match operation.kind_mut() {
                OperationKind::Drill(task) if task.tool == from => task.tool = to,
                OperationKind::SpotDrill(task) if task.tool == from => task.tool = to,
                OperationKind::HoleMill(task) if task.tool == from => task.tool = to,
                OperationKind::Cutout(task) if task.tool == from => task.tool = to,
                OperationKind::Engrave(task) if task.tool == from => task.tool = to,
                _ => {}
            }
        }
    }

    /// Remove an operation. Returns `true` when it existed.
    pub fn remove_operation(&mut self, id: OperationId) -> bool {
        let before = self.operations.len();
        self.operations.retain(|operation| operation.id() != id);
        self.operations.len() != before
    }

    /// Move an operation to a new index in the timeline.
    pub fn move_operation(&mut self, id: OperationId, target: usize) -> bool {
        let Some(index) = self
            .operations
            .iter()
            .position(|operation| operation.id() == id)
        else {
            return false;
        };
        let operation = self.operations.remove(index);
        let target = target.min(self.operations.len());
        self.operations.insert(target, operation);
        true
    }

    /// Number of enabled drilling tasks.
    pub fn enabled_drill_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|operation| {
                operation.enabled() && matches!(operation.kind(), OperationKind::Drill(_))
            })
            .count()
    }

    /// Number of enabled tasks of any kind.
    pub fn enabled_task_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|operation| operation.enabled())
            .count()
    }

    /// Tasks in execution order (alias for [`Self::operations`]).
    pub fn tasks(&self) -> &[Operation] {
        &self.operations
    }

    /// Look up a task (alias for [`Self::operation`]).
    pub fn task(&self, id: OperationId) -> Option<&Operation> {
        self.operation(id)
    }

    /// Look up a task mutably (alias for [`Self::operation_mut`]).
    pub fn task_mut(&mut self, id: OperationId) -> Option<&mut Operation> {
        self.operation_mut(id)
    }

    /// Append a task (alias for [`Self::add_operation`]).
    pub fn add_task(&mut self, name: impl Into<String>, kind: OperationKind) -> OperationId {
        self.add_operation(name, kind)
    }

    /// Remove a task (alias for [`Self::remove_operation`]).
    pub fn remove_task(&mut self, id: OperationId) -> bool {
        self.remove_operation(id)
    }

    /// Move a task (alias for [`Self::move_operation`]).
    pub fn move_task(&mut self, id: OperationId, target: usize) -> bool {
        self.move_operation(id, target)
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
    use crate::ToolId;
    use crate::geometry::{GerberLayer, Point, Primitive};
    use crate::operation::DrillOperation;

    fn layer() -> LayerGeometry {
        LayerGeometry::Gerber(GerberLayer::default())
    }

    fn point_layer(x: f64) -> LayerGeometry {
        point_layer_at(x, 0.0)
    }

    fn point_layer_at(x: f64, y: f64) -> LayerGeometry {
        let mut gerber = GerberLayer::default();
        gerber.primitives.push(Primitive::Flash {
            position: Point::new(x, y),
            aperture: crate::geometry::Aperture::circle(1.0),
            rotation_degrees: 0.0,
            polarity: crate::geometry::Polarity::Dark,
        });
        LayerGeometry::Gerber(gerber)
    }

    #[test]
    fn empty_project_is_untitled_in_millimetres() {
        let project = Project::default();
        assert_eq!(project.name(), UNTITLED_PROJECT);
        assert_eq!(project.units(), Unit::Millimeter);
        assert!(project.is_empty());
        assert_eq!(project.object_count(), 0);
    }

    #[test]
    fn blank_name_falls_back_to_untitled() {
        let project = Project::new("   ");
        assert_eq!(project.name(), UNTITLED_PROJECT);
    }

    #[test]
    fn layers_get_stable_unique_ids() {
        let mut project = Project::new("Board");
        let first = project.add_layer("Top copper", None, None, layer());
        let second = project.add_layer("", None, None, layer());

        assert_ne!(first, second);
        assert_eq!(project.layers().len(), 2);
        assert_eq!(project.layer(first).unwrap().name(), "Top copper");
        assert_eq!(project.layer(second).unwrap().name(), "Gerber 2");
    }

    #[test]
    fn layer_visibility_can_be_toggled() {
        let mut project = Project::default();
        let id = project.add_layer("Drill", None, None, layer());

        assert!(project.layer(id).unwrap().visible());
        assert!(project.set_layer_visible(id, false));
        assert!(!project.layer(id).unwrap().visible());
        assert!(!project.set_layer_visible(LayerId::new(99), false));
        assert_eq!(project.visible_layer_count(), 0);
    }

    #[test]
    fn removing_a_layer_keeps_the_others_and_drops_operations() {
        let mut project = Project::default();
        let first = project.add_layer("A", None, None, layer());
        let second = project.add_layer("B", None, None, layer());
        let operation = project.add_operation(
            "Drill A",
            OperationKind::Drill(DrillOperation::new(first, ToolId::new(1))),
        );

        assert!(project.remove_layer(first));
        assert!(!project.remove_layer(first));
        assert_eq!(project.layers().len(), 1);
        assert_eq!(project.layer(second).unwrap().name(), "B");
        assert!(project.operation(operation).is_none());
    }

    #[test]
    fn project_bounds_follow_visibility() {
        let mut project = Project::default();
        let left = project.add_layer("left", None, None, point_layer(0.0));
        let right = project.add_layer("right", None, None, point_layer(10.0));
        let bounds = project.bounds(true).unwrap();
        assert!((bounds.width() - 11.0).abs() < 1e-9);
        project.set_layer_visible(left, false);
        let bounds = project.bounds(true).unwrap();
        assert!((bounds.width() - 1.0).abs() < 1e-9);
        project.set_layer_visible(right, false);
        assert!(project.bounds(true).is_none());
    }

    #[test]
    fn layer_flips_use_the_board_outline_centre() {
        let mut project = Project::default();
        let copper = project.add_layer("Top copper", None, None, point_layer_at(0.0, 0.0));
        let outline = project.add_layer("Board Outline", None, None, point_layer_at(10.0, 5.0));

        assert_eq!(project.mirror_center(), Some(Point::new(10.0, 5.0)));
        assert_eq!(project.flip_layers(&[copper], true, true), 1);
        assert_eq!(project.flip_axes(), Some(FlipAxes::new(10.0, 5.0)));
        assert_eq!(project.flip_layers(&[outline], true, true), 1);
        assert_eq!(project.flip_axes(), Some(FlipAxes::new(10.0, 5.0)));
        assert_eq!(project.flip_layers(&[copper], false, false), 0);

        let Some(LayerGeometry::Gerber(gerber)) = project.layer(copper).map(Layer::geometry) else {
            panic!("expected Gerber copper layer");
        };
        let Primitive::Flash { position, .. } = &gerber.primitives[0] else {
            panic!("expected flash");
        };
        assert_eq!(*position, Point::new(20.0, 10.0));

        assert!(project.set_flip_axes(FlipAxes::new(2.0, 3.0)));
        let Some(LayerGeometry::Gerber(gerber)) = project.layer(copper).map(Layer::geometry) else {
            panic!("expected Gerber copper layer");
        };
        let Primitive::Flash { position, .. } = &gerber.primitives[0] else {
            panic!("expected flash");
        };
        assert_eq!(*position, Point::new(4.0, 6.0));
    }

    #[test]
    fn tasks_can_be_remapped_to_another_tool() {
        let mut project = Project::default();
        let layer = project.add_layer("A", None, None, layer());
        let task = project.add_operation(
            "one",
            OperationKind::Drill(DrillOperation::new(layer, ToolId::new(1))),
        );
        project.remap_tool(ToolId::new(1), ToolId::new(7));
        assert_eq!(project.operation(task).unwrap().tool(), ToolId::new(7));
    }

    #[test]
    fn operations_can_be_reordered() {
        let mut project = Project::default();
        let a = project.add_layer("A", None, None, layer());
        let first = project.add_operation(
            "one",
            OperationKind::Drill(DrillOperation::new(a, ToolId::new(1))),
        );
        let second = project.add_operation(
            "two",
            OperationKind::Drill(DrillOperation::new(a, ToolId::new(1))),
        );

        assert!(project.move_operation(second, 0));
        assert_eq!(project.operations()[0].id(), second);
        assert_eq!(project.operations()[1].id(), first);
        assert!(project.remove_operation(second));
        assert_eq!(project.operations().len(), 1);
    }
}
