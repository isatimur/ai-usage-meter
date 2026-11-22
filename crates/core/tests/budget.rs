use aium_core::budget::{forecast, BudgetStatus};

#[test]
fn on_pace_to_overspend() {
    // Day 10 of 30, $60 spent: projects to $180 against a $150 budget.
    let f = forecast(60.0, 150.0, 10, 30).unwrap();
    assert_eq!(f.status, BudgetStatus::Over);
    assert!((f.projected.unwrap() - 180.0).abs() < 1e-9);
}

#[test]
fn comfortably_under() {
    let f = forecast(30.0, 150.0, 10, 30).unwrap();
    assert_eq!(f.status, BudgetStatus::Ok);
}

#[test]
fn already_exceeded_wins_over_projection() {
    let f = forecast(160.0, 150.0, 5, 30).unwrap();
    assert_eq!(f.status, BudgetStatus::Exceeded);
}

#[test]
fn no_projection_in_first_two_days() {
    let f = forecast(100.0, 150.0, 2, 30).unwrap();
    assert_eq!(f.projected, None);
    assert_eq!(f.status, BudgetStatus::Ok);
}

#[test]
fn invalid_input_gives_none() {
    assert!(forecast(10.0, 0.0, 5, 30).is_none());
    assert!(forecast(10.0, -5.0, 5, 30).is_none());
    assert!(forecast(10.0, 100.0, 0, 30).is_none());
    assert!(forecast(-1.0, 100.0, 5, 30).is_none());
}
