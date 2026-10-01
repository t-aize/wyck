//! Script-owned chart drawings. The drawing model and geometry are shared with manual tools.

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};
use serde_json::{Map as JsonMap, Number, Value};

use super::api::parse_color;
use super::run::{MAX_SCRIPT_DRAWINGS, Mode, with_run};
use super::series::Fallible;
use crate::drawing::model::{Drawing, Level, MAX_BRUSH_POINTS, Point, Style, Tool};
use crate::study::ScriptDrawing;

const MAX_SCRIPT_POINTS: usize = 20_000;
const MAX_OPTION_NODES: usize = 2_000;

pub fn register(engine: &mut Engine) {
    engine.register_fn("drawing_tools", || -> Array {
        Tool::ALL
            .iter()
            .map(|tool| {
                Dynamic::from(
                    serde_json::to_value(tool)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                )
            })
            .collect()
    });
    engine.register_fn("bar_point", |index: i64, price: Dynamic| {
        point_on_bar(index, price)
    });
    engine.register_fn("time_point", |time: i64, price: Dynamic| {
        point_at_time(time, price)
    });
    engine.register_fn("draw", |key: &str, tool: &str, points: Array| {
        draw(key, tool, points, Map::new())
    });
    engine.register_fn(
        "draw",
        |key: &str, tool: &str, points: Array, options: Map| draw(key, tool, points, options),
    );
}

fn number(value: &Dynamic) -> Option<f64> {
    value
        .as_float()
        .ok()
        .or_else(|| value.as_int().ok().map(|n| n as f64))
        .filter(|n| n.is_finite())
}

fn point_map(time: i64, price: Dynamic) -> Fallible<Map> {
    let price = number(&price).ok_or("point: price must be a finite number")?;
    let mut point = Map::new();
    point.insert("time".into(), Dynamic::from(time));
    point.insert("price".into(), Dynamic::from(price));
    Ok(point)
}

fn point_on_bar(index: i64, price: Dynamic) -> Fallible<Map> {
    let time = with_run(|run| {
        usize::try_from(index)
            .ok()
            .and_then(|i| run.columns.raw_time.get(i).copied())
    })?
    .ok_or("bar_point: index is outside the available bars")?;
    point_map(time, price)
}

fn point_at_time(time: i64, price: Dynamic) -> Fallible<Map> {
    point_map(time, price)
}

fn parse_point(value: Dynamic) -> Fallible<Point> {
    let map = value
        .try_cast::<Map>()
        .ok_or("draw: each point must be a map from bar_point or time_point")?;
    if map.len() != 2 || !map.contains_key("time") || !map.contains_key("price") {
        return Err("draw: a point must contain only time and price".into());
    }
    let time = map["time"]
        .as_int()
        .map_err(|_| "draw: point time must be Unix milliseconds as an integer")?;
    let price = number(&map["price"]).ok_or("draw: point price must be a finite number")?;
    Ok(Point { t: time, p: price })
}

fn json_value(value: Dynamic, depth: usize, remaining: &mut usize) -> Fallible<Value> {
    if depth > 8 || *remaining == 0 {
        return Err("draw: options are too large or deeply nested".into());
    }
    *remaining -= 1;
    if let Some(map) = value.clone().try_cast::<Map>() {
        let mut out = JsonMap::new();
        for (key, value) in map {
            out.insert(key.to_string(), json_value(value, depth + 1, remaining)?);
        }
        return Ok(Value::Object(out));
    }
    if let Some(array) = value.clone().try_cast::<Array>() {
        return Ok(Value::Array(
            array
                .into_iter()
                .map(|v| json_value(v, depth + 1, remaining))
                .collect::<Fallible<_>>()?,
        ));
    }
    if let Ok(n) = value.as_int() {
        return Ok(Value::Number(Number::from(n)));
    }
    if let Ok(n) = value.as_float() {
        return Number::from_f64(n)
            .map(Value::Number)
            .ok_or_else(|| "draw: options must contain finite numbers".into());
    }
    if let Ok(flag) = value.as_bool() {
        return Ok(Value::Bool(flag));
    }
    if let Ok(text) = value.into_string() {
        return Ok(Value::String(text));
    }
    Err("draw: an option contains an unsupported value".into())
}

fn style_key(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "width"
            | "dash"
            | "opacity"
            | "fill"
            | "fill_color"
            | "fill_opacity"
            | "extend_left"
            | "extend_right"
            | "text_size"
            | "text_color"
            | "bold"
            | "labels"
            | "middle"
            | "position"
            | "text_layout"
            | "caps"
            | "measure"
            | "profile"
            | "level_text"
            | "label_side"
            | "scale"
    )
}

fn normalize_colors(value: &mut Value, key: &str) -> Fallible<()> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                normalize_colors(child, key)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                normalize_colors(child, key)?;
            }
        }
        Value::String(text) if key == "color" || key.ends_with("_color") => {
            let color =
                parse_color(text).ok_or_else(|| format!("draw: \"{text}\" is not a color"))?;
            *value = Value::Number(Number::from(color));
        }
        _ => {}
    }
    Ok(())
}

fn validate_fields(value: &Value, example: &Value, path: &str) -> Fallible<()> {
    let (Some(fields), Some(allowed)) = (value.as_object(), example.as_object()) else {
        return Ok(());
    };
    for (name, child) in fields {
        let Some(expected) = allowed.get(name) else {
            return Err(format!("draw: unknown option \"{path}.{name}\"").into());
        };
        validate_fields(child, expected, &format!("{path}.{name}"))?;
    }
    Ok(())
}

fn style_schema() -> Value {
    let style = Style::default();
    let mut schema = serde_json::to_value(&style).unwrap_or_default();
    for (key, value) in [
        ("position", serde_json::to_value(&style.position)),
        ("text_layout", serde_json::to_value(style.text_layout)),
        ("caps", serde_json::to_value(style.caps)),
        ("measure", serde_json::to_value(style.measure)),
        ("profile", serde_json::to_value(style.profile)),
        ("level_text", serde_json::to_value(style.level_text)),
        ("label_side", serde_json::to_value(style.label_side)),
        ("scale", serde_json::to_value(style.scale)),
    ] {
        schema[key] = value.unwrap_or(Value::Null);
    }
    schema
}

fn draw(key: &str, tool: &str, points: Array, options: Map) -> Fallible<()> {
    if key.is_empty()
        || key.len() > 64
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("draw: id must be 1 to 64 ASCII letters, digits, _ or -".into());
    }
    let tool: Tool = serde_json::from_value(Value::String(tool.to_owned()))
        .map_err(|_| format!("draw: unknown tool \"{tool}\""))?;
    if tool == Tool::Unknown {
        return Err("draw: unknown tool".into());
    }
    if points.len() > MAX_BRUSH_POINTS {
        return Err(format!("draw: at most {MAX_BRUSH_POINTS} points per drawing").into());
    }
    let points: Vec<Point> = points
        .into_iter()
        .map(parse_point)
        .collect::<Fallible<_>>()?;
    let mut drawing = Drawing::new(0, tool, points);
    let mut json = serde_json::to_value(&drawing).map_err(|e| e.to_string())?;
    let mut style = json["style"].as_object().cloned().unwrap_or_default();
    let mut remaining = MAX_OPTION_NODES;
    for (key, value) in options {
        let key = key.as_str();
        if key == "style" {
            let Some(map) = value.try_cast::<Map>() else {
                return Err("draw: style must be a map".into());
            };
            for (style_key_name, style_value) in map {
                if !style_key(style_key_name.as_str()) {
                    return Err(format!("draw: unknown style option \"{style_key_name}\"").into());
                }
                style.insert(
                    style_key_name.to_string(),
                    json_value(style_value, 0, &mut remaining)?,
                );
            }
        } else if style_key(key) {
            style.insert(key.to_owned(), json_value(value, 0, &mut remaining)?);
        } else if matches!(
            key,
            "text" | "name" | "levels" | "timeframes" | "reverse" | "degree"
        ) {
            json[key] = json_value(value, 0, &mut remaining)?;
        } else {
            return Err(format!("draw: unknown option \"{key}\"").into());
        }
    }
    json["style"] = Value::Object(style);
    validate_fields(&json["style"], &style_schema(), "style")?;
    if let Some(levels) = json["levels"].as_array() {
        let mut schema = serde_json::to_value(Level::default()).unwrap_or_default();
        schema["width"] = Value::Null;
        schema["dash"] = Value::Null;
        for level in levels {
            validate_fields(level, &schema, "levels")?;
        }
    }
    normalize_colors(&mut json, "")?;
    drawing = serde_json::from_value(json)
        .map_err(|e| -> Box<EvalAltResult> { format!("draw: {e}").into() })?;
    if !drawing.is_valid() {
        return Err(format!(
            "draw: {} needs {} valid point(s)",
            tool.label(),
            tool.anchors()
        )
        .into());
    }
    drawing = drawing.normalized();
    with_run(|run| {
        if run.mode == Mode::Declare {
            return Ok(());
        }
        if run.drawings.iter().any(|d| d.key == key) {
            return Err(format!("draw: id \"{key}\" is used twice").into());
        }
        if run.drawings.len() >= MAX_SCRIPT_DRAWINGS {
            return Err(format!(
                "draw: an indicator may show at most {MAX_SCRIPT_DRAWINGS} drawings"
            )
            .into());
        }
        let used: usize = run.drawings.iter().map(|d| d.drawing.points.len()).sum();
        if used + drawing.points.len() > MAX_SCRIPT_POINTS {
            return Err(
                format!("draw: an indicator may use at most {MAX_SCRIPT_POINTS} points").into(),
            );
        }
        run.drawings.push(ScriptDrawing {
            key: key.to_owned(),
            drawing,
        });
        Ok(())
    })?
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::run::{Limits, Script};
    use super::*;
    use crate::study::StudyInput;

    fn bars() -> StudyInput {
        StudyInput {
            time: vec![1_700_000_000_000, 1_700_000_060_000],
            open: vec![100.0, 101.0],
            high: vec![102.0, 103.0],
            low: vec![99.0, 100.0],
            close: vec![101.0, 102.0],
            volume: vec![10.0, 12.0],
            day: vec![0, 0],
        }
    }

    fn compute(source: &str) -> Fallible<crate::study::StudyOutput> {
        let script = Script::compile(source).map_err(|p| format!("{p:?}"))?;
        script
            .compute(&bars(), &BTreeMap::new(), Limits::default(), None)
            .map(|done| done.output)
            .map_err(|p| format!("{p:?}").into())
    }

    #[test]
    fn every_manual_tool_can_be_drawn_from_rhai() {
        let mut source = String::from("indicator(#{ overlay: false }); if n > 0 {\n");
        for (i, tool) in Tool::ALL.iter().enumerate() {
            let name = serde_json::to_value(tool).unwrap();
            let name = name.as_str().unwrap();
            let points = (0..tool.anchors())
                .map(|j| {
                    format!(
                        "time_point({}, {})",
                        1_700_000_000_000_i64 + j as i64 * 60_000,
                        100 + j
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            source.push_str(&format!(
                "draw(\"d{i}\", \"{name}\", [{points}], #{{ color: \"red\" }});\n"
            ));
        }
        source.push('}');
        let output = compute(&source).unwrap();
        assert_eq!(output.drawings.len(), Tool::ALL.len());
        for (drawing, tool) in output.drawings.iter().zip(Tool::ALL) {
            assert_eq!(drawing.drawing.tool, tool);
            assert!(drawing.drawing.is_valid());
            assert_eq!(drawing.drawing.style.color, 0xef5350);
        }
    }

    #[test]
    fn a_drawing_only_script_is_valid_and_keeps_its_style() {
        let source = r##"indicator(#{ name: "Draw only" });
            if n > 0 {
                draw("trade", "long_position", [
                    bar_point(0, 100), bar_point(0, 99),
                    bar_point(1, 102), bar_point(1, 100)
                ], #{ style: #{ position: #{ show_pips: true,
                    target_color: "green" } }, text: "Setup" });
            }"##;
        let script = Script::compile(source).unwrap();
        assert!(
            script
                .warnings
                .iter()
                .all(|p| !p.message.contains("draws nothing"))
        );
        let output = compute(source).unwrap();
        assert_eq!(output.drawings.len(), 1);
        let drawing = &output.drawings[0].drawing;
        assert!(drawing.style.position.show_pips);
        assert_eq!(drawing.style.position.target_color, 0x26a69a);
        assert_eq!(drawing.text, "Setup");
    }

    #[test]
    fn bad_points_options_ids_and_limits_are_reported() {
        for (source, message) in [
            ("draw(\"x\", \"trend_line\", []);", "valid point"),
            (
                "draw(\"x\", \"trend_line\", [time_point(0, 1), time_point(1, 2)], #{ typo: true });",
                "unknown option",
            ),
            (
                "draw(\"x\", \"text\", [time_point(0, 1)], #{ position: #{ typo: true } });",
                "unknown option",
            ),
            ("draw(\"x\", \"bogus\", []);", "unknown tool"),
            (
                "let p = [time_point(0, 1)]; draw(\"x\", \"text\", p); draw(\"x\", \"text\", p);",
                "used twice",
            ),
        ] {
            let result = compute(source).unwrap_err().to_string();
            assert!(result.contains(message), "{result}");
        }
        let source =
            "if n > 0 { for i in 0..501 { draw(`d${i}`, \"text\", [bar_point(0, 100)]); } }";
        assert!(
            compute(source)
                .unwrap_err()
                .to_string()
                .contains("at most 500")
        );
    }
}
