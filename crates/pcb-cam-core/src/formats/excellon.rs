//! Excellon drill and rout parser.
//!
//! Supports the common Excellon 2 subset emitted by KiCad, Eagle and
//! Gerbv: `M48` headers, metric/inch units, tool definitions, decimal or
//! fixed-format coordinates, drill hits, `G85` slots and `M15`/`M16`
//! routed slots.

use std::collections::BTreeMap;

use super::ParseError;
use crate::Unit;
use crate::geometry::{DrillHit, DrillSlot, DrillTool, ExcellonLayer, Point};

/// Parse an Excellon file.
pub fn parse_excellon(source: &str) -> Result<ExcellonLayer, ParseError> {
    let mut parser = Parser::new();
    for (index, raw_line) in source.lines().enumerate() {
        parser.line = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line == "M48" {
            parser.in_header = true;
            continue;
        }
        if line == "%" {
            parser.in_header = false;
            continue;
        }
        if line == "M30" || line == "M00" {
            break;
        }
        if parser.in_header {
            parser.parse_header_line(line);
        } else {
            parser.parse_body_line(line)?;
        }
    }

    parser.finish()
}

struct Parser {
    line: usize,
    in_header: bool,
    units: Option<Unit>,
    unit_warning: bool,
    coordinate_decimals: u32,
    coordinate_integer_digits: u32,
    leading_zero_omission: bool,
    absolute: bool,
    tools_raw: BTreeMap<u32, f64>,
    current_tool: Option<u32>,
    current: Point,
    holes: Vec<DrillHit>,
    slots: Vec<DrillSlot>,
    routing: bool,
    warnings: Vec<String>,
}

impl Parser {
    fn new() -> Self {
        Self {
            line: 0,
            in_header: false,
            units: None,
            unit_warning: false,
            coordinate_decimals: 3,
            coordinate_integer_digits: 3,
            leading_zero_omission: true,
            absolute: true,
            tools_raw: BTreeMap::new(),
            current_tool: None,
            current: Point::ORIGIN,
            holes: Vec::new(),
            slots: Vec::new(),
            routing: false,
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, message: impl Into<String>) {
        if self.warnings.len() < 64 {
            self.warnings
                .push(format!("line {}: {}", self.line, message.into()));
        }
    }

    fn parse_header_line(&mut self, line: &str) {
        let upper = line.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("METRIC") {
            self.units = Some(Unit::Millimeter);
            self.coordinate_decimals = 3;
            self.coordinate_integer_digits = 3;
            self.parse_unit_options(rest);
        } else if let Some(rest) = upper.strip_prefix("INCH") {
            self.units = Some(Unit::Inch);
            self.coordinate_decimals = 4;
            self.coordinate_integer_digits = 2;
            self.parse_unit_options(rest);
        } else if let Some(code) = upper.strip_prefix('T') {
            self.parse_tool_definition(code);
        } else if upper.starts_with("FMAT") || upper.starts_with("ICI") || upper.starts_with("VER")
        {
            // Header metadata.
        } else if upper.starts_with("M71") {
            self.units = Some(Unit::Millimeter);
            self.coordinate_decimals = 3;
            self.coordinate_integer_digits = 3;
        } else if upper.starts_with("M72") {
            self.units = Some(Unit::Inch);
            self.coordinate_decimals = 4;
            self.coordinate_integer_digits = 2;
        } else if upper.starts_with("G90") || upper.starts_with("G05") {
            // Body commands sometimes appear before the header terminator.
        } else if !upper.starts_with(";") {
            self.warn(format!("unsupported header statement `{line}`"));
        }
    }

    fn parse_unit_options(&mut self, rest: &str) {
        for option in rest.trim_start_matches(',').split(',') {
            let option = option.trim();
            if option == "TZ" {
                // Trailing zeros are kept, so leading zeros are omitted.
                self.leading_zero_omission = true;
            } else if option == "LZ" {
                // Leading zeros are kept, so trailing zeros are omitted.
                self.leading_zero_omission = false;
            } else if option.contains('.') {
                let parts: Vec<&str> = option.split('.').collect();
                if parts.len() == 2 {
                    self.coordinate_integer_digits = parts[0].len() as u32;
                    self.coordinate_decimals = parts[1].len() as u32;
                }
            }
        }
    }

    fn parse_tool_definition(&mut self, body: &str) {
        let Some((number_text, rest)) = body.split_once('C') else {
            self.warn("tool definition without a diameter");
            return;
        };
        let Ok(number) = number_text.trim().parse::<u32>() else {
            self.warn("invalid tool number");
            return;
        };
        let diameter_text = rest
            .split(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+'))
            .next()
            .unwrap_or_default();
        match diameter_text.parse::<f64>() {
            Ok(diameter) if diameter.is_finite() && diameter > 0.0 => {
                self.tools_raw.insert(number, diameter);
            }
            _ => self.warn(format!("invalid diameter for tool T{number}")),
        }
    }

    fn parse_body_line(&mut self, line: &str) -> Result<(), ParseError> {
        let upper = line.to_ascii_uppercase();
        if upper == "G90" {
            self.absolute = true;
            return Ok(());
        }
        if upper == "G91" {
            self.absolute = false;
            self.warn("incremental Excellon coordinates are supported with limited validation");
            return Ok(());
        }
        if upper == "M15" {
            self.routing = true;
            return Ok(());
        }
        if upper == "M16" {
            self.routing = false;
            return Ok(());
        }
        if upper == "G05" || upper == "M71" || upper == "M72" {
            if upper == "M71" {
                self.units = Some(Unit::Millimeter);
                self.coordinate_decimals = 3;
                self.coordinate_integer_digits = 3;
            } else if upper == "M72" {
                self.units = Some(Unit::Inch);
                self.coordinate_decimals = 4;
                self.coordinate_integer_digits = 2;
            }
            return Ok(());
        }
        if let Some(number) = upper.strip_prefix('T')
            && number.chars().all(|ch| ch.is_ascii_digit())
        {
            let tool = number
                .parse::<u32>()
                .map_err(|_| ParseError::new(self.line, "invalid tool selection"))?;
            self.current_tool = Some(tool);
            return Ok(());
        }

        let groups = split_coordinate_groups(line);
        if groups.is_empty() {
            if !upper.starts_with('G') && !upper.starts_with('M') {
                self.warn(format!("unsupported statement `{line}`"));
            }
            return Ok(());
        }

        let factor = match self.units {
            Some(Unit::Inch) => 25.4,
            _ => {
                if self.units.is_none() && !self.unit_warning {
                    self.unit_warning = true;
                    self.warn("no units declared; assuming millimetres");
                }
                1.0
            }
        };
        let tool = self
            .current_tool
            .ok_or_else(|| ParseError::new(self.line, "coordinates before a tool selection"))?;
        let diameter = self
            .tools_raw
            .get(&tool)
            .map(|diameter| diameter * factor)
            .ok_or_else(|| ParseError::new(self.line, format!("tool T{tool} has no definition")))?;

        let slot = upper.contains("G85");
        let rapid = upper.contains("G00");
        let first = self.group_point(groups.first().map(Vec::as_slice).unwrap_or(&[]), factor)?;

        if rapid && !slot {
            // G00 only positions the tool. Without this, routed slots are
            // misread as a drill hit at every end point.
            self.current = first;
            return Ok(());
        }

        if slot {
            let second =
                self.group_point(groups.get(1).map(Vec::as_slice).unwrap_or(&[]), factor)?;
            self.slots.push(DrillSlot {
                start: first,
                end: second,
                tool,
                diameter_mm: diameter,
            });
            self.current = second;
        } else if self.routing {
            // A coordinate line between M15 and M16 is a routed slot. Some
            // files emit one G01 segment, others several.
            let mut start = self.current;
            for group in &groups {
                let end = self.group_point(group, factor)?;
                self.slots.push(DrillSlot {
                    start,
                    end,
                    tool,
                    diameter_mm: diameter,
                });
                start = end;
            }
            self.current = start;
        } else {
            self.holes.push(DrillHit {
                position: first,
                tool,
                diameter_mm: diameter,
            });
            self.current = first;
            if groups.len() > 1 {
                self.warn("multiple coordinate groups without G85; only the first was used");
            }
        }
        Ok(())
    }

    fn group_point(&self, values: &[(char, String)], factor: f64) -> Result<Point, ParseError> {
        let x = values
            .iter()
            .rev()
            .find(|(axis, _)| *axis == 'X')
            .map(|(_, value)| value.as_str());
        let y = values
            .iter()
            .rev()
            .find(|(axis, _)| *axis == 'Y')
            .map(|(_, value)| value.as_str());

        let parse_axis = |value: Option<&str>, current: f64| -> Result<f64, ParseError> {
            match value {
                Some(raw) => {
                    let value = self.parse_coordinate(raw)?;
                    let millimetres = value * factor;
                    if self.absolute {
                        Ok(millimetres)
                    } else {
                        Ok(current + millimetres)
                    }
                }
                None => Ok(current),
            }
        };

        Ok(Point::new(
            parse_axis(x, self.current.x)?,
            parse_axis(y, self.current.y)?,
        ))
    }

    fn parse_coordinate(&self, raw: &str) -> Result<f64, ParseError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(ParseError::new(self.line, "empty coordinate"));
        }
        if raw.contains('.') {
            return raw
                .parse::<f64>()
                .map_err(|_| ParseError::new(self.line, format!("invalid coordinate `{raw}`")));
        }

        let negative = raw.starts_with('-');
        let digits = raw.trim_start_matches(['-', '+']);
        if !digits.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(ParseError::new(
                self.line,
                format!("invalid coordinate `{raw}`"),
            ));
        }
        let total = (self.coordinate_integer_digits + self.coordinate_decimals) as usize;
        let padded = if self.leading_zero_omission {
            format!("{digits:0>total$}")
        } else {
            format!("{digits:0<total$}")
        };
        let value = padded
            .parse::<f64>()
            .map_err(|_| ParseError::new(self.line, format!("invalid coordinate `{raw}`")))?
            / 10_f64.powi(self.coordinate_decimals as i32);
        Ok(if negative { -value } else { value })
    }

    fn finish(mut self) -> Result<ExcellonLayer, ParseError> {
        if self.routing {
            self.warn("routing started with M15 but never stopped with M16");
        }
        if self.holes.is_empty() && self.slots.is_empty() {
            return Err(ParseError::new(
                self.line,
                "Excellon file contains no drill hits or slots",
            ));
        }
        let factor = match self.units {
            Some(Unit::Inch) => 25.4,
            _ => 1.0,
        };
        let tools = self
            .tools_raw
            .iter()
            .map(|(&number, &diameter)| DrillTool {
                number,
                diameter_mm: diameter * factor,
            })
            .collect();
        Ok(ExcellonLayer {
            tools,
            holes: self.holes,
            slots: self.slots,
            source_units: self.units,
            warnings: self.warnings,
        })
    }
}

/// Split a body line into coordinate groups, one per `G85` segment.
fn split_coordinate_groups(line: &str) -> Vec<Vec<(char, String)>> {
    let mut groups = vec![Vec::new()];
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            'X' | 'Y' => {
                let mut raw = String::new();
                while let Some(next) = chars.peek() {
                    if next.is_ascii_digit() || matches!(next, '.' | '-' | '+') {
                        raw.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !raw.is_empty()
                    && let Some(group) = groups.last_mut()
                {
                    group.push((ch, raw));
                }
            }
            'G' => {
                let mut raw = String::new();
                while let Some(next) = chars.peek() {
                    if next.is_ascii_digit() {
                        raw.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if raw == "85" {
                    groups.push(Vec::new());
                }
            }
            _ => {}
        }
    }
    groups.retain(|group| !group.is_empty());
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_metric_decimal_drills() {
        let source = "\
M48
METRIC
T1C0.8
T2C1.2
%
G90
T1
X10.0Y20.0
X15.5Y20.0
T2
X30.0Y40.0
M30
";
        let layer = parse_excellon(source).unwrap();
        assert_eq!(layer.tools.len(), 2);
        assert_eq!(layer.holes.len(), 3);
        assert!((layer.tools[0].diameter_mm - 0.8).abs() < 1e-9);
        assert_eq!(layer.holes[0].position, Point::new(10.0, 20.0));
        assert_eq!(layer.holes[2].diameter_mm, 1.2);
    }

    #[test]
    fn parses_fixed_format_coordinates_with_leading_zero_omission() {
        let source = "\
M48
METRIC,TZ,000.000
T1C0.4
%
T1
X101359Y62655
";
        let layer = parse_excellon(source).unwrap();
        assert_eq!(layer.holes[0].position, Point::new(101.359, 62.655));
    }

    #[test]
    fn parses_inch_units() {
        let source = "\
M48
INCH
T1C0.04
%
T1
X1.5Y2.5
";
        let layer = parse_excellon(source).unwrap();
        assert!((layer.holes[0].diameter_mm - 1.016).abs() < 1e-9);
        assert!((layer.holes[0].position.x - 38.1).abs() < 1e-9);
    }

    #[test]
    fn parses_m15_m16_routed_slots() {
        let source = "\
M48
METRIC
T1C1.0
%
G90
G05
T1
G00X10.0Y10.0
M15
G01X20.0Y10.0
M16
M30
";
        let layer = parse_excellon(source).unwrap();
        assert!(layer.holes.is_empty());
        assert_eq!(layer.slots.len(), 1);
        assert_eq!(layer.slots[0].start, Point::new(10.0, 10.0));
        assert_eq!(layer.slots[0].end, Point::new(20.0, 10.0));
        assert_eq!(layer.slots[0].diameter_mm, 1.0);
    }

    #[test]
    fn parses_g85_slots() {
        let source = "\
M48
METRIC
T1C1.0
%
T1
X10.0Y10.0G85X20.0Y10.0
";
        let layer = parse_excellon(source).unwrap();
        assert_eq!(layer.slots.len(), 1);
        assert_eq!(layer.slots[0].start, Point::new(10.0, 10.0));
        assert_eq!(layer.slots[0].end, Point::new(20.0, 10.0));
    }
}
