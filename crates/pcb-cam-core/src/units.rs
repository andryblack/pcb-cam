//! Length units used by PCB fabrication data.

use serde::{Deserialize, Serialize};

/// A length unit used by a project and by imported fabrication files.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Unit {
    /// Millimetres.
    #[default]
    Millimeter,
    /// Imperial inches.
    Inch,
}

impl Unit {
    /// Number of millimetres in one inch.
    pub const MM_PER_INCH: f64 = 25.4;

    /// Short symbol shown in the UI: `mm` or `in`.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Inch => "in",
        }
    }

    /// Human-readable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Millimeter => "Millimetres",
            Self::Inch => "Inches",
        }
    }

    /// Convert a value expressed in this unit to millimetres.
    pub fn to_mm(self, value: f64) -> f64 {
        match self {
            Self::Millimeter => value,
            Self::Inch => value * Self::MM_PER_INCH,
        }
    }

    /// Convert a value expressed in millimetres to this unit.
    pub fn from_mm(self, millimetres: f64) -> f64 {
        match self {
            Self::Millimeter => millimetres,
            Self::Inch => millimetres / Self::MM_PER_INCH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Unit;

    #[test]
    fn inch_conversion_round_trips() {
        let mm = Unit::Inch.to_mm(2.5);
        assert!((mm - 63.5).abs() < f64::EPSILON);
        assert!((Unit::Inch.from_mm(mm) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn millimetre_conversion_is_identity() {
        assert_eq!(Unit::Millimeter.to_mm(12.0), 12.0);
        assert_eq!(Unit::Millimeter.from_mm(12.0), 12.0);
    }

    #[test]
    fn symbols_are_stable() {
        assert_eq!(Unit::Millimeter.symbol(), "mm");
        assert_eq!(Unit::Inch.symbol(), "in");
    }
}
