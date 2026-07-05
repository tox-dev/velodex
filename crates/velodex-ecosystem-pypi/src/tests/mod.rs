mod filename_tests;
mod html_tests;
mod metadata_tests;
mod name_tests;
mod simple_tests;
mod version_tests;

#[test]
fn test_pypi_driver_reports_its_ecosystem() {
    use velodex_format::{Ecosystem, EcosystemDriver};
    assert_eq!(crate::PypiDriver.ecosystem(), Ecosystem::Pypi);
}
