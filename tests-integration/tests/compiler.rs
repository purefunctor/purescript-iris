fn compiler(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::compiler(path)
}

datatest_stable::harness! {
    { test = compiler, root = "fixtures/compiler", pattern = r".*/Main\.(purs|iris)$" },
}
