use actix_web::cookie::time::OffsetDateTime;
use std::sync::Arc;
use test_context::test_context;
use test_log::test;
use trustify_common::db::query::Query;
use trustify_common::db::test::TrustifyContext;
use trustify_common::model::Paginated;
use trustify_module_ingestor::graph::advisory::AdvisoryInformation;
use trustify_module_ingestor::graph::Graph;

#[test_context(TrustifyContext, skip_teardown)]
#[test(actix_web::test)]
async fn all_organizations(ctx: TrustifyContext) -> Result<(), anyhow::Error> {
    let db = ctx.db;
    let graph = Arc::new(Graph::new(db.clone()));

    graph
        .ingest_advisory(
            "CPIC-1",
            "http://captpickles.com/",
            "8675309",
            AdvisoryInformation {
                title: Some("CAPT-1".to_string()),
                issuer: Some("Capt Pickles Industrial Conglomerate".to_string()),
                published: Some(OffsetDateTime::now_utc()),
                modified: None,
                withdrawn: None,
            },
            (),
        )
        .await?;

    let service = crate::organization::service::OrganizationService::new(db);

    let orgs = service
        .fetch_organizations(Query::default(), Paginated::default(), ())
        .await?;

    assert_eq!(1, orgs.total);
    assert_eq!(1, orgs.items.len());

    Ok(())
}
