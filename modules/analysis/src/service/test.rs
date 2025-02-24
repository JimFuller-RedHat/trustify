use super::*;
use crate::{
    model::*,
    test::{Node, *},
};
use std::{str::FromStr, time::SystemTime};
use test_context::test_context;
use test_log::test;
use trustify_common::{
    cpe::Cpe,
    db::query::Query,
    model::{BinaryByteSize, Paginated},
    purl::Purl,
    sbom::spdx::fix_license,
};
use trustify_test_context::{TrustifyContext, document, spdx::fix_spdx_rels};

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_analysis_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json", "spdx/simple.json"])
        .await?; //double ingestion intended

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("DD"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.root_traces();
    log::debug!("After: {analysis_graph:#?}");

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert!(
            matches!(
                ancestors[..],
                [[
                    ..,
                    Node {
                        id: "SPDXRef-AA",
                        purls: ["pkg:rpm/redhat/AA@0.0.0?arch=src"],
                        ..
                    }
                ]]
            ),
            "doesn't match: {ancestors:#?}"
        );
    });
    assert_eq!(analysis_graph.total, 1);

    // ensure we set implicit relationship on components with no defined relationships
    let analysis_graph = service
        .retrieve(
            &Query::q("EE"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.roots();
    log::debug!("After: {analysis_graph:#?}");

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_analysis_cyclonedx_service(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["cyclonedx/simple.json", "cyclonedx/simple.json"])
        .await?; //double ingestion intended

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("DD"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.root_traces();
    log::debug!("After: {analysis_graph:#?}");

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert!(
            matches!(
                ancestors[..],
                [[
                    ..,
                    Node {
                        id: "aa",
                        name: "AA",
                        purls: ["pkg:rpm/redhat/AA@0.0.0?arch=src"],
                        ..
                    }
                ]]
            ),
            "doesn't match: {ancestors:#?}"
        );
    });
    assert_eq!(analysis_graph.total, 1);

    // ensure we set implicit relationship on components with no defined relationships
    let analysis_graph = service
        .retrieve(
            &Query::q("EE"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.root_traces();
    log::debug!("After: {analysis_graph:#?}");

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_by_name_analysis_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            ComponentReference::Name("B"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Result: {analysis_graph:#?}");

    let analysis_graph = analysis_graph.root_traces();

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert_eq!(
            ancestors,
            &[&[
                Node {
                    id: "SPDXRef-A",
                    name: "A",
                    version: "1",
                    cpes: &["cpe:/a:redhat:simple:1:*:el9:*"],
                    purls: &["pkg:rpm/redhat/A@0.0.0?arch=src"],
                },
                Node {
                    id: "SPDXRef-DOCUMENT",
                    name: "simple",
                    version: "",
                    cpes: &[],
                    purls: &[],
                },
            ]]
        );
    });

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn simple_by_name_analysis_service_filter_rel(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            ComponentReference::Name("B"),
            QueryOptions {
                relationships: HashSet::from_iter([Relationship::Contains]),
                ..QueryOptions::ancestors()
            },
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Result: {analysis_graph:#?}");

    let analysis_graph = analysis_graph.root_traces();

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert_eq!(
            ancestors,
            &[&[Node {
                id: "SPDXRef-A",
                name: "A",
                version: "1",
                cpes: &["cpe:/a:redhat:simple:1:*:el9:*"],
                purls: &["pkg:rpm/redhat/A@0.0.0?arch=src"],
            },]]
        );
    });

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_by_purl_analysis_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let component_purl: Purl = Purl::from_str("pkg:rpm/redhat/B@0.0.0").map_err(Error::Purl)?;

    let analysis_graph = service
        .retrieve(
            &component_purl,
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.root_traces();
    log::debug!("After: {analysis_graph:#?}");

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert_eq!(
            ancestors,
            [[
                Node {
                    id: "SPDXRef-A",
                    name: "A",
                    version: "1",
                    purls: &["pkg:rpm/redhat/A@0.0.0?arch=src"],
                    cpes: &["cpe:/a:redhat:simple:1:*:el9:*"],
                },
                Node {
                    id: "SPDXRef-DOCUMENT",
                    name: "simple",
                    version: "",
                    cpes: &[],
                    purls: &[],
                }
            ]]
        );
    });

    assert_eq!(analysis_graph.total, 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_quarkus_analysis_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("spymemcached"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.root_traces();
    log::debug!("After: {analysis_graph:#?}");

    assert_ancestors(&analysis_graph.items, |ancestors| {
        assert!(
            matches!(
                ancestors,
                [
                    [..],
                    [
                        Node {
                            id: "SPDXRef-e24fec28-1001-499c-827f-2e2e5f2671b5",
                            name: "quarkus-bom",
                            version: "3.2.12.Final-redhat-00002",
                            cpes: ["cpe:/a:redhat:quarkus:3.2:*:el8:*",],
                            purls: [
                                "pkg:maven/com.redhat.quarkus.platform/quarkus-bom@3.2.12.Final-redhat-00002?repository_url=https://maven.repository.redhat.com/ga/&type=pom"
                            ],
                        },
                        Node {
                            id: "SPDXRef-DOCUMENT",
                            name: "quarkus-bom-3.2.12.Final-redhat-00002",
                            version: "",
                            ..
                        },
                    ]
                ]
            ),
            "doesn't match: {ancestors:#?}"
        );
    });

    assert_eq!(analysis_graph.total, 2);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_status_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());
    let all_graphs = service.load_all_graphs(&ctx.db).await?;
    assert_eq!(all_graphs.len(), 1);

    let analysis_status = service.status(&ctx.db).await?;
    assert_eq!(analysis_status.sbom_count, 1);
    assert_eq!(analysis_status.graph_count, 1);

    service.clear_all_graphs()?;

    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let analysis_status = service.status(&ctx.db).await?;

    assert_eq!(analysis_status.sbom_count, 3);
    assert_eq!(analysis_status.graph_count, 0);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_cache_size_used(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());
    assert_eq!(service.cache_size_used(), 0u64);

    let all_graphs = service.load_all_graphs(&ctx.db).await?;
    assert_eq!(all_graphs.len(), 1);

    let kb = 1024;
    let small_sbom_size = service.cache_size_used();
    assert!(small_sbom_size > 6 * kb);
    assert!(small_sbom_size < 7 * kb);

    ctx.ingest_documents(["spdx/quarkus-bom-3.2.11.Final-redhat-00001.json"])
        .await?;
    let all_graphs = service.load_all_graphs(&ctx.db).await?;
    assert_eq!(all_graphs.len(), 2);

    let big_sbom_size = service.cache_size_used() - small_sbom_size;
    assert!(big_sbom_size > 950 * kb);
    assert!(big_sbom_size < 960 * kb);

    // Now lets try it with small cache that can at least fit the small bom
    let service = AnalysisService::new(AnalysisConfig {
        max_cache_size: BinaryByteSize::from(small_sbom_size * 2),
    });

    let all_graphs = service.load_all_graphs(&ctx.db).await?;
    // we should be able to load all the graphs even if they can't fit in the cache.
    assert_eq!(all_graphs.len(), 2);

    // but the cache should only contain the first sbom
    assert_eq!(small_sbom_size, service.cache_size_used());
    assert_eq!(1u64, service.cache_len());

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_deps_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("AA"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    assert_eq!(analysis_graph.total, 1);

    // ensure we set implicit relationship on components with no defined relationships
    let analysis_graph = service
        .retrieve(
            &Query::q("EE"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {analysis_graph:#?}");
    let analysis_graph = analysis_graph.roots();
    log::debug!("After: {analysis_graph:#?}");

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_deps_cyclonedx_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["cyclonedx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("AA"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    assert_eq!(analysis_graph.total, 1);

    // ensure we set implicit relationship on component with no defined relationships
    let analysis_graph = service
        .retrieve(
            &Query::q("EE"),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?
        .roots();
    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_by_name_deps_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            ComponentReference::Name("A"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    assert_eq!(analysis_graph.items.len(), 1);
    assert_eq!(analysis_graph.total, 1);

    assert_eq!(
        analysis_graph.items[0].purl,
        vec![Purl::from_str("pkg:rpm/redhat/A@0.0.0?arch=src")?]
    );
    assert_eq!(
        analysis_graph.items[0].cpe,
        vec![Cpe::from_str("cpe:/a:redhat:simple:1::el9")?]
    );

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_simple_by_purl_deps_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let component_purl: Purl =
        Purl::from_str("pkg:rpm/redhat/AA@0.0.0?arch=src").map_err(Error::Purl)?;

    let analysis_graph = service
        .retrieve(
            &component_purl,
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    assert_eq!(
        analysis_graph.items[0].purl,
        vec![Purl::from_str("pkg:rpm/redhat/AA@0.0.0?arch=src")?]
    );

    assert_eq!(analysis_graph.total, 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_quarkus_deps_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            &Query::q("spymemcached"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    assert_eq!(analysis_graph.total, 2);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_circular_deps_cyclonedx_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["cyclonedx/cyclonedx-circular.json"])
        .await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            ComponentReference::Name("junit-bom"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    // we should get zero, as we don't deal with circular dependencies and don't load such graphs
    assert_eq!(analysis_graph.total, 0);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_circular_deps_spdx_service(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/loop.json"]).await?;

    let service = AnalysisService::new(AnalysisConfig::default());

    let analysis_graph = service
        .retrieve(
            ComponentReference::Name("A"),
            QueryOptions::descendants(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    // we should get zero, as we don't deal with circular dependencies and don't load such graphs
    assert_eq!(analysis_graph.total, 0);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn test_retrieve_all_sbom_roots_by_name(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    ctx.ingest_documents(["spdx/quarkus-bom-3.2.11.Final-redhat-00001.json"])
        .await?;

    let service = AnalysisService::new(AnalysisConfig::default());
    let component_name = "quarkus-vertx-http".to_string();

    let analysis_graph = service
        .retrieve(
            &Query::q(&component_name),
            QueryOptions::ancestors(),
            Paginated::default(),
            &ctx.db,
        )
        .await?;

    let analysis_graph = analysis_graph.roots();

    log::debug!("Result: {analysis_graph:#?}");

    let sbom_id = analysis_graph
        .items
        .last()
        .unwrap()
        .sbom_id
        .parse::<Uuid>()?;

    let roots = service
        .retrieve_single(
            sbom_id,
            ComponentReference::Name(&component_name),
            QueryOptions::ancestors(),
            Default::default(),
            &ctx.db,
        )
        .await?;

    log::debug!("Before: {roots:#?}");
    let roots = roots.root_traces();
    log::debug!("After: {roots:#?}");

    assert_ancestors(&roots.items, |ancestors| {
        assert!(
            matches!(
                ancestors,
                [[
                    ..,
                    Node {
                        name: "quarkus-bom-3.2.11.Final-redhat-00001",
                        ..
                    }
                ]]
            ),
            "doesn't match: {ancestors:#?}"
        );
    });

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn load_performance(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let (spdx, _) =
        document::<serde_json::Value>("openshift-container-storage-4.8.z.json.xz").await?;
    let (spdx, _) = fix_license(&(), spdx);
    let spdx = fix_spdx_rels(serde_json::from_value(spdx)?);

    log::info!("Start ingestion");

    ctx.ingest_json(spdx).await?;

    log::info!("Start populating graph");

    let start = SystemTime::now();
    let service = AnalysisService::new(AnalysisConfig::default());
    service.load_all_graphs(&ctx.db).await?;

    log::info!(
        "Loading took: {}",
        humantime::format_duration(start.elapsed()?)
    );

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(tokio::test)]
async fn resolve_sbom_external_node_sbom(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    // ingest cdx
    ctx.ingest_document("cyclonedx/simple-ext-a.json").await?;
    let service = AnalysisService::new(AnalysisConfig::default());
    let get_external_sbom_id = service
        .resolve_external_sbom_id(
            "urn:cdx:a4f16b62-fea9-42c1-8365-d72d3cef37d1/2#b".to_string(),
            &ctx.db,
        )
        .await;
    assert_eq!(get_external_sbom_id, None);

    // now ingest cdx sbom referred in "cyclonedx/simple-ext-b.json"
    ctx.ingest_document("cyclonedx/simple-ext-b.json").await?;
    let get_external_sbom_id = service
        .resolve_external_sbom_id(
            "urn:cdx:a4f16b62-fea9-42c1-8365-d72d3cef37d1/2#b".to_string(),
            &ctx.db,
        )
        .await;
    assert!(get_external_sbom_id.is_some());

    // now try spdx
    ctx.ingest_document("spdx/simple-ext-a.json").await?;
    let get_external_sbom_id = service
        .resolve_external_sbom_id("DocumentRef-ext-b:SPDXRef-A".to_string(), &ctx.db)
        .await;
    assert_eq!(get_external_sbom_id, None);

    // now ingest spdx sbom referred in "spdx/simple-ext-b.json"
    ctx.ingest_document("spdx/simple-ext-b.json").await?;
    let get_external_sbom_id = service
        .resolve_external_sbom_id("DocumentRef-ext-b:SPDXRef-A".to_string(), &ctx.db)
        .await;
    assert!(get_external_sbom_id.is_some());

    // ingest rh product "spdx/rh/product_component/rhel-9.2-eus.spdx.json"
    ctx.ingest_document("spdx/rh/product_component/rhel-9.2-eus.spdx.json")
        .await?;
    let get_external_sbom_id = service
        .resolve_external_sbom_id(
            "SPDXRef-RHEL-9.2-EUS:SPDXRef-openssl-3.0.7-18.el9-2".to_string(),
            &ctx.db,
        )
        .await;
    assert_eq!(get_external_sbom_id, None);

    // now ingest rh component spdx "spdx/rh/product_component/openssl-3.0.7-18.el9_2.spdx.json"
    ctx.ingest_document("spdx/rh/product_component/openssl-3.0.7-18.el9_2.spdx.json")
        .await?;
    let get_external_sbom_id = service
        .resolve_external_sbom_id(
            "SPDXRef-RHEL-9.2-EUS:SPDXRef-openssl-3.0.7-18.el9-2".to_string(),
            &ctx.db,
        )
        .await;

    if let Some(external_sbom_id) = get_external_sbom_id {
        let sbom = sbom::Entity::find()
            .filter(sbom::Column::SbomId.eq(external_sbom_id))
            .one(&ctx.db)
            .await;
        assert_eq!(
            sbom.unwrap().unwrap().document_id,
            Some("https://www.redhat.com/openssl-3.0.7-18.el9_2.spdx.json".to_string()),
        )
    } else {
        panic!("Could not find sbom_id.");
    }

    Ok(())
}
