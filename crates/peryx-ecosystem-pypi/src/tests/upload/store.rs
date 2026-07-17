//! Persisting a prepared upload, including the PEP 740 provenance sibling on the blocking path the
//! offline import command uses.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake2::Blake2bVar;
use blake2::digest::{Update as _, VariableOutput as _};
use peryx_storage::blob::{BlobStorage, Digest};
use peryx_storage::meta::MetaStore;
use serde_json::{Value, json};

use super::support::{hex, staged_form, wheel_metadata};
use crate::store::PypiStore as _;
use crate::upload::{StagedUpload, prepare, store_prepared_blocking};

const FILENAME: &str = "Flask-1.0-py3-none-any.whl";

fn attestations_field(filename: &str, sha256: &str) -> String {
    let statement = STANDARD.encode(
        json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [{"name": filename, "digest": {"sha256": sha256}}],
            "predicateType": "https://docs.pypi.org/attestations/publish/v1",
            "predicate": {},
        })
        .to_string(),
    );
    json!([{
        "version": 1,
        "verification_material": {"certificate": "Zm9v", "transparency_entries": []},
        "envelope": {"statement": statement, "signature": "YmFy"},
    }])
    .to_string()
}

fn blake2_256(bytes: &[u8]) -> String {
    let mut blake2 = Blake2bVar::new(32).unwrap();
    blake2.update(bytes);
    let mut digest = [0; 32];
    blake2.finalize_variable(&mut digest).unwrap();
    hex(&digest)
}

#[test]
fn test_store_prepared_blocking_stages_and_records_the_provenance_bundle() {
    let wheel = wheel_metadata("Flask", "1.0");
    let dir = tempfile::tempdir().unwrap();
    let blobs = BlobStorage::filesystem(dir.path().join("blobs"));
    let meta = MetaStore::open(dir.path().join("peryx.redb")).unwrap();

    let blob = blobs.blocking().stage_bytes(&wheel).unwrap();
    let sha = blob.digest().as_str().to_owned();
    let staged = StagedUpload {
        blob,
        blake2_256: blake2_256(&wheel),
    };
    let mut form = staged_form(&wheel);
    form.attestations = Some(attestations_field(FILENAME, &sha));

    let prepared = prepare(form, staged, "root/hosted", 1000).unwrap();
    assert!(
        prepared.provenance.is_some(),
        "attestations produce a provenance object"
    );

    let stored = store_prepared_blocking(&meta, &blobs, "hosted", prepared).unwrap();

    assert!(stored);
    let (provenance_sha, size) = meta
        .get_provenance(&sha)
        .unwrap()
        .expect("the provenance row is written");
    let bytes = blobs
        .blocking()
        .read_bytes(&Digest::from_hex(&provenance_sha).unwrap(), 1 << 20)
        .unwrap();
    assert_eq!(bytes.len() as u64, size, "the recorded size matches the staged blob");
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["version"], 1);
    assert_eq!(
        document["attestation_bundles"][0]["attestations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
