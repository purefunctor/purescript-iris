fn format(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::format(path)
}

datatest_stable::harness! {
    { test = format, root = "fixtures/format", pattern = r".*\.purs$" },
}
