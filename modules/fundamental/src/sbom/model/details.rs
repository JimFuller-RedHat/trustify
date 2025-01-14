use super::SbomSummary;
use crate::{
    advisory::model::AdvisoryHead,
    purl::model::{details::purl::StatusContext, summary::purl::PurlSummary},
    sbom::{
        model::SbomPackage,
        service::{sbom::QueryCatcher, SbomService},
    },
    vulnerability::model::VulnerabilityHead,
    Error,
};
use cpe::{cpe::Cpe, uri::OwnedUri};
use sea_orm::{
    ConnectionTrait, DbBackend, DbErr, EntityTrait, FromQueryResult, JoinType, ModelTrait,
    QueryFilter, QueryResult, QuerySelect, RelationTrait, Select, Statement,
};
use sea_query::{Asterisk, Expr, Func, SimpleExpr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use trustify_common::{
    cpe::CpeCompare,
    db::{
        multi_model::{FromQueryResultMultiModel, SelectIntoMultiModel},
        VersionMatches,
    },
    memo::Memo,
};
use trustify_cvss::cvss3::{score::Score, severity::Severity, Cvss3Base};
use trustify_entity::{
    advisory, base_purl, cvss3, product_status, product_version, purl_status, qualified_purl, sbom,
    sbom_node, sbom_package, sbom_package_purl_ref, status, version_range, versioned_purl,
    vulnerability,
};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SbomDetails {
    #[serde(flatten)]
    pub summary: SbomSummary,

    pub advisories: Vec<SbomAdvisory>,
}

impl SbomDetails {
    /// turn an (sbom, sbom_node) row into an [`SbomDetails`], if possible
    pub async fn from_entity<C: ConnectionTrait>(
        (sbom, node): (sbom::Model, Option<sbom_node::Model>),
        service: &SbomService,
        tx: &C,
        statuses: Vec<String>,
    ) -> Result<Option<SbomDetails>, Error> {
        let mut relevant_advisory_info = sbom
            .find_related(sbom_package::Entity)
            .join(JoinType::Join, sbom_package::Relation::Node.def())
            .join(JoinType::LeftJoin, sbom_package::Relation::Purl.def())
            .join(
                JoinType::LeftJoin,
                sbom_package_purl_ref::Relation::Purl.def(),
            )
            .join(
                JoinType::LeftJoin,
                qualified_purl::Relation::VersionedPurl.def(),
            )
            .join(JoinType::LeftJoin, versioned_purl::Relation::BasePurl.def())
            .join(JoinType::Join, base_purl::Relation::PurlStatus.def())
            .join(JoinType::Join, purl_status::Relation::Status.def())
            .filter(Expr::col((status::Entity, status::Column::Slug)).is_in(statuses.clone()))
            .join(
                JoinType::LeftJoin,
                purl_status::Relation::VersionRange.def(),
            )
            .filter(SimpleExpr::FunctionCall(
                Func::cust(VersionMatches)
                    .arg(Expr::col((
                        versioned_purl::Entity,
                        versioned_purl::Column::Version,
                    )))
                    .arg(Expr::col((version_range::Entity, Asterisk))),
            ))
            .join(JoinType::LeftJoin, purl_status::Relation::ContextCpe.def())
            .join(JoinType::Join, purl_status::Relation::Advisory.def())
            .join(JoinType::Join, purl_status::Relation::Vulnerability.def())
            .select_only()
            .try_into_multi_model::<QueryCatcher>()?
            .all(tx)
            .await?;

        // The query for now is in the raw form for couple of reasons
        // First some of the join are not easily (or at all) doable using sea-orm concepts
        // Second, it's much easier to iterate over query and work on it in this form
        // than using the code
        // It might be a good practice to start like this for complex query logic and
        // turn it into a code once things stabilize
        let product_advisory_info = r#"
            SELECT
                "advisory"."id" AS "advisory$id",
                "advisory"."identifier" AS "advisory$identifier",
                "advisory"."version" AS "advisory$version",
                "advisory"."document_id" AS "advisory$document_id",
                "advisory"."deprecated" AS "advisory$deprecated",
                "advisory"."issuer_id" AS "advisory$issuer_id",
                "advisory"."published" AS "advisory$published",
                "advisory"."modified" AS "advisory$modified",
                "advisory"."withdrawn" AS "advisory$withdrawn",
                "advisory"."title" AS "advisory$title",
                "advisory"."labels" AS "advisory$labels",
                "advisory"."source_document_id" AS "advisory$source_document_id",
                "vulnerability"."id" AS "vulnerability$id",
                "vulnerability"."title" AS "vulnerability$title",
                "vulnerability"."reserved" AS "vulnerability$reserved",
                "vulnerability"."published" AS "vulnerability$published",
                "vulnerability"."modified" AS "vulnerability$modified",
                "vulnerability"."withdrawn" AS "vulnerability$withdrawn",
                "vulnerability"."cwes" AS "vulnerability$cwes",
                "base_purl"."id" AS "base_purl$id",
                "base_purl"."type" AS "base_purl$type",
                "base_purl"."namespace" AS "base_purl$namespace",
                "base_purl"."name" AS "base_purl$name",
                "versioned_purl"."id" AS "versioned_purl$id",
                "versioned_purl"."base_purl_id" AS "versioned_purl$base_purl_id",
                "versioned_purl"."version" AS "versioned_purl$version",
                "qualified_purl"."id" AS "qualified_purl$id",
                "qualified_purl"."versioned_purl_id" AS "qualified_purl$versioned_purl_id",
                "qualified_purl"."qualifiers" AS "qualified_purl$qualifiers",
                "qualified_purl"."purl" AS "qualified_purl$purl",
                "sbom_package"."sbom_id" AS "sbom_package$sbom_id",
                "sbom_package"."node_id" AS "sbom_package$node_id",
                "sbom_package"."version" AS "sbom_package$version",
                "sbom_node"."sbom_id" AS "sbom_node$sbom_id",
                "sbom_node"."node_id" AS "sbom_node$node_id",
                "sbom_node"."name" AS "sbom_node$name",
                "status"."id" AS "status$id",
                "status"."slug" AS "status$slug",
                "status"."name" AS "status$name",
                "status"."description" AS "status$description",
                "cpe"."id" AS "cpe$id",
                "cpe"."part" AS "cpe$part",
                "cpe"."vendor" AS "cpe$vendor",
                "cpe"."product" AS "cpe$product",
                "cpe"."version" AS "cpe$version",
                "cpe"."update" AS "cpe$update",
                "cpe"."edition" AS "cpe$edition",
                "cpe"."language" AS "cpe$language"
            FROM "sbom"
            -- find statuses that matches SBOMs
            JOIN "product_version" ON "product_version"."sbom_id" = "sbom"."sbom_id"
            JOIN "product" ON "product_version"."product_id" = "product"."id"
            JOIN "cpe" ON "product"."cpe_key" = "cpe"."product"
            JOIN "product_status" ON "cpe"."id" = "product_status"."context_cpe_id" AND product_status.package IS NOT NULL
            JOIN "product_version_range" ON "product_status"."product_version_range_id" = "product_version_range"."id"
            JOIN "version_range" ON "product_version_range"."version_range_id" = "version_range"."id" AND version_matches("product_version"."version", "version_range".*)

            -- now find matching purls in these statuses
            JOIN base_purl ON product_status.package = base_purl.name OR product_status.package LIKE CONCAT(base_purl.namespace, '/%')
            JOIN "versioned_purl" ON "versioned_purl"."base_purl_id" = "base_purl"."id"
            JOIN "qualified_purl" ON "qualified_purl"."versioned_purl_id" = "versioned_purl"."id"
            join sbom_package_purl_ref ON sbom_package_purl_ref.qualified_purl_id = qualified_purl.id AND sbom_package_purl_ref.sbom_id = sbom.sbom_id
            JOIN sbom_package on sbom_package.sbom_id = sbom_package_purl_ref.sbom_id AND sbom_package.node_id = sbom_package_purl_ref.node_id
            JOIN sbom_node on sbom_node.sbom_id = sbom_package_purl_ref.sbom_id AND sbom_node.node_id = sbom_package_purl_ref.node_id

            -- get basic status info
            JOIN "status" ON "product_status"."status_id" = "status"."id"
            JOIN "advisory" ON "product_status"."advisory_id" = "advisory"."id"
            JOIN "vulnerability" ON "product_status"."vulnerability_id" = "vulnerability"."id"
            WHERE
            "sbom"."sbom_id" = $1
            AND "status"."slug" = ANY($2::text[])
            "#;

        let result: Vec<QueryResult> = tx
            .query_all(Statement::from_sql_and_values(
                DbBackend::Postgres,
                product_advisory_info,
                [sbom.sbom_id.into(), statuses.into()],
            ))
            .await?;

        relevant_advisory_info.extend(
            result
                .iter()
                .map(|row| QueryCatcher::from_query_result(row, ""))
                .collect::<Result<Vec<_>, _>>()?,
        );

        let summary = SbomSummary::from_entity((sbom, node), service, tx).await?;

        Ok(match summary {
            Some(summary) => Some(SbomDetails {
                summary: summary.clone(),
                advisories: SbomAdvisory::from_models(
                    &summary.described_by,
                    &relevant_advisory_info,
                    tx,
                )
                .await?,
            }),
            None => None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SbomAdvisory {
    #[serde(flatten)]
    pub head: AdvisoryHead,
    pub status: Vec<SbomStatus>,
}

impl SbomAdvisory {
    pub async fn from_models<C: ConnectionTrait>(
        described_by: &[SbomPackage],
        statuses: &[QueryCatcher],
        tx: &C,
    ) -> Result<Vec<Self>, Error> {
        let mut advisories = HashMap::new();

        let sbom_cpes = described_by
            .iter()
            .flat_map(|each| each.cpe.iter())
            .flat_map(|e| {
                let e = e.replace(":*:", "::");
                let e = e.replace(":*", "");
                let result = cpe::uri::Uri::parse(&e);
                result.ok().map(|wfn| wfn.as_uri().to_owned())
            })
            .collect::<Vec<_>>();

        'status: for each in statuses {
            let status_cpe = if let Some(status_cpe) = &each.context_cpe {
                let status_cpe: Result<OwnedUri, _> = status_cpe.try_into();
                if let Ok(status_cpe) = status_cpe {
                    if sbom_cpes.iter().any(|sbom_cpe| {
                        let status_version = status_cpe.version().to_string();
                        let sbom_version = sbom_cpe.version().to_string();
                        // This is a bit simplified logic, but it is tune with v1 parity.
                        // We need to investigate this more and apply proper version matching in the future
                        status_cpe.is_superset(sbom_cpe)
                            || status_version == "*"
                            || sbom_version.starts_with(&status_version)
                    }) {
                        // status context is applicable, keep truckin'
                    } else {
                        // status context excludes this one, skip over
                        continue 'status;
                    }
                    Some(status_cpe)
                } else {
                    None
                }
            } else {
                None
            };

            // if we got here, then there's either no context or the context matches this SBOM
            let advisory = if let Some(advisory) = advisories.get_mut(&each.advisory.id) {
                advisory
            } else {
                advisories.insert(
                    each.advisory.id,
                    SbomAdvisory {
                        head: AdvisoryHead::from_advisory(&each.advisory, Memo::NotProvided, tx)
                            .await?,
                        status: vec![],
                    },
                );

                advisories
                    .get_mut(&each.advisory.id)
                    .ok_or(Error::Data("Failed to build advisories".to_string()))?
            };

            let sbom_status = if let Some(status) = advisory.status.iter_mut().find(|status| {
                if status.status == each.status.slug
                    && status.vulnerability.identifier == each.vulnerability.id
                {
                    match (&status.context, &status_cpe) {
                        (Some(StatusContext::Cpe(context_cpe)), Some(status_cpe)) => {
                            *context_cpe == status_cpe.to_string()
                        }
                        (None, None) => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }) {
                status
            } else {
                let status = SbomStatus::new(
                    &each.vulnerability,
                    each.status.slug.clone(),
                    status_cpe,
                    vec![],
                    tx,
                )
                .await?;
                advisory.status.push(status);
                if let Some(status) = advisory.status.last_mut() {
                    status
                } else {
                    return Err(Error::Data("failed to build advisory status".to_string()));
                }
            };

            sbom_status.packages.push(SbomPackage {
                id: each.sbom_package.node_id.clone(),
                name: each.sbom_node.name.clone(),
                version: each.sbom_package.version.clone(),
                purl: vec![
                    PurlSummary::from_entity(
                        &each.base_purl,
                        &each.versioned_purl,
                        &each.qualified_purl,
                        tx,
                    )
                    .await?,
                ],
                cpe: vec![],
            });
        }

        Ok(advisories.values().cloned().collect::<Vec<_>>())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SbomStatus {
    #[serde(flatten)]
    pub vulnerability: VulnerabilityHead,
    pub average_severity: Severity,
    pub status: String,
    pub context: Option<StatusContext>,
    pub packages: Vec<SbomPackage>,
}

impl SbomStatus {
    pub async fn new<C: ConnectionTrait>(
        vulnerability: &vulnerability::Model,
        status: String,
        cpe: Option<OwnedUri>,
        packages: Vec<SbomPackage>,
        tx: &C,
    ) -> Result<Self, Error> {
        let cvss3 = vulnerability.find_related(cvss3::Entity).all(tx).await?;
        let average_severity = Score::from_iter(cvss3.iter().map(Cvss3Base::from)).severity();
        Ok(Self {
            vulnerability: VulnerabilityHead::from_vulnerability_entity(
                vulnerability,
                Memo::NotProvided,
                tx,
            )
            .await?,
            context: cpe.as_ref().map(|e| StatusContext::Cpe(e.to_string())),
            average_severity,
            status,
            packages,
        })
    }
    pub fn identifier(&self) -> &str {
        &self.vulnerability.identifier
    }
}

#[derive(Debug)]
#[allow(dead_code)] //TODO sbom field is not used at the moment, but we will probably need it for graph search
pub struct ProductStatusCatcher {
    advisory: advisory::Model,
    vulnerability: trustify_entity::vulnerability::Model,
    product_status: product_status::Model,
    cpe: trustify_entity::cpe::Model,
    status: status::Model,
    sbom: Option<sbom::Model>,
    base_purl: base_purl::Model,
    versioned_purl: versioned_purl::Model,
    qualified_purl: qualified_purl::Model,
    sbom_package: sbom_package::Model,
    sbom_node: sbom_node::Model,
}

impl FromQueryResult for ProductStatusCatcher {
    fn from_query_result(res: &QueryResult, _pre: &str) -> Result<Self, DbErr> {
        Ok(Self {
            advisory: Self::from_query_result_multi_model(res, "", advisory::Entity)?,
            vulnerability: Self::from_query_result_multi_model(
                res,
                "",
                trustify_entity::vulnerability::Entity,
            )?,
            product_status: Self::from_query_result_multi_model(res, "", product_status::Entity)?,
            cpe: Self::from_query_result_multi_model(res, "", trustify_entity::cpe::Entity)?,
            status: Self::from_query_result_multi_model(res, "", status::Entity)?,
            sbom: Self::from_query_result_multi_model_optional(res, "", sbom::Entity)?,
            base_purl: Self::from_query_result_multi_model(res, "", base_purl::Entity)?,
            versioned_purl: Self::from_query_result_multi_model(res, "", versioned_purl::Entity)?,
            qualified_purl: Self::from_query_result_multi_model(res, "", qualified_purl::Entity)?,
            sbom_package: Self::from_query_result_multi_model(res, "", sbom_package::Entity)?,
            sbom_node: Self::from_query_result_multi_model(res, "", sbom_node::Entity)?,
        })
    }
}

impl FromQueryResultMultiModel for ProductStatusCatcher {
    fn try_into_multi_model<E: EntityTrait>(select: Select<E>) -> Result<Select<E>, DbErr> {
        select
            .try_model_columns(advisory::Entity)?
            .try_model_columns(trustify_entity::vulnerability::Entity)?
            .try_model_columns(product_status::Entity)?
            .try_model_columns(trustify_entity::cpe::Entity)?
            .try_model_columns(status::Entity)?
            .try_model_columns(product_version::Entity)?
            .try_model_columns(base_purl::Entity)?
            .try_model_columns(versioned_purl::Entity)?
            .try_model_columns(qualified_purl::Entity)?
            .try_model_columns(sbom_package::Entity)?
            .try_model_columns(sbom_node::Entity)
    }
}
