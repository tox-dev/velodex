use std::collections::BTreeMap;

use peryx_ecosystem_pypi::{CoreMetadata, File, Meta, ProjectDetail, Provenance, Yanked};

pub fn project_detail(project: &str, files: usize) -> ProjectDetail {
    let files: Vec<File> = (0..files).map(|index| sample_file(project, index)).collect();
    let mut versions: Vec<String> = files
        .iter()
        .filter_map(|file| file.filename.split('-').nth(1).map(str::to_owned))
        .collect();
    versions.sort();
    versions.dedup();
    ProjectDetail {
        meta: Meta::default(),
        name: project.to_owned(),
        versions,
        files,
    }
}

fn sample_file(project: &str, index: usize) -> File {
    let version = format!("{}.{}.{}", index / 100, (index / 10) % 10, index % 10);
    let py = 8 + index % 5;
    let filename = format!("{project}-{version}-cp3{py}-cp3{py}-manylinux_2_17_x86_64.whl");
    let mut hashes = BTreeMap::new();
    hashes.insert(
        "sha256".to_owned(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
    );
    File {
        url: format!("https://files.pythonhosted.org/packages/ab/cd/{filename}"),
        filename,
        hashes,
        requires_python: Some(">=3.8".to_owned()),
        size: Some(1_000_000 + index as u64),
        upload_time: Some("2024-01-01T00:00:00.000000Z".to_owned()),
        yanked: Yanked::No,
        core_metadata: CoreMetadata::Available,
        dist_info_metadata: CoreMetadata::Absent,
        gpg_sig: None,
        provenance: Provenance::default(),
    }
}
