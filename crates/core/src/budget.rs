//! Monthly budget check: "at this pace, will the month end above my budget?"
//! The cost is the API-price estimate, not a bill.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    Ok,
    /// The month is on pace to end above the budget.
    Over,
    /// Spending has already passed the budget.
    Exceeded,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BudgetForecast {
    pub status: BudgetStatus,
    pub spent: f64,
    pub budget: f64,
    /// Projected month-end cost. `None` in the first 3 days, when a line through so little data says nothing.
    pub projected: Option<f64>,
}

/// `day` is the day of the month, 1-based, and counts today as a full day.
pub fn forecast(spent: f64, budget: f64, day: u32, days_in_month: u32) -> Option<BudgetForecast> {
    if budget <= 0.0 || day == 0 || days_in_month == 0 || spent < 0.0 {
        return None;
    }
    let projected = (day >= 3).then(|| spent / day as f64 * days_in_month as f64);
    let status = if spent >= budget {
        BudgetStatus::Exceeded
    } else if projected.is_some_and(|p| p > budget) {
        BudgetStatus::Over
    } else {
        BudgetStatus::Ok
    };
    Some(BudgetForecast {
        status,
        spent,
        budget,
        projected,
    })
}
