//! What was read and what was left out. Skipped features are aggregated by
//! feature (first location kept, occurrences counted) so a large file does
//! not produce a huge report.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skipped {
    pub feature: String,
    pub first_location: String,
    pub occurrences: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportReport {
    pub converted: BTreeMap<String, u64>,
    pub skipped: Vec<Skipped>,
}

impl ImportReport {
    pub fn count(&mut self, what: &str, n: u64) {
        *self.converted.entry(what.to_string()).or_insert(0) += n;
    }

    /// Add what `other` converted and skipped (its first locations kept for
    /// features not seen here yet).
    pub fn merge(&mut self, other: ImportReport) {
        for (what, n) in other.converted {
            self.count(&what, n);
        }
        for s in other.skipped {
            match self.skipped.iter_mut().find(|t| t.feature == s.feature) {
                Some(t) => t.occurrences += s.occurrences,
                None => self.skipped.push(s),
            }
        }
    }

    pub fn skip(&mut self, feature: impl Into<String>, location: &str) {
        let feature = feature.into();
        if let Some(s) = self.skipped.iter_mut().find(|s| s.feature == feature) {
            s.occurrences += 1;
        } else {
            self.skipped.push(Skipped {
                feature,
                first_location: location.to_string(),
                occurrences: 1,
            });
        }
    }

    pub fn skipped_feature(&self, feature: &str) -> Option<&Skipped> {
        self.skipped.iter().find(|s| s.feature == feature)
    }
}
