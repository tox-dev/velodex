use std::num::NonZeroUsize;
use std::time::Duration;

use rstest::rstest;

use super::toml_config;
use crate::config::{self, AvailabilityConfig, AvailabilityMode, Config, ReplicationConfig, SecretSource};

#[test]
fn test_omitted_table_and_explicit_none_resolve_alike() {
    let omitted = Config::default().availability;
    let explicit = toml_config("[availability]\nmode = \"none\"\n").availability;

    assert_eq!(omitted, AvailabilityConfig::None);
    assert_eq!(explicit, AvailabilityConfig::None);
}

#[test]
fn test_empty_table_selects_none() {
    assert_eq!(toml_config("[availability]\n").availability, AvailabilityConfig::None);
}

#[rstest]
#[case::none(AvailabilityConfig::None, AvailabilityMode::None, false)]
#[case::dc(AvailabilityConfig::Dc(primary()), AvailabilityMode::Dc, true)]
#[case::ha(AvailabilityConfig::Ha(primary()), AvailabilityMode::Ha, true)]
fn test_availability_accessors_report_mode_and_topology(
    #[case] availability: AvailabilityConfig,
    #[case] mode: AvailabilityMode,
    #[case] carries_role: bool,
) {
    assert_eq!(availability.mode(), mode);
    assert_eq!(availability.replication().is_some(), carries_role);
}

#[rstest]
#[case::none_with_role(
    "[availability]\nmode = \"none\"\n[availability.replication]\nrole = \"primary\"\nsource = \"a\"\ntoken = \"b\"\n",
    "`none` mode configures no replication"
)]
#[case::role_without_mode(
    "[availability]\n[availability.replication]\nrole = \"primary\"\nsource = \"a\"\ntoken = \"b\"\n",
    "`none` mode configures no replication"
)]
#[case::dc_without_role("[availability]\nmode = \"dc\"\n", "`dc` and `ha` modes need")]
#[case::ha_without_role("[availability]\nmode = \"ha\"\n", "`dc` and `ha` modes need")]
#[case::unknown_mode("[availability]\nmode = \"quorum\"\n", "unknown variant")]
fn test_availability_rejects_impossible_combinations(#[case] text: &str, #[case] expected: &str) {
    let error = config::from_toml("x.toml".into(), text)
        .and_then(|partial| Config::default().apply(partial))
        .unwrap_err();

    assert!(error.to_string().contains(expected), "{error}");
}

fn primary() -> ReplicationConfig {
    ReplicationConfig::Primary {
        source: "primary-a".to_owned(),
        token: SecretSource::Literal("secret".to_owned()),
    }
}

#[test]
fn test_dc_and_ha_carry_distinct_topology() {
    let replica = || ReplicationConfig::Replica {
        upstream: "https://primary.example/".to_owned(),
        token: SecretSource::Literal("secret".to_owned()),
        poll_interval: Duration::from_secs(1),
        page_size: NonZeroUsize::MIN,
    };

    assert_ne!(AvailabilityConfig::Dc(replica()), AvailabilityConfig::Ha(replica()));
}
