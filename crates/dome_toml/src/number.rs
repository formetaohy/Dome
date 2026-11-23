pub(crate) fn printed(value: f32) -> f64 {
    assert!(
        value.is_finite(),
        "a TOML float holds {value}, which spans no finite number",
    );
    format!("{value}")
        .parse::<f64>()
        .expect("a printed f32 spans a finite f64")
}

pub(crate) fn whole(value: u64) -> i64 {
    i64::try_from(value)
        .unwrap_or_else(|_| panic!("a TOML integer holds {value}, which outruns 64 bits"))
}

pub(crate) fn narrow(value: f64, what: &str) -> f32 {
    assert!(
        value.is_finite() && value.abs() <= f64::from(f32::MAX),
        "{what} holds {value}, which spans no finite f32",
    );
    value as f32
}
