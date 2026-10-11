use super::*;

#[test]
fn style_inputs_follow_kind_name_and_explicit_section() {
    let mut input = InputDecl {
        key: "bullish_fvg".to_owned(),
        label: "Bullish FVG".to_owned(),
        kind: InputType::Color,
        default: 0.0,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        options: Vec::new(),
        section: None,
        text: String::new(),
        group: None,
        tooltip: None,
    };
    assert!(input.in_style());
    input.section = Some(InputSection::Inputs);
    assert!(!input.in_style());
    input.key = "fill_opacity".to_owned();
    input.kind = InputType::Float;
    input.section = None;
    assert!(input.in_style());
    input.section = Some(InputSection::Inputs);
    assert!(!input.in_style());
    input.section = Some(InputSection::Style);
    assert!(input.in_style());
}

fn bars(n: usize) -> StudyInput {
    let close: Vec<f64> = (0..n)
        .map(|i| 100.0 + (i as f64 * 0.7).sin() * 5.0)
        .collect();
    StudyInput {
        time: (0..n as i64).map(|i| i * 60_000).collect(),
        open: close.iter().map(|c| c - 0.5).collect(),
        high: close.iter().map(|c| c + 1.0).collect(),
        low: close.iter().map(|c| c - 1.0).collect(),
        volume: vec![10.0; n],
        day: vec![0; n],
        close,
    }
}

#[test]
fn a_script_that_does_not_compile_says_where() {
    let problems = Script::compile("let a = 1;\nlet b = ;\n").unwrap_err();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, 2);
}

#[test]
fn an_unknown_name_is_found_before_the_script_runs() {
    let problems = Script::compile("plot(\"x\", clos);").unwrap_err();
    assert_eq!(problems[0].line, 1);
    assert!(
        problems[0].message.contains("clos"),
        "{}",
        problems[0].message
    );
}

#[test]
fn the_declaration_run_learns_the_inputs_and_plots_without_bars() {
    let script = Script::compile(
        "indicator(#{ name: \"Two lines\", short: \"TL\", overlay: true });\n\
         let len = input_int(\"length\", 20, #{ min: 2, max: 200 });\n\
         plot(\"fast\", sma(close, len));\n\
         plot(\"slow\", sma(close, len * 2), #{ color: \"#ff9800\", width: 2 });",
    )
    .unwrap();
    let d = &script.declaration;
    assert_eq!(d.meta.name.as_deref(), Some("Two lines"));
    assert!(d.meta.overlay);
    assert_eq!(d.inputs.len(), 1);
    assert_eq!(
        (d.inputs[0].key.as_str(), d.inputs[0].default),
        ("length", 20.0)
    );
    assert_eq!(d.plots.len(), 2);
    assert_eq!(d.plots[1].color, 0xff9800);
}

#[test]
fn computing_uses_the_values_the_user_chose() {
    let script = Script::compile(
        "let len = input_int(\"length\", 3, #{ min: 1, max: 50 });\n plot(\"ma\", sma(close, len));",
    )
    .unwrap();
    let input = bars(30);
    let mut values = BTreeMap::new();
    let with_default = script
        .compute(&input, &values, Limits::default(), None)
        .unwrap();
    values.insert("length".to_owned(), 5.0);
    let with_five = script
        .compute(&input, &values, Limits::default(), None)
        .unwrap();
    let (a, b) = (
        &with_default.output.plots[0].values,
        &with_five.output.plots[0].values,
    );
    assert!(a[1].is_nan() && a[2].is_finite());
    assert!(b[3].is_nan() && b[4].is_finite());
}

#[test]
fn a_runaway_script_is_stopped() {
    let script = Script::compile("let i = 0;\n while i < n * 100_000_000 { i += 1; }").unwrap();
    let limits = Limits {
        operations: 50_000,
        time: Duration::from_secs(5),
    };
    let problems = script
        .compute(&bars(10), &BTreeMap::new(), limits, None)
        .unwrap_err();
    assert!(
        problems[0].message.contains("operations"),
        "{}",
        problems[0].message
    );
}

#[test]
fn a_run_can_be_cancelled() {
    let script = Script::compile("let i = 0;\n while i < n * 100_000_000 { i += 1; }").unwrap();
    let cancel = Arc::new(AtomicBool::new(true));
    let limits = Limits {
        operations: u64::MAX,
        time: Duration::from_secs(60),
    };
    let started = Instant::now();
    let problems = script
        .compute(&bars(10), &BTreeMap::new(), limits, Some(cancel))
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        problems[0].message.contains("stopped"),
        "{}",
        problems[0].message
    );
}

#[test]
fn a_mistake_that_the_bars_cause_says_the_line() {
    let script = Script::compile(
        "plot(\"a\", close);
let big = series(n * 1_000_000, 0.0);",
    )
    .unwrap();
    let problems = script
        .compute(&bars(10), &BTreeMap::new(), Limits::default(), None)
        .unwrap_err();
    assert_eq!(problems[0].line, 2);
    assert!(
        problems[0].message.contains("too long"),
        "{}",
        problems[0].message
    );
}

#[test]
fn a_mistake_in_a_call_is_found_while_declaring() {
    let problems = Script::compile(
        "plot(\"a\", close);
let x = sma(close, 0);",
    )
    .unwrap_err();
    assert_eq!(problems[0].line, 2);
    assert!(
        problems[0].message.contains("period"),
        "{}",
        problems[0].message
    );
}

#[test]
fn what_a_script_prints_is_kept_for_the_console() {
    let script = Script::compile("print(\"hello\");\nplot(\"a\", close);").unwrap();
    let done = script
        .compute(&bars(5), &BTreeMap::new(), Limits::default(), None)
        .unwrap();
    assert_eq!(done.log, vec!["hello".to_owned()]);
}

#[test]
fn a_script_cannot_load_a_file_or_evaluate_text() {
    assert!(Script::compile("import \"other\" as o;").is_err());
    assert!(Script::compile("eval(\"1 + 1\");").is_err());
}
