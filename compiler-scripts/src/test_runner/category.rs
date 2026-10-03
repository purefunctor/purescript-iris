use std::str::FromStr;

use anyhow::bail;

#[derive(Copy, Clone, Debug)]
pub enum TestCategory {
    Compiler,
    Format,
    Lowering,
    Resolving,
    Lsp,
    Docs,
}

impl TestCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestCategory::Compiler => "compiler",
            TestCategory::Format => "format",
            TestCategory::Lowering => "lowering",
            TestCategory::Resolving => "resolving",
            TestCategory::Lsp => "lsp",
            TestCategory::Docs => "docs",
        }
    }

    pub fn fixtures_subdir_fragment(&self) -> String {
        format!("tests-integration/fixtures/{}", self.as_str())
    }

    pub fn test_targets(&self) -> &'static [&'static str] {
        match self {
            TestCategory::Compiler => &["compiler"],
            TestCategory::Format => &["format"],
            TestCategory::Lowering => &["lowering"],
            TestCategory::Resolving => &["resolving"],
            TestCategory::Lsp => &["lsp"],
            TestCategory::Docs => &["docs"],
        }
    }

    pub fn snapshot_path_fragments(&self) -> Vec<String> {
        vec![
            format!("tests-integration/fixtures/{}", self.as_str()),
            format!("tests-integration/tests/snapshots/{}__", self.as_str()),
        ]
    }
}

impl FromStr for TestCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "compiler" | "c" => Ok(TestCategory::Compiler),
            "format" => Ok(TestCategory::Format),
            "lowering" | "l" => Ok(TestCategory::Lowering),
            "resolving" | "r" => Ok(TestCategory::Resolving),
            "lsp" => Ok(TestCategory::Lsp),
            "docs" => Ok(TestCategory::Docs),
            _ => bail!(
                "unknown test category '{}', expected: compiler (c), format, lowering (l), resolving (r), lsp, docs",
                s
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_has_c_alias() {
        assert!(matches!(TestCategory::from_str("compiler"), Ok(TestCategory::Compiler)));
        assert!(matches!(TestCategory::from_str("c"), Ok(TestCategory::Compiler)));
    }

    #[test]
    fn obsolete_compiler_category_names_are_rejected() {
        for name in ["backend", "b", "checking", "semantic", "s", "functional"] {
            assert!(TestCategory::from_str(name).is_err(), "{name} should be rejected");
        }
    }
}
