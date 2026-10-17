fn sourcemap(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::sourcemap(path)
}

datatest_stable::harness! {
    { test = sourcemap, root = "fixtures/sourcemap", pattern = r".*/Main\.purs$" },
}
