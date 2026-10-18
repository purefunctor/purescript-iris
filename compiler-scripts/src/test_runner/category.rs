use std::str::FromStr;

use anyhow::bail;

#[derive(Copy, Clone, Debug)]
pub enum TestCategory {
    Compiler,
    Lowering,
    Resolving,
    Lsp,
    Docs,
    Formatting,
    SourceMap,
}

impl TestCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestCategory::Compiler => "compiler",
            TestCategory::Lowering => "lowering",
            TestCategory::Resolving => "resolving",
            TestCategory::Lsp => "lsp",
            TestCategory::Docs => "docs",
            TestCategory::Formatting => "formatting",
            TestCategory::SourceMap => "sourcemap",
        }
    }

    pub fn fixtures_subdir_fragment(&self) -> String {
        format!("tests-integration/fixtures/{}", self.as_str())
    }

    pub fn test_targets(&self) -> &'static [&'static str] {
        match self {
            TestCategory::Compiler => &["compiler"],
            TestCategory::Lowering => &["lowering", "lowering_unit"],
            TestCategory::Resolving => &["resolving"],
            TestCategory::Lsp => &["lsp"],
            TestCategory::Docs => &["docs"],
            TestCategory::Formatting => &["formatting"],
            TestCategory::SourceMap => &["sourcemap"],
        }
    }

    pub fn snapshot_path_fragments(&self) -> Vec<String> {
        let mut fragments = vec![format!("tests-integration/fixtures/{}", self.as_str())];
        for target in self.test_targets() {
            fragments.push(format!("tests-integration/tests/snapshots/{target}__"));
        }
        fragments
    }
}

impl FromStr for TestCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "compiler" | "c" => Ok(TestCategory::Compiler),
            "lowering" | "l" => Ok(TestCategory::Lowering),
            "resolving" | "r" => Ok(TestCategory::Resolving),
            "lsp" => Ok(TestCategory::Lsp),
            "docs" => Ok(TestCategory::Docs),
            "formatting" => Ok(TestCategory::Formatting),
            "sourcemap" => Ok(TestCategory::SourceMap),
            _ => bail!(
                "unknown test category '{}', expected: compiler (c), lowering (l), resolving (r), lsp, docs, formatting, sourcemap",
                s
            ),
        }
    }
}
