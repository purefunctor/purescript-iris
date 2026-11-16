fn checking_v2(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::checking_v2(path)
}

datatest_stable::harness! {
    { test = checking_v2, root = "fixtures/checking-v2", pattern = r".*\.purs$" },
}
