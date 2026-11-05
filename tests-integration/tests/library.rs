fn library(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::library(path)
}

datatest_stable::harness! {
    { test = library, root = "../library", pattern = r"^[^/]+/src/.+\.purs$" },
}
