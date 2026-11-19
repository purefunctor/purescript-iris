fn refactor(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::refactor(path)
}

datatest_stable::harness! {
    { test = refactor, root = "fixtures/refactor", pattern = r".*/Main\.purs$" },
}
