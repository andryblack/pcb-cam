//! Gerber RS-274X importer.
//!
//! The low-level tokenizer and aperture-macro parser are provided by the
//! `gerber_parser` crate. This module turns its command stream into the
//! flattened millimetre geometry used by the canvas and by CAM operations.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::io::BufReader;

use gerber_parser::gerber_types::{
    Aperture, ApertureMacro, CenterLinePrimitive, CirclePrimitive, Command, Coordinates, DCode,
    ExtendedCode, FunctionCode, GCode, ImagePolarity, InterpolationMode, MacroBoolean,
    MacroContent, MacroDecimal, MacroInteger, Operation, OutlinePrimitive,
    Polygon as GerberPolygon, PolygonPrimitive, QuadrantMode, Unit as GerberUnit,
    VectorLinePrimitive,
};

use super::ParseError;
use crate::Unit;
use crate::geometry::{
    Aperture as BoardAperture, ApertureShape, GerberLayer, Path, Point, Polarity, Primitive,
};

const MAX_WARNINGS: usize = 80;

/// Parse a Gerber RS-274X file.
pub fn parse_gerber(source: &str) -> Result<GerberLayer, ParseError> {
    let parse_result = gerber_parser::parse(BufReader::new(source.as_bytes()));
    let mut warnings: Vec<String> = Vec::new();
    let doc = match parse_result {
        Ok(doc) => doc,
        Err((doc, error)) => {
            warnings.push(error.to_string());
            doc
        }
    };

    for error in doc.errors().into_iter().take(32) {
        warnings.push(error.to_string());
    }

    let units = doc.units.map(convert_unit);
    if units.is_none() {
        warnings.push("no MO units declaration; assuming millimetres".into());
    }
    let mut converter = Converter::new(units.unwrap_or(Unit::Millimeter));
    converter.collect_macros(&doc);
    converter.collect_apertures(&doc);

    let mut state = State::default();
    for command in doc.commands() {
        if let Err(error) = converter.apply_command(command, &mut state) {
            converter.warn(error);
            if converter.warnings.len() >= MAX_WARNINGS {
                break;
            }
        }
    }
    converter.finish_state(&mut state);
    warnings.extend(converter.warnings.iter().cloned());

    if state.primitives.is_empty() {
        return Err(ParseError::new(
            0,
            if warnings.is_empty() {
                "Gerber file contains no drawable primitives".to_owned()
            } else {
                format!(
                    "Gerber file contains no drawable primitives: {}",
                    warnings.first().cloned().unwrap_or_default()
                )
            },
        ));
    }

    warnings.truncate(MAX_WARNINGS);
    Ok(GerberLayer {
        primitives: state.primitives,
        source_units: units,
        warnings,
    })
}

fn convert_unit(unit: GerberUnit) -> Unit {
    match unit {
        GerberUnit::Inches => Unit::Inch,
        GerberUnit::Millimeters => Unit::Millimeter,
    }
}

#[derive(Debug, Clone)]
enum ApertureDef {
    Standard(BoardAperture),
    Macro { name: String, arguments: Vec<f64> },
}

#[derive(Debug)]
struct State {
    current: Point,
    aperture: Option<ApertureDef>,
    interpolation: Interpolation,
    multi_quadrant: bool,
    absolute: bool,
    polarity: Polarity,
    region: bool,
    region_contours: Vec<Path>,
    region_points: Vec<Point>,
    stroke: Option<StrokeState>,
    primitives: Vec<Primitive>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            current: Point::ORIGIN,
            aperture: None,
            interpolation: Interpolation::Linear,
            multi_quadrant: false,
            absolute: true,
            polarity: Polarity::Dark,
            region: false,
            region_contours: Vec::new(),
            region_points: Vec::new(),
            stroke: None,
            primitives: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Interpolation {
    #[default]
    Linear,
    Clockwise,
    CounterClockwise,
}

#[derive(Debug)]
struct StrokeState {
    points: Vec<Point>,
    aperture: BoardAperture,
    polarity: Polarity,
}

struct Converter {
    factor: f64,
    macros: HashMap<String, ApertureMacro>,
    apertures: HashMap<i32, ApertureDef>,
    warnings: Vec<String>,
}

impl Converter {
    fn new(units: Unit) -> Self {
        Self {
            factor: match units {
                Unit::Millimeter => 1.0,
                Unit::Inch => 25.4,
            },
            macros: HashMap::new(),
            apertures: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, message: impl Into<String>) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(message.into());
        }
    }

    fn collect_macros(&mut self, doc: &gerber_parser::GerberDoc) {
        for command in doc.commands() {
            if let Command::ExtendedCode(ExtendedCode::ApertureMacro(macros)) = command {
                self.macros.insert(macros.name.clone(), macros.clone());
            }
        }
    }

    fn collect_apertures(&mut self, doc: &gerber_parser::GerberDoc) {
        for (code, aperture) in &doc.apertures {
            let def = match aperture {
                Aperture::Circle(circle) => {
                    self.standard_circle(circle.diameter, circle.hole_diameter)
                }
                Aperture::Rectangle(rect) => {
                    if rect.x <= 0.0 || rect.y <= 0.0 {
                        self.warn(format!("aperture D{code} has an empty rectangle"));
                        continue;
                    }
                    ApertureDef::Standard(BoardAperture {
                        shape: ApertureShape::Rectangle {
                            width: rect.x * self.factor,
                            height: rect.y * self.factor,
                        },
                        hole: rect.hole_diameter.map(|diameter| {
                            crate::geometry::ApertureHole::Circle {
                                diameter: diameter * self.factor,
                            }
                        }),
                    })
                }
                Aperture::Obround(rect) => ApertureDef::Standard(BoardAperture {
                    shape: ApertureShape::Obround {
                        width: rect.x * self.factor,
                        height: rect.y * self.factor,
                    },
                    hole: rect.hole_diameter.map(|diameter| {
                        crate::geometry::ApertureHole::Circle {
                            diameter: diameter * self.factor,
                        }
                    }),
                }),
                Aperture::Polygon(polygon) => self.standard_polygon(polygon),
                Aperture::Macro(name, arguments) => {
                    let args = arguments
                        .as_ref()
                        .map(|arguments| {
                            arguments
                                .iter()
                                .map(|argument| self.eval_decimal(argument, &HashMap::new()))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .transpose();
                    match args {
                        Ok(args) => ApertureDef::Macro {
                            name: name.clone(),
                            arguments: args.unwrap_or_default(),
                        },
                        Err(error) => {
                            self.warn(format!("aperture D{code}: {error}"));
                            continue;
                        }
                    }
                }
            };
            self.apertures.insert(*code, def);
        }
    }

    fn standard_circle(&self, diameter: f64, hole: Option<f64>) -> ApertureDef {
        ApertureDef::Standard(BoardAperture {
            shape: ApertureShape::Circle {
                diameter: diameter * self.factor,
            },
            hole: hole.map(|diameter| crate::geometry::ApertureHole::Circle {
                diameter: diameter * self.factor,
            }),
        })
    }

    fn standard_polygon(&self, polygon: &GerberPolygon) -> ApertureDef {
        ApertureDef::Standard(BoardAperture {
            shape: ApertureShape::Polygon {
                diameter: polygon.diameter * self.factor,
                vertices: u32::from(polygon.vertices),
                rotation_degrees: polygon.rotation.unwrap_or(0.0),
            },
            hole: polygon
                .hole_diameter
                .map(|diameter| crate::geometry::ApertureHole::Circle {
                    diameter: diameter * self.factor,
                }),
        })
    }

    fn apply_command(&mut self, command: &Command, state: &mut State) -> Result<(), String> {
        match command {
            Command::ExtendedCode(code) => self.apply_extended(code, state),
            Command::FunctionCode(code) => self.apply_function(code, state),
        }
    }

    fn apply_extended(&mut self, code: &ExtendedCode, state: &mut State) -> Result<(), String> {
        match code {
            ExtendedCode::Unit(unit) => {
                self.factor = match convert_unit(*unit) {
                    Unit::Millimeter => 1.0,
                    Unit::Inch => 25.4,
                };
            }
            ExtendedCode::LoadPolarity(polarity) => {
                self.flush_stroke(state);
                state.polarity = match polarity {
                    gerber_parser::gerber_types::Polarity::Dark => Polarity::Dark,
                    gerber_parser::gerber_types::Polarity::Clear => Polarity::Clear,
                };
            }
            ExtendedCode::ApertureDefinition(def) => {
                // Apertures are normally collected from `doc.apertures`; this
                // covers files where the parser stores a definition later.
                if !self.apertures.contains_key(&def.code) {
                    let converted = match &def.aperture {
                        Aperture::Circle(circle) => {
                            Some(self.standard_circle(circle.diameter, circle.hole_diameter))
                        }
                        Aperture::Rectangle(rect) => Some(ApertureDef::Standard(BoardAperture {
                            shape: ApertureShape::Rectangle {
                                width: rect.x * self.factor,
                                height: rect.y * self.factor,
                            },
                            hole: None,
                        })),
                        Aperture::Obround(rect) => Some(ApertureDef::Standard(BoardAperture {
                            shape: ApertureShape::Obround {
                                width: rect.x * self.factor,
                                height: rect.y * self.factor,
                            },
                            hole: None,
                        })),
                        Aperture::Polygon(polygon) => Some(self.standard_polygon(polygon)),
                        Aperture::Macro(name, arguments) => {
                            let args = arguments
                                .as_ref()
                                .map(|arguments| {
                                    arguments
                                        .iter()
                                        .map(|argument| {
                                            self.eval_decimal(argument, &HashMap::new())
                                        })
                                        .collect::<Result<Vec<_>, _>>()
                                })
                                .transpose()?;
                            Some(ApertureDef::Macro {
                                name: name.clone(),
                                arguments: args.unwrap_or_default(),
                            })
                        }
                    };
                    if let Some(converted) = converted {
                        self.apertures.insert(def.code, converted);
                    }
                }
            }
            ExtendedCode::StepAndRepeat(_) => {
                self.warn("step-and-repeat is not supported yet; repeated image omitted");
            }
            ExtendedCode::ApertureBlock(_) => {
                self.warn("aperture blocks are not supported yet");
            }
            ExtendedCode::LoadMirroring(_)
            | ExtendedCode::LoadRotation(_)
            | ExtendedCode::LoadScaling(_)
            | ExtendedCode::MirrorImage(_)
            | ExtendedCode::OffsetImage(_)
            | ExtendedCode::ScaleImage(_)
            | ExtendedCode::RotateImage(_) => {
                self.warn("image transformation attributes are ignored");
            }
            ExtendedCode::ImagePolarity(polarity) => {
                if *polarity == ImagePolarity::Negative {
                    self.warn("negative image polarity is not supported; drawn as positive");
                }
            }
            ExtendedCode::CoordinateFormat(_)
            | ExtendedCode::ApertureMacro(_)
            | ExtendedCode::FileAttribute(_)
            | ExtendedCode::ObjectAttribute(_)
            | ExtendedCode::ApertureAttribute(_)
            | ExtendedCode::DeleteAttribute(_)
            | ExtendedCode::AxisSelect(_)
            | ExtendedCode::ImageName(_) => {}
        }
        Ok(())
    }

    fn apply_function(&mut self, code: &FunctionCode, state: &mut State) -> Result<(), String> {
        match code {
            FunctionCode::DCode(dcode) => match dcode {
                DCode::SelectAperture(code) => {
                    self.flush_stroke(state);
                    state.aperture = self.apertures.get(code).cloned();
                    if state.aperture.is_none() {
                        self.warn(format!("aperture D{code} is selected but not defined"));
                    }
                }
                DCode::Operation(operation) => self.apply_operation(operation, state)?,
            },
            FunctionCode::GCode(gcode) => match gcode {
                GCode::InterpolationMode(mode) => {
                    state.interpolation = match mode {
                        InterpolationMode::Linear => Interpolation::Linear,
                        InterpolationMode::ClockwiseCircular => Interpolation::Clockwise,
                        InterpolationMode::CounterclockwiseCircular => {
                            Interpolation::CounterClockwise
                        }
                    };
                }
                GCode::RegionMode(enabled) => {
                    if *enabled {
                        self.flush_stroke(state);
                        state.region = true;
                        state.region_contours.clear();
                        state.region_points.clear();
                    } else {
                        self.finish_region(state);
                    }
                }
                GCode::QuadrantMode(mode) => {
                    state.multi_quadrant = matches!(mode, QuadrantMode::Multi);
                }
                GCode::CoordinateMode(mode) => {
                    state.absolute =
                        matches!(mode, gerber_parser::gerber_types::CoordinateMode::Absolute);
                }
                GCode::Unit(unit) => {
                    self.factor = match convert_unit(*unit) {
                        Unit::Millimeter => 1.0,
                        Unit::Inch => 25.4,
                    };
                }
                GCode::Comment(_) | GCode::SelectAperture => {}
            },
            FunctionCode::MCode(_) => {}
        }
        Ok(())
    }

    fn apply_operation(&mut self, operation: &Operation, state: &mut State) -> Result<(), String> {
        match operation {
            Operation::Move(coordinates) => {
                self.flush_stroke(state);
                if state.region {
                    self.finish_contour(state);
                }
                let next = self.absolute_point(coordinates.as_ref(), state);
                state.current = next;
                state.region_points.push(next);
            }
            Operation::Interpolate(coordinates, offset) => {
                let next = self.absolute_point(coordinates.as_ref(), state);
                let points = if state.interpolation == Interpolation::Linear {
                    vec![next]
                } else {
                    let (offset_x, offset_y) = offset.as_ref().map_or((0.0, 0.0), |offset| {
                        (
                            offset.x.map_or(0.0, |value| f64::from(value) * self.factor),
                            offset.y.map_or(0.0, |value| f64::from(value) * self.factor),
                        )
                    });
                    self.arc_points(
                        state.current,
                        next,
                        Point::new(offset_x, offset_y),
                        state.interpolation,
                        state.multi_quadrant,
                    )?
                };

                if state.region {
                    for point in points {
                        state.region_points.push(point);
                    }
                } else {
                    let aperture = match state.aperture.clone() {
                        Some(ApertureDef::Standard(aperture)) => aperture,
                        Some(ApertureDef::Macro { .. }) => {
                            self.warn(
                                "aperture macros cannot be used as stroke tools; using 0.1 mm",
                            );
                            BoardAperture::circle(0.1)
                        }
                        None => BoardAperture::circle(0.1),
                    };
                    let needs_new = state.stroke.as_ref().is_none_or(|stroke| {
                        stroke.aperture != aperture || stroke.polarity != state.polarity
                    });
                    if needs_new {
                        self.flush_stroke(state);
                        state.stroke = Some(StrokeState {
                            points: vec![state.current],
                            aperture,
                            polarity: state.polarity,
                        });
                    }
                    if let Some(stroke) = &mut state.stroke {
                        for point in points {
                            if stroke
                                .points
                                .last()
                                .is_none_or(|last| last.distance(point) > 1e-9)
                            {
                                stroke.points.push(point);
                            }
                        }
                    }
                }
                state.current = next;
            }
            Operation::Flash(coordinates) => {
                self.flush_stroke(state);
                let position = self.absolute_point(coordinates.as_ref(), state);
                state.current = position;
                self.flash(state, position);
            }
        }
        Ok(())
    }

    fn absolute_point(&self, coordinates: Option<&Coordinates>, state: &State) -> Point {
        let x = coordinates
            .and_then(|coordinates| coordinates.x)
            .map(|value| f64::from(value) * self.factor);
        let y = coordinates
            .and_then(|coordinates| coordinates.y)
            .map(|value| f64::from(value) * self.factor);
        if state.absolute {
            Point::new(x.unwrap_or(state.current.x), y.unwrap_or(state.current.y))
        } else {
            Point::new(
                state.current.x + x.unwrap_or(0.0),
                state.current.y + y.unwrap_or(0.0),
            )
        }
    }

    fn flash(&mut self, state: &mut State, position: Point) {
        let Some(def) = state.aperture.clone() else {
            self.warn("flash without a selected aperture");
            return;
        };
        match def {
            ApertureDef::Standard(aperture) => {
                state.primitives.push(Primitive::Flash {
                    position,
                    aperture,
                    rotation_degrees: 0.0,
                    polarity: state.polarity,
                });
            }
            ApertureDef::Macro { name, arguments } => {
                self.flash_macro(state, position, &name, &arguments);
            }
        }
    }

    fn flash_macro(&mut self, state: &mut State, position: Point, name: &str, arguments: &[f64]) {
        let Some(macros) = self.macros.get(name).cloned() else {
            self.warn(format!("aperture macro `{name}` is not defined"));
            return;
        };

        let mut variables = HashMap::new();
        for (index, argument) in arguments.iter().enumerate() {
            variables.insert(index as u32 + 1, *argument);
        }

        for content in &macros.content {
            match content {
                MacroContent::VariableDefinition(definition) => {
                    match eval_expression(&definition.expression, &variables) {
                        Ok(value) => {
                            variables.insert(definition.number, value);
                        }
                        Err(error) => self.warn(format!("macro `{name}`: {error}")),
                    }
                }
                MacroContent::Comment(_) => {}
                MacroContent::Circle(circle) => {
                    self.macro_circle(state, position, &variables, circle);
                }
                MacroContent::VectorLine(line) => {
                    self.macro_vector_line(state, position, &variables, line);
                }
                MacroContent::CenterLine(line) => {
                    self.macro_center_line(state, position, &variables, line);
                }
                MacroContent::Outline(outline) => {
                    self.macro_outline(state, position, &variables, outline);
                }
                MacroContent::Polygon(polygon) => {
                    self.macro_polygon(state, position, &variables, polygon);
                }
                MacroContent::Thermal(thermal) => {
                    let exposure =
                        evaluate_boolean(&MacroBoolean::Value(true), &variables).unwrap_or(true);
                    let center = self.macro_point(&thermal.center, &variables);
                    let outer = self
                        .eval_decimal(&thermal.outer_diameter, &variables)
                        .unwrap_or(0.0)
                        * self.factor;
                    let inner = self
                        .eval_decimal(&thermal.inner_diameter, &variables)
                        .unwrap_or(0.0)
                        * self.factor;
                    if outer > 0.0 {
                        state.primitives.push(Primitive::Flash {
                            position: position + center,
                            aperture: BoardAperture::circle(outer),
                            rotation_degrees: 0.0,
                            polarity: if exposure {
                                state.polarity
                            } else {
                                Polarity::Clear
                            },
                        });
                    }
                    if inner > 0.0 {
                        state.primitives.push(Primitive::Flash {
                            position: position + center,
                            aperture: BoardAperture::circle(inner),
                            rotation_degrees: 0.0,
                            polarity: Polarity::Clear,
                        });
                    }
                    self.warn(format!(
                        "macro `{name}`: thermal gaps are approximated by concentric circles"
                    ));
                }
                MacroContent::Moire(_) => {
                    self.warn(format!("macro `{name}`: moire primitive is not supported"));
                }
            }
        }
    }

    fn macro_circle(
        &mut self,
        state: &mut State,
        position: Point,
        variables: &HashMap<u32, f64>,
        circle: &CirclePrimitive,
    ) {
        let exposure = evaluate_boolean(&circle.exposure, variables).unwrap_or(true);
        let diameter = self
            .eval_decimal(&circle.diameter, variables)
            .unwrap_or(0.0)
            * self.factor;
        if diameter <= 0.0 {
            return;
        }
        let center = self.macro_point(&circle.center, variables);
        let angle = circle
            .angle
            .as_ref()
            .and_then(|angle| self.eval_decimal(angle, variables).ok())
            .unwrap_or(0.0);
        state.primitives.push(Primitive::Flash {
            position: position + center.rotated(angle.to_radians()),
            aperture: BoardAperture::circle(diameter),
            rotation_degrees: 0.0,
            polarity: if exposure {
                state.polarity
            } else {
                Polarity::Clear
            },
        });
    }

    fn macro_vector_line(
        &mut self,
        state: &mut State,
        position: Point,
        variables: &HashMap<u32, f64>,
        line: &VectorLinePrimitive,
    ) {
        let exposure = evaluate_boolean(&line.exposure, variables).unwrap_or(true);
        let width = self.eval_decimal(&line.width, variables).unwrap_or(0.0) * self.factor;
        if width <= 0.0 {
            return;
        }
        let angle = self
            .eval_decimal(&line.angle, variables)
            .unwrap_or(0.0)
            .to_radians();
        let start = self.macro_point(&line.start, variables).rotated(angle);
        let end = self.macro_point(&line.end, variables).rotated(angle);
        state.primitives.push(Primitive::Stroke {
            path: Path::new(vec![position + start, position + end]),
            aperture: BoardAperture::circle(width),
            polarity: if exposure {
                state.polarity
            } else {
                Polarity::Clear
            },
        });
    }

    fn macro_center_line(
        &mut self,
        state: &mut State,
        position: Point,
        variables: &HashMap<u32, f64>,
        line: &CenterLinePrimitive,
    ) {
        let exposure = evaluate_boolean(&line.exposure, variables).unwrap_or(true);
        let width = self
            .eval_decimal(&line.dimensions.0, variables)
            .unwrap_or(0.0)
            * self.factor;
        let height = self
            .eval_decimal(&line.dimensions.1, variables)
            .unwrap_or(0.0)
            * self.factor;
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let angle = self.eval_decimal(&line.angle, variables).unwrap_or(0.0);
        let center = self.macro_point(&line.center, variables);
        state.primitives.push(Primitive::Flash {
            position: position + center,
            aperture: BoardAperture {
                shape: ApertureShape::Rectangle { width, height },
                hole: None,
            },
            rotation_degrees: angle,
            polarity: if exposure {
                state.polarity
            } else {
                Polarity::Clear
            },
        });
    }

    fn macro_outline(
        &mut self,
        state: &mut State,
        position: Point,
        variables: &HashMap<u32, f64>,
        outline: &OutlinePrimitive,
    ) {
        let exposure = evaluate_boolean(&outline.exposure, variables).unwrap_or(true);
        let angle = self
            .eval_decimal(&outline.angle, variables)
            .unwrap_or(0.0)
            .to_radians();
        let mut points: Vec<Point> = outline
            .points
            .iter()
            .map(|point| position + self.macro_point(point, variables).rotated(angle))
            .collect();
        if points.len() < 3 {
            return;
        }
        if points.first().is_some_and(|first| {
            points
                .last()
                .is_some_and(|last| first.distance(*last) < 1e-9)
        }) {
            points.pop();
        }
        state.primitives.push(Primitive::Region {
            contours: vec![Path::closed(points)],
            polarity: if exposure {
                state.polarity
            } else {
                Polarity::Clear
            },
        });
    }

    fn macro_polygon(
        &mut self,
        state: &mut State,
        position: Point,
        variables: &HashMap<u32, f64>,
        polygon: &PolygonPrimitive,
    ) {
        let exposure = evaluate_boolean(&polygon.exposure, variables).unwrap_or(true);
        let vertices = evaluate_integer(&polygon.vertices, variables).unwrap_or(0);
        let diameter = self
            .eval_decimal(&polygon.diameter, variables)
            .unwrap_or(0.0)
            * self.factor;
        if vertices < 3 || diameter <= 0.0 {
            return;
        }
        let center = self.macro_point(&polygon.center, variables);
        let angle = self
            .eval_decimal(&polygon.angle, variables)
            .unwrap_or(0.0)
            .to_radians();
        let radius = diameter * 0.5;
        let points = (0..vertices)
            .map(|index| {
                let angle = angle + TAU * index as f64 / vertices as f64;
                position + center + Point::new(angle.cos() * radius, angle.sin() * radius)
            })
            .collect();
        state.primitives.push(Primitive::Region {
            contours: vec![Path::closed(points)],
            polarity: if exposure {
                state.polarity
            } else {
                Polarity::Clear
            },
        });
    }

    fn macro_point(
        &self,
        point: &(MacroDecimal, MacroDecimal),
        variables: &HashMap<u32, f64>,
    ) -> Point {
        Point::new(
            self.eval_decimal(&point.0, variables).unwrap_or(0.0) * self.factor,
            self.eval_decimal(&point.1, variables).unwrap_or(0.0) * self.factor,
        )
    }

    fn eval_decimal(
        &self,
        value: &MacroDecimal,
        variables: &HashMap<u32, f64>,
    ) -> Result<f64, String> {
        match value {
            MacroDecimal::Value(value) => Ok(*value),
            MacroDecimal::Variable(variable) => variables
                .get(variable)
                .copied()
                .ok_or_else(|| format!("undefined macro variable ${variable}")),
            MacroDecimal::Expression(expression) => eval_expression(expression, variables),
        }
    }

    fn arc_points(
        &mut self,
        start: Point,
        end: Point,
        offset: Point,
        direction: Interpolation,
        multi_quadrant: bool,
    ) -> Result<Vec<Point>, String> {
        if !start.is_finite() || !end.is_finite() || !offset.is_finite() {
            return Err("non-finite arc coordinate".into());
        }
        let center = start + offset;
        let radius = center.distance(start);
        if radius <= 1e-9 {
            return Ok(vec![end]);
        }

        let start_angle = (start.y - center.y).atan2(start.x - center.x);
        let end_angle = (end.y - center.y).atan2(end.x - center.x);
        let full_circle = start.distance(end) < 1e-7;
        let mut sweep = if direction == Interpolation::Clockwise {
            let mut sweep = start_angle - end_angle;
            while sweep <= 0.0 {
                sweep += TAU;
            }
            sweep
        } else {
            let mut sweep = end_angle - start_angle;
            while sweep <= 0.0 {
                sweep += TAU;
            }
            sweep
        };
        if full_circle {
            sweep = TAU;
        }
        if !multi_quadrant && sweep > PI * 0.5 + 1e-6 {
            self.warn("single-quadrant arc larger than 90 degrees; using computed sweep");
        }

        let segments = ((sweep / (PI / 18.0)).ceil() as u32).clamp(6, 720);
        let signed = if direction == Interpolation::Clockwise {
            -sweep
        } else {
            sweep
        };
        let mut points = Vec::with_capacity(segments as usize);
        for index in 1..=segments {
            let angle = start_angle + signed * index as f64 / segments as f64;
            let point = center + Point::new(angle.cos() * radius, angle.sin() * radius);
            if points
                .last()
                .is_none_or(|last: &Point| last.distance(point) > 1e-9)
            {
                points.push(point);
            }
        }
        if let Some(last) = points.last_mut() {
            *last = end;
        }
        Ok(points)
    }

    fn finish_contour(&mut self, state: &mut State) {
        if state.region_points.len() >= 3 {
            let mut points = std::mem::take(&mut state.region_points);
            if points.first().is_some_and(|first| {
                points
                    .last()
                    .is_some_and(|last| first.distance(*last) < 1e-9)
            }) {
                points.pop();
            }
            if points.len() >= 3 {
                state.region_contours.push(Path::closed(points));
            }
        } else {
            state.region_points.clear();
        }
    }

    fn finish_region(&mut self, state: &mut State) {
        self.finish_contour(state);
        state.region = false;
        if !state.region_contours.is_empty() {
            state.primitives.push(Primitive::Region {
                contours: std::mem::take(&mut state.region_contours),
                polarity: state.polarity,
            });
        }
    }

    fn flush_stroke(&mut self, state: &mut State) {
        if let Some(stroke) = state.stroke.take()
            && stroke.points.len() >= 2
        {
            state.primitives.push(Primitive::Stroke {
                path: Path::new(stroke.points),
                aperture: stroke.aperture,
                polarity: stroke.polarity,
            });
        }
    }

    fn finish_state(&mut self, state: &mut State) {
        self.flush_stroke(state);
        if state.region {
            self.finish_region(state);
        }
    }
}

fn evaluate_boolean(value: &MacroBoolean, variables: &HashMap<u32, f64>) -> Result<bool, String> {
    match value {
        MacroBoolean::Value(value) => Ok(*value),
        MacroBoolean::Variable(variable) => variables
            .get(variable)
            .map(|value| *value != 0.0)
            .ok_or_else(|| format!("undefined macro variable ${variable}")),
        MacroBoolean::Expression(expression) => Ok(eval_expression(expression, variables)? != 0.0),
    }
}

fn evaluate_integer(value: &MacroInteger, variables: &HashMap<u32, f64>) -> Result<u32, String> {
    let value = match value {
        MacroInteger::Value(value) => f64::from(*value),
        MacroInteger::Variable(variable) => variables
            .get(variable)
            .copied()
            .ok_or_else(|| format!("undefined macro variable ${variable}"))?,
        MacroInteger::Expression(expression) => eval_expression(expression, variables)?,
    };
    if !value.is_finite() || value < 0.0 {
        return Err("macro integer is not a non-negative number".into());
    }
    Ok(value.round() as u32)
}

fn eval_expression(expression: &str, variables: &HashMap<u32, f64>) -> Result<f64, String> {
    let mut parser = ExpressionParser {
        input: expression.as_bytes(),
        position: 0,
        variables,
    };
    let value = parser.parse_sum()?;
    parser.skip_whitespace();
    if parser.position != parser.input.len() {
        return Err(format!(
            "unexpected text in macro expression `{expression}`"
        ));
    }
    if !value.is_finite() {
        return Err(format!("non-finite macro expression `{expression}`"));
    }
    Ok(value)
}

struct ExpressionParser<'a> {
    input: &'a [u8],
    position: usize,
    variables: &'a HashMap<u32, f64>,
}

impl ExpressionParser<'_> {
    fn skip_whitespace(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        self.input.get(self.position).copied()
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn parse_sum(&mut self) -> Result<f64, String> {
        let mut value = self.parse_product()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    value += self.parse_product()?;
                }
                Some(b'-') => {
                    self.position += 1;
                    value -= self.parse_product()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_product(&mut self) -> Result<f64, String> {
        let mut value = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(b'x' | b'X' | b'*') => {
                    self.position += 1;
                    value *= self.parse_factor()?;
                }
                Some(b'/') => {
                    self.position += 1;
                    let divisor = self.parse_factor()?;
                    if divisor.abs() <= f64::EPSILON {
                        return Err("division by zero in macro expression".into());
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_factor(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        if self.consume(b'+') {
            return self.parse_factor();
        }
        if self.consume(b'-') {
            return Ok(-self.parse_factor()?);
        }
        if self.consume(b'(') {
            let value = self.parse_sum()?;
            if !self.consume(b')') {
                return Err("missing ')' in macro expression".into());
            }
            return Ok(value);
        }
        if self.consume(b'$') {
            let start = self.position;
            while self
                .input
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
            if start == self.position {
                return Err("missing variable number after '$'".into());
            }
            let number = std::str::from_utf8(&self.input[start..self.position])
                .ok()
                .and_then(|text| text.parse::<u32>().ok())
                .ok_or_else(|| "invalid macro variable number".to_owned())?;
            self.variables
                .get(&number)
                .copied()
                .ok_or_else(|| format!("undefined macro variable ${number}"))
        } else {
            let start = self.position;
            let mut has_dot = false;
            while let Some(byte) = self.input.get(self.position).copied() {
                if byte.is_ascii_digit() {
                    self.position += 1;
                } else if byte == b'.' && !has_dot {
                    has_dot = true;
                    self.position += 1;
                } else {
                    break;
                }
            }
            if start == self.position {
                return Err("expected a number in macro expression".into());
            }
            std::str::from_utf8(&self.input[start..self.position])
                .ok()
                .and_then(|text| text.parse::<f64>().ok())
                .ok_or_else(|| "invalid number in macro expression".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lines_and_flashes() {
        let source = "\
%FSLAX46Y46*%
%MOMM*%
%ADD10C,0.300*%
%ADD11C,1.600*%
D10*
X0Y0D02*
X10000000Y0D01*
X10000000Y5000000D01*
D11*
X20000000Y10000000D03*
M02*
";
        let layer = parse_gerber(source).unwrap();
        assert_eq!(layer.primitives.len(), 2);
        assert_eq!(layer.source_units, Some(Unit::Millimeter));
        let bounds = layer.bounds().unwrap();
        assert!(bounds.width() >= 20.8);
        assert!(bounds.height() >= 10.8);
    }

    #[test]
    fn parses_region_and_arc() {
        let source = "\
%FSLAX46Y46*%
%MOMM*%
%ADD10C,0.100*%
G36*
X0Y0D02*
X10000000Y0D01*
X10000000Y10000000D01*
X0Y10000000D01*
X0Y0D01*
G37*
G75*
D10*
X0Y0D02*
G03X10000000Y0I5000000J0D01*
M02*
";
        let layer = parse_gerber(source).unwrap();
        assert_eq!(layer.primitives.len(), 2);
        assert!(matches!(layer.primitives[0], Primitive::Region { .. }));
        assert!(matches!(layer.primitives[1], Primitive::Stroke { .. }));
    }

    #[test]
    fn parses_aperture_macro() {
        let source = "\
%FSLAX46Y46*%
%MOMM*%
%AMROUND*
1,1,1.0,0,0*
%
%ADD10ROUND*%
D10*
X0Y0D03*
M02*
";
        let layer = parse_gerber(source).unwrap();
        assert_eq!(layer.primitives.len(), 1);
        assert!(layer.bounds().is_some());
    }

    #[test]
    fn expression_parser_respects_precedence() {
        let mut variables = HashMap::new();
        variables.insert(1, 2.0);
        assert_eq!(eval_expression("$1+3x2", &variables).unwrap(), 8.0);
        assert_eq!(eval_expression("($1+3)x2", &variables).unwrap(), 10.0);
    }
}
