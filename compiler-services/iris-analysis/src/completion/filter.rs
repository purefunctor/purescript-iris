use strsim::jaro_winkler;

use super::prelude::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct NoFilter;

impl Filter for NoFilter {
    fn matches(&self, _name: &str) -> bool {
        true
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StartsWith<'a>(pub &'a str);

impl Filter for StartsWith<'_> {
    fn matches(&self, name: &str) -> bool {
        name.starts_with(self.0)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FuzzyMatch<'a>(pub &'a str);

impl Filter for FuzzyMatch<'_> {
    fn matches(&self, name: &str) -> bool {
        jaro_winkler(self.0, name) >= 0.5
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PerfectSegmentFuzzy<'a>(pub &'a str);

impl Filter for PerfectSegmentFuzzy<'_> {
    fn matches(&self, name: &str) -> bool {
        let mut segments = self.0.split('.');

        let perfect = segments.any(|segment| segment.starts_with(name));
        let fuzzy = jaro_winkler(self.0, name) >= 0.5;

        perfect || fuzzy
    }
}
