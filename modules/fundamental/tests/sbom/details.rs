use test_context::test_context;
use test_log::test;
use tracing::instrument;
use trustify_cvss::cvss3::severity::Severity;
use trustify_module_fundamental::sbom::model::details::SbomDetails;
use trustify_module_fundamental::sbom::service::SbomService;
use trustify_test_context::TrustifyContext;

#[test_context(TrustifyContext)]
#[instrument]
#[test(tokio::test)]
async fn sbom_details_cyclonedx_osv(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let sbom = SbomService::new(ctx.db.clone());

    // ingest the SBOM
    let result1 = ctx.ingest_document("cyclonedx/ghsa_test.json").await?;

    assert_eq!(
        result1.document_id,
        Some("urn:uuid:a5ddee00-4b86-498c-b7fd-b001b77479d1".to_string())
    );

    // ingest the advisory
    let pypi = ctx.ingest_document("osv/GHSA-45c4-8wx5-qw6w.json").await?;

    assert_eq!(pypi.document_id, Some("GHSA-45c4-8wx5-qw6w".to_string()));

    let cratesio = ctx.ingest_document("osv/GHSA-c25x-cm9x-qqgx.json").await?;

    assert_eq!(
        cratesio.document_id,
        Some("GHSA-c25x-cm9x-qqgx".to_string())
    );

    let sbom1 = sbom
        .fetch_sbom_details(result1.id, vec![], &ctx.db)
        .await?
        .expect("SBOM details must be found");
    log::info!("SBOM1: {sbom1:?}");

    assert_eq!(2, sbom1.advisories.len());
    check_advisory(
        &sbom1,
        "GHSA-45c4-8wx5-qw6w",
        "CVE-2023-37276",
        Severity::Medium,
    );
    check_advisory(
        &sbom1,
        "GHSA-c25x-cm9x-qqgx",
        "CVE-2023-28445",
        Severity::Critical,
    );
    Ok(())
}

fn check_advisory(
    sbom: &SbomDetails,
    advisory_id: &str,
    vulnerability_id: &str,
    severity: Severity,
) {
    let advisories = sbom
        .advisories
        .clone()
        .into_iter()
        .filter(|advisory| advisory.head.document_id == advisory_id)
        .collect::<Vec<_>>();
    assert_eq!(
        1,
        advisories.len(),
        "Found multiple advisories with ID {}",
        advisory_id
    );
    let advisory = advisories[0].clone();
    assert_eq!(1, advisory.status.len());
    assert_eq!(
        vulnerability_id,
        advisory.status[0].vulnerability.identifier
    );
    assert_eq!(severity, advisory.status[0].average_severity);
    assert_eq!("affected", advisory.status[0].status);
}
