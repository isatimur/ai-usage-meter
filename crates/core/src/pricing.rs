use crate::{CostSource, UsageEvent};
use std::collections::HashMap;
use std::path::Path;

const BUNDLED: &str = include_str!("../data/litellm_prices.json");

#[derive(Clone, Debug, Default)]
struct Prices {
    input: Option<f64>,
    output: Option<f64>,
    cache_read: Option<f64>,
    cache_write_5m: Option<f64>,
    cache_write_1h: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct PriceTable {
    models: HashMap<String, Prices>,
}

fn parse(json: &str) -> Result<HashMap<String, Prices>, serde_json::Error> {
    let root: serde_json::Value = serde_json::from_str(json)?;
    let mut models = HashMap::new();
    if let Some(obj) = root.as_object() {
        for (name, v) in obj {
            let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
            models.insert(
                name.clone(),
                Prices {
                    input: f("input_cost_per_token"),
                    output: f("output_cost_per_token"),
                    cache_read: f("cache_read_input_token_cost"),
                    cache_write_5m: f("cache_creation_input_token_cost"),
                    cache_write_1h: f("cache_creation_input_token_cost_above_1hr"),
                },
            );
        }
    }
    Ok(models)
}

impl PriceTable {
    pub fn from_json_str(json: &str) -> Result<PriceTable, serde_json::Error> {
        Ok(PriceTable {
            models: parse(json)?,
        })
    }

    pub fn bundled() -> PriceTable {
        Self::from_json_str(BUNDLED).expect("bundled price table is valid JSON")
    }

    pub fn with_overrides(mut self, path: &Path) -> Result<PriceTable, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        self.models.extend(parse(&text)?);
        Ok(self)
    }

    /// A stable digest of every price. It changes when any price changes.
    pub fn fingerprint(&self) -> String {
        let mut names: Vec<&String> = self.models.keys().collect();
        names.sort();
        let mut h: u64 = 0xcbf29ce484222325; // FNV-1a
        for name in names {
            for b in format!("{name}={:?};", self.models[name]).bytes() {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        format!("{h:016x}")
    }

    fn lookup(&self, model: &str) -> Option<&Prices> {
        self.models
            .get(model)
            .or_else(|| model.rsplit('/').next().and_then(|m| self.models.get(m)))
    }

    /// Fills `cost_usd` only when the event has no cost yet and every
    /// component with tokens has a known price. Never guesses.
    pub fn price(&self, ev: &mut UsageEvent) {
        if ev.cost_source != CostSource::Unknown || ev.cost_usd.is_some() {
            return;
        }
        let Some(p) = self.lookup(&ev.model) else {
            return;
        };
        let parts = [
            (ev.input, p.input),
            (ev.output, p.output),
            (ev.cache_read, p.cache_read),
            (ev.cache_write_5m, p.cache_write_5m),
            (ev.cache_write_1h, p.cache_write_1h),
        ];
        let mut total = 0.0;
        for (tokens, price) in parts {
            if tokens == 0 {
                continue;
            }
            match price {
                Some(rate) => total += tokens as f64 * rate,
                None => return,
            }
        }
        ev.cost_usd = Some(total);
        ev.cost_source = CostSource::Computed;
    }
}
