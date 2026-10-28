//! Cutting and drilling tools.

use crate::LayerId;

/// Stable identifier of a tool inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// How a tool is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// Drill bit.
    Drill,
    /// Isolation cutter (V-bit or end mill).
    Isolation,
    /// Board cut-out cutter.
    Cutout,
}

impl ToolKind {
    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Drill => "Drill",
            Self::Isolation => "Isolation",
            Self::Cutout => "Cutout",
        }
    }
}

/// A tool from the project tool table.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    id: ToolId,
    name: String,
    kind: ToolKind,
    diameter_mm: f64,
    feed_mm_min: f64,
    plunge_mm_min: f64,
    spindle_rpm: u32,
    source_layer: Option<LayerId>,
}

impl Tool {
    pub(crate) fn new(
        id: ToolId,
        name: String,
        kind: ToolKind,
        diameter_mm: f64,
        source_layer: Option<LayerId>,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            diameter_mm: diameter_mm.max(0.0),
            feed_mm_min: 300.0,
            plunge_mm_min: 120.0,
            spindle_rpm: 10_000,
            source_layer,
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

    /// Tool use.
    pub const fn kind(&self) -> ToolKind {
        self.kind
    }

    /// Change the tool use.
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

    /// Source layer for tools imported from an Excellon file.
    pub const fn source_layer(&self) -> Option<LayerId> {
        self.source_layer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dimensions_are_ignored() {
        let mut tool = Tool::new(ToolId::new(1), "T1".into(), ToolKind::Drill, 0.8, None);
        tool.set_diameter_mm(-1.0);
        tool.set_feed_mm_min(0.0);
        assert_eq!(tool.diameter_mm(), 0.0);
        assert_eq!(tool.feed_mm_min(), 300.0);
    }
}
