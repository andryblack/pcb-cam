//! Machining tools and the user tool library.

use serde::{Deserialize, Serialize};

/// Stable identifier of a machining tool in the user library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ToolId(u64);

impl ToolId {
    /// Construct an identifier from its numeric value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Numeric value of the identifier.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// How a machining tool cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    /// Drill bit. It can only plunge to size.
    Drill,
    /// End mill. It can drill and helically interpolate larger holes.
    EndMill,
    /// V-bit or engraving cutter.
    VBit,
    /// Spot drill / countersink used for centre punching.
    Cone,
}

impl ToolKind {
    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Drill => "Drill",
            Self::EndMill => "End mill",
            Self::VBit => "V-bit",
            Self::Cone => "Cone",
        }
    }

    /// Stable command id fragment.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Drill => "drill",
            Self::EndMill => "end_mill",
            Self::VBit => "v_bit",
            Self::Cone => "cone",
        }
    }

    /// Parse a command id fragment.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "drill" => Ok(Self::Drill),
            "end_mill" | "endmill" => Ok(Self::EndMill),
            "v_bit" | "vbit" | "engraver" => Ok(Self::VBit),
            "cone" | "spot" | "spot_drill" | "countersink" => Ok(Self::Cone),
            other => Err(format!("unknown tool kind `{other}`")),
        }
    }

    /// Whether the tool can enlarge a hole by side cutting.
    pub const fn can_interpolate(self) -> bool {
        matches!(self, Self::EndMill | Self::VBit)
    }
}

/// A machining tool from the user library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    id: ToolId,
    name: String,
    kind: ToolKind,
    diameter_mm: f64,
    feed_mm_min: f64,
    plunge_mm_min: f64,
    spindle_rpm: u32,
}

impl Tool {
    pub(crate) fn new(id: ToolId, name: String, kind: ToolKind, diameter_mm: f64) -> Self {
        Self {
            id,
            name,
            kind,
            diameter_mm: diameter_mm.max(0.0),
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            spindle_rpm: 10_000,
        }
    }

    /// Stable tool identifier.
    pub const fn id(&self) -> ToolId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the tool. Empty names are ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if !name.is_empty() {
            self.name = name.to_owned();
        }
    }

    /// Tool type.
    pub const fn kind(&self) -> ToolKind {
        self.kind
    }

    /// Change the tool type.
    pub fn set_kind(&mut self, kind: ToolKind) {
        self.kind = kind;
    }

    /// Cutting diameter in millimetres.
    pub const fn diameter_mm(&self) -> f64 {
        self.diameter_mm
    }

    /// Change the cutting diameter.
    pub fn set_diameter_mm(&mut self, diameter_mm: f64) {
        if diameter_mm.is_finite() {
            self.diameter_mm = diameter_mm.max(0.0);
        }
    }

    /// XY feed rate.
    pub const fn feed_mm_min(&self) -> f64 {
        self.feed_mm_min
    }

    /// Change the XY feed rate.
    pub fn set_feed_mm_min(&mut self, feed_mm_min: f64) {
        if feed_mm_min.is_finite() && feed_mm_min > 0.0 {
            self.feed_mm_min = feed_mm_min;
        }
    }

    /// Z plunge rate.
    pub const fn plunge_mm_min(&self) -> f64 {
        self.plunge_mm_min
    }

    /// Change the Z plunge rate.
    pub fn set_plunge_mm_min(&mut self, plunge_mm_min: f64) {
        if plunge_mm_min.is_finite() && plunge_mm_min > 0.0 {
            self.plunge_mm_min = plunge_mm_min;
        }
    }

    /// Spindle speed.
    pub const fn spindle_rpm(&self) -> u32 {
        self.spindle_rpm
    }

    /// Change the spindle speed.
    pub fn set_spindle_rpm(&mut self, spindle_rpm: u32) {
        self.spindle_rpm = spindle_rpm;
    }
}

/// The user's machining tool library. It is shared by all projects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolLibrary {
    tools: Vec<Tool>,
    next_id: u64,
}

impl Default for ToolLibrary {
    fn default() -> Self {
        let mut library = Self {
            tools: Vec::new(),
            next_id: 1,
        };
        library.add_default("Cone 6.00 mm", ToolKind::Cone, 6.00, 200.0, 80.0, 12_000);
        library.add_default("Drill 0.30 mm", ToolKind::Drill, 0.30, 80.0, 50.0, 20_000);
        library.add_default("Drill 0.80 mm", ToolKind::Drill, 0.80, 120.0, 80.0, 18_000);
        library.add_default("Drill 1.00 mm", ToolKind::Drill, 1.00, 150.0, 100.0, 15_000);
        library.add_default(
            "End mill 1.00 mm",
            ToolKind::EndMill,
            1.00,
            300.0,
            120.0,
            12_000,
        );
        library.add_default(
            "End mill 2.00 mm",
            ToolKind::EndMill,
            2.00,
            400.0,
            150.0,
            10_000,
        );
        library.add_default("V-bit 0.20 mm", ToolKind::VBit, 0.20, 400.0, 100.0, 15_000);
        library
    }
}

impl ToolLibrary {
    /// Built-in library.
    pub fn new() -> Self {
        Self::default()
    }

    fn add_default(
        &mut self,
        name: &str,
        kind: ToolKind,
        diameter_mm: f64,
        feed: f64,
        plunge: f64,
        rpm: u32,
    ) {
        let id = self.add(name, kind, diameter_mm, feed, plunge, rpm);
        debug_assert!(self.tool(id).is_some());
    }

    /// All tools in library order.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Look up a tool.
    pub fn tool(&self, id: ToolId) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.id() == id)
    }

    /// Look up a tool mutably.
    pub fn tool_mut(&mut self, id: ToolId) -> Option<&mut Tool> {
        self.tools.iter_mut().find(|tool| tool.id() == id)
    }

    /// Add a tool and return its new id.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        kind: ToolKind,
        diameter_mm: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
    ) -> ToolId {
        let id = ToolId::new(self.next_id);
        self.next_id += 1;
        let mut name = name.into();
        if name.trim().is_empty() {
            name = format!("{} {:.2} mm", kind.label(), diameter_mm.max(0.0));
        }
        let mut tool = Tool::new(id, name, kind, diameter_mm);
        tool.set_feed_mm_min(feed);
        tool.set_plunge_mm_min(plunge);
        tool.set_spindle_rpm(spindle);
        self.tools.push(tool);
        id
    }

    /// Remove a tool. Returns `true` when it existed.
    pub fn remove(&mut self, id: ToolId) -> bool {
        let before = self.tools.len();
        self.tools.retain(|tool| tool.id() != id);
        self.tools.len() != before
    }

    /// Number of tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// First tool of a kind, used as a default for new tasks.
    pub fn first_of_kind(&self, kind: ToolKind) -> Option<ToolId> {
        self.tools
            .iter()
            .find(|tool| tool.kind() == kind)
            .map(Tool::id)
    }

    /// A tool of a kind with the closest diameter, preferring an exact match.
    pub fn closest(&self, kind: ToolKind, diameter_mm: f64) -> Option<ToolId> {
        self.tools
            .iter()
            .filter(|tool| tool.kind() == kind)
            .min_by(|a, b| {
                (a.diameter_mm() - diameter_mm)
                    .abs()
                    .total_cmp(&(b.diameter_mm() - diameter_mm).abs())
            })
            .map(Tool::id)
    }

    /// The largest tool of a kind that is not larger than `max_diameter_mm`.
    pub fn largest_fitting(&self, kind: ToolKind, max_diameter_mm: f64) -> Option<ToolId> {
        self.tools
            .iter()
            .filter(|tool| tool.kind() == kind && tool.diameter_mm() <= max_diameter_mm + 1e-9)
            .max_by(|a, b| a.diameter_mm().total_cmp(&b.diameter_mm()))
            .map(Tool::id)
    }

    /// Replace the contents; used when loading a persisted library.
    pub fn set_tools(&mut self, tools: Vec<Tool>) {
        self.tools = tools;
        self.next_id = self
            .tools
            .iter()
            .map(|tool| tool.id().value())
            .max()
            .unwrap_or(0)
            + 1;
    }

    /// Ensure the next id does not collide after deserialisation.
    pub fn normalize(&mut self) {
        if self.next_id == 0 {
            self.next_id = 1;
        }
        self.next_id = self.next_id.max(
            self.tools
                .iter()
                .map(|tool| tool.id().value() + 1)
                .max()
                .unwrap_or(1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_library_has_all_tool_kinds() {
        let library = ToolLibrary::new();
        assert!(library.len() >= 6);
        assert!(library.first_of_kind(ToolKind::Drill).is_some());
        assert!(library.first_of_kind(ToolKind::EndMill).is_some());
        assert!(library.first_of_kind(ToolKind::VBit).is_some());
        assert!(library.first_of_kind(ToolKind::Cone).is_some());
    }

    #[test]
    fn tool_dimensions_ignore_invalid_values() {
        let mut tool = Tool::new(ToolId::new(1), "T1".into(), ToolKind::Drill, 0.8);
        tool.set_diameter_mm(-1.0);
        tool.set_feed_mm_min(0.0);
        assert_eq!(tool.diameter_mm(), 0.0);
        assert_eq!(tool.feed_mm_min(), 300.0);
    }

    #[test]
    fn library_add_remove_and_closest() {
        let mut library = ToolLibrary::new();
        let id = library.add("Probe mill", ToolKind::EndMill, 0.5, 200.0, 80.0, 9000);
        assert_eq!(library.tool(id).unwrap().name(), "Probe mill");
        assert_eq!(library.closest(ToolKind::EndMill, 0.55), Some(id));
        assert!(library.remove(id));
        assert!(!library.remove(id));
    }

    #[test]
    fn tools_are_json_serializable() {
        let library = ToolLibrary::new();
        let json = serde_json::to_string(&library).unwrap();
        let restored: ToolLibrary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, library);
    }
}
