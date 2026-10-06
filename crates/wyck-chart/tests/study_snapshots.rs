//! Pins the numeric output of every built-in study on one fixed bar set, so a move or a
//! refactor that changes a value fails here.
//!
//! After an intended change, regenerate with
//! `UPDATE_SNAPSHOTS=1 cargo test -p wyck-chart --test study_snapshots` and review the diff.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a panic is the failure report"
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use wyck_chart::study::{StudyConfig, StudyInput, StudyKind, compute};

const BARS: usize = 500;
const SAMPLES: [usize; 8] = [0, 60, 120, 249, 250, 301, 420, 499];

/// Raw prices around 110000 (the server's integers), a 30 minute gap at bar 250 and one very
/// large bar at 300. Only integer and basic arithmetic, so the data is the same on every platform.
fn bars() -> StudyInput {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as f64) / (1u64 << 31) as f64
    };

    let mut input = StudyInput::default();
    let mut previous = 110_000.0;
    for i in 0..BARS {
        let wave = ((i % 100) as f64 - 50.0).abs() * 30.0;
        let close = 110_000.0 + wave + i as f64 * 2.0 + (next() - 0.5) * 300.0;
        let open = if i == 0 { close } else { previous };
        let spread = 100.0 + next() * 250.0;
        let mut high = open.max(close) + spread * next();
        let low = open.min(close) - spread * next();
        let mut volume = (50.0 + next() * 100.0).floor();
        if i == 300 {
            high += 4_000.0;
            volume *= 20.0;
        }
        let gap = if i >= 250 { 30 * 60_000 } else { 0 };
        let time = 1_700_000_000_000 + i as i64 * 60_000 + gap;

        input.time.push(time);
        input.open.push(open);
        input.high.push(high);
        input.low.push(low);
        input.close.push(close);
        input.volume.push(volume);
        input.day.push(time.div_euclid(86_400_000));
        previous = close;
    }
    input
}

fn number(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{value:.6e}")
    }
}

fn render() -> String {
    let input = bars();
    let mut text = String::new();
    for kind in StudyKind::ALL {
        let out = compute(&StudyConfig::new(kind), &input);
        let name = serde_json::to_value(kind).expect("a study kind serializes");
        writeln!(
            text,
            "[{}] plots={} fills={} drawings={} levels={:?} band={:?}",
            name.as_str().unwrap_or("?"),
            out.plots.len(),
            out.fills.len(),
            out.drawings.len(),
            out.levels.iter().map(|v| number(*v)).collect::<Vec<_>>(),
            out.band.map(|(a, b)| (number(a), number(b))),
        )
        .expect("writing to a string");
        for fill in &out.fills {
            writeln!(
                text,
                "  fill a={} b={} alpha={}",
                fill.a, fill.b, fill.alpha
            )
            .expect("writing to a string");
        }
        for plot in &out.plots {
            let finite = plot.values.iter().filter(|v| v.is_finite()).count();
            let first = plot.values.iter().position(|v| v.is_finite());
            let sum: f64 = plot.values.iter().filter(|v| v.is_finite()).sum();
            let weighted: f64 = plot
                .values
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
                .map(|(i, v)| (i + 1) as f64 * v)
                .sum();
            let samples: Vec<String> = SAMPLES
                .iter()
                .map(|&i| format!("{i}:{}", number(plot.values[i])))
                .collect();
            let ups = plot.up.as_ref().map(|u| u.iter().filter(|b| **b).count());
            writeln!(
                text,
                "  {} kind={:?} offset={} len={} finite={} first={:?} ups={:?} sum={} wsum={} at=[{}]",
                plot.key,
                plot.kind,
                plot.offset,
                plot.values.len(),
                finite,
                first,
                ups,
                number(sum),
                number(weighted),
                samples.join(" "),
            )
            .expect("writing to a string");
        }
    }
    text
}

#[test]
fn every_study_keeps_its_numbers() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/studies.txt");
    let actual = render();

    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the snapshot has a folder"))
            .expect("create the snapshot folder");
        std::fs::write(&path, &actual).expect("write the snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with UPDATE_SNAPSHOTS=1", path.display()))
        .replace("\r\n", "\n");
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        panic!(
            "study output changed at line {} of {}\n  expected: {}\n  actual:   {}\nRun with UPDATE_SNAPSHOTS=1 if the change is intended.",
            line + 1,
            path.display(),
            expected.lines().nth(line).unwrap_or("<end>"),
            actual.lines().nth(line).unwrap_or("<end>"),
        );
    }
}

#[test]
fn the_snapshot_covers_every_study() {
    assert_eq!(StudyKind::ALL.len(), 50);
    let text = render();
    assert_eq!(
        text.lines().filter(|l| l.starts_with('[')).count(),
        StudyKind::ALL.len()
    );
}
