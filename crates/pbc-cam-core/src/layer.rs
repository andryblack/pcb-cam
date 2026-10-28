//! Layer and source-format definitions.

use std::path::PathBuf;

/// Stable identifier of a layer inside a [`Project`](crate::Project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(u64);

impl LayerId {
    /// Numeric value of the identifier.
    pub const fn value(self) -> u64 {
        self.0
    }

    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// File format a layer was imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    /// Gerber RS-274X / X2 fabrication data.
    Gerber,
    /// Excellon drill and rout data.
    Excellon,
}

impl SourceFormat {
    /// Short label suitable for menus and layer rows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gerber => "Gerber",
            Self::Excellon => "Excellon",
        }
    }

    /// Lower-case file extensions commonly used by this format.
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Gerber => &[
                "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gto", "gbo", "gko",
            ],
            Self::Excellon => &["drl", "xln", "exc", "nc", "txt"],
        }
    }
}

/// A single source layer in a project.
///
/// Geometry is added by the import pipeline. At this stage the layer only
/// describes where it came from and whether it is visible on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    id: LayerId,
    name: String,
    format: SourceFormat,
    source: Option<PathBuf>,
    visible: bool,
}

impl Layer {
    pub(crate) fn new(
        id: LayerId,
        name: String,
        format: SourceFormat,
        source: Option<PathBuf>,
    ) -> Self {
        Self {
            id,
            name,
            format,
            source,
            visible: true,
        }
    }

    /// Stable layer identifier.
    pub const fn id(&self) -> LayerId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename the layer. Empty names are ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if !name.is_empty() {
            self.name = name.to_owned();
        }
    }

    /// Format this layer was imported from.
    pub const fn format(&self) -> SourceFormat {
        self.format
    }

    /// Path of the imported source file, when the layer came from disk.
    pub fn source(&self) -> Option<&std::path::Path> {
        self.source.as_deref()
    }

    /// Set or clear the source path.
    pub fn set_source(&mut self, source: Option<PathBuf>) {
        self.source = source;
    }

    /// Whether the layer is visible on the canvas.
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Show or hide the layer.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}
