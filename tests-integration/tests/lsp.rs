fn lsp(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::lsp(path)
}

datatest_stable::harness! {
    { test = lsp, root = "fixtures/lsp", pattern = r".*/Main\.(purs|iris)$" },
}
