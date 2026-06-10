use gravity_types::{deviation_bps, Fixed, GravityError, Price, Quantity, SCALE};

#[test]
fn fixed_checked_math_reports_overflow() {
    let max = Fixed::raw(i128::MAX);
    assert!(matches!(max.checked_add(Fixed::ONE), Err(GravityError::Overflow)));
    assert!(matches!(Fixed::ONE.checked_div(Fixed::ZERO), Err(GravityError::DivisionByZero)));
}

#[test]
fn fixed_parser_is_deterministic_and_truncates_to_scale() {
    let value: Fixed = "1.123456789".parse().unwrap();
    assert_eq!(value.as_raw(), SCALE + 123_456);
    assert_eq!(value.to_string(), "1.123456");
}

#[test]
fn prices_and_quantities_must_be_positive() {
    assert!(Price::new(Fixed::ZERO).is_err());
    assert!(Quantity::new(Fixed::raw(-1)).is_err());
}

#[test]
fn deviation_bps_is_symmetric_for_equal_magnitude_moves() {
    let reference = Price::new(Fixed::from_units(100)).unwrap();
    let up = Price::new(Fixed::from_units(110)).unwrap();
    let down = Price::new(Fixed::from_units(90)).unwrap();
    assert_eq!(deviation_bps(reference, up), 1000);
    assert_eq!(deviation_bps(reference, down), 1000);
}
