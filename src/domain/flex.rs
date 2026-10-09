//! Reading integers that may arrive as a number or as text.
//!
//! The cTrader JSON gateway sends 64 bit values either way; every wire type reads them here.

use serde::Deserialize;
use serde::de::{Deserializer, Error};

#[derive(Deserialize)]
#[serde(untagged)]
enum Num {
    Int(i64),
    Uint(u64),
    Float(f64),
    Text(String),
}

fn to_i64<E: Error>(value: Num) -> Result<i64, E> {
    match value {
        Num::Int(v) => Ok(v),
        Num::Uint(v) => i64::try_from(v).map_err(|_| E::custom("number out of range")),
        Num::Float(v) if v.fract() == 0.0 && v.abs() < 9.0e15 => Ok(v as i64),
        Num::Float(_) => Err(E::custom("not a whole number")),
        Num::Text(text) => text
            .trim()
            .parse::<i64>()
            .map_err(|_| E::custom("not a whole number")),
    }
}

/// A required integer.
///
/// # Errors
///
/// When the value is neither a whole number nor text holding one.
pub fn int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    to_i64(Num::deserialize(d)?)
}

/// An optional integer (absent or null gives `None`).
///
/// # Errors
///
/// When the value is present but is not a whole number.
pub fn opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<Num>::deserialize(d)?.map(to_i64).transpose()
}

/// A list of integers.
///
/// # Errors
///
/// When an element is not a whole number.
pub fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    Vec::<Num>::deserialize(d)?
        .into_iter()
        .map(to_i64)
        .collect()
}
