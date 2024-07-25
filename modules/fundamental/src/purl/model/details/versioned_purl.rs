use crate::{
    advisory::model::AdvisoryHead,
    purl::model::{BasePurlHead, PurlHead, VersionedPurlHead},
    vulnerability::model::VulnerabilityHead,
    Error,
};
use sea_orm::{
    ColumnTrait, EntityTrait, LoaderTrait, ModelTrait, QueryFilter, QuerySelect, RelationTrait,
};
use sea_query::{Asterisk, Expr, Func, JoinType, SimpleExpr};
use serde::{Deserialize, Serialize};
use trustify_common::db::{ConnectionOrTransaction, VersionMatches};
use trustify_entity::{
    advisory, base_purl, organization, purl_status, qualified_purl, status, version_range,
    versioned_purl, vulnerability,
};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct VersionedPurlDetails {
    #[serde(flatten)]
    pub head: VersionedPurlHead,
    pub base: BasePurlHead,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub purls: Vec<PurlHead>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<VersionedPurlAdvisory>,
}

impl VersionedPurlDetails {
    pub async fn from_entity(
        package: Option<base_purl::Model>,
        package_version: &versioned_purl::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        let package = if let Some(package) = package {
            package
        } else {
            package_version
                .find_related(base_purl::Entity)
                .one(tx)
                .await?
                .ok_or(Error::Data("underlying package missing".to_string()))?
        };

        let qualified_packages = package_version
            .find_related(qualified_purl::Entity)
            .all(tx)
            .await?;

        let qualified_packages =
            PurlHead::from_entities(&package, package_version, &qualified_packages, tx).await?;

        let statuses = purl_status::Entity::find()
            .columns([
                version_range::Column::Id,
                version_range::Column::LowVersion,
                version_range::Column::LowInclusive,
                version_range::Column::HighVersion,
                version_range::Column::HighInclusive,
            ])
            .left_join(base_purl::Entity)
            .join(
                JoinType::LeftJoin,
                base_purl::Relation::VersionedPurls.def(),
            )
            .left_join(version_range::Entity)
            .filter(purl_status::Column::BasePurlId.eq(package.id))
            .filter(SimpleExpr::FunctionCall(
                Func::cust(VersionMatches)
                    .arg(Expr::col(versioned_purl::Column::Version))
                    .arg(Expr::col((version_range::Entity, Asterisk))),
            ))
            .all(tx)
            .await?;

        Ok(Self {
            head: VersionedPurlHead::from_entity(&package, package_version, tx).await?,
            base: BasePurlHead::from_entity(&package, tx).await?,
            purls: qualified_packages,
            advisories: VersionedPurlAdvisory::from_entities(statuses, tx).await?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct VersionedPurlAdvisory {
    #[serde(flatten)]
    pub head: AdvisoryHead,
    pub status: Vec<VersionedPurlStatus>,
}

impl VersionedPurlAdvisory {
    pub async fn from_entities(
        statuses: Vec<purl_status::Model>,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Vec<Self>, Error> {
        let vulns = statuses.load_one(vulnerability::Entity, tx).await?;

        let advisories = statuses.load_one(advisory::Entity, tx).await?;

        let mut results: Vec<Self> = Vec::new();

        for ((vuln, advisory), status) in vulns.iter().zip(advisories.iter()).zip(statuses.iter()) {
            if let (Some(vulnerability), Some(advisory)) = (vuln, advisory) {
                let qualified_package_status =
                    VersionedPurlStatus::from_entity(vulnerability, status, tx).await?;

                if let Some(entry) = results.iter_mut().find(|e| e.head.uuid == advisory.id) {
                    entry.status.push(qualified_package_status)
                } else {
                    let organization = advisory.find_related(organization::Entity).one(tx).await?;

                    results.push(Self {
                        head: AdvisoryHead::from_advisory(advisory, Some(organization), tx).await?,
                        status: vec![qualified_package_status],
                    })
                }
            }
        }

        Ok(results)
    }
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct VersionedPurlStatus {
    pub vulnerability: VulnerabilityHead,
    pub status: String,
}

impl VersionedPurlStatus {
    pub async fn from_entity(
        vuln: &vulnerability::Model,
        package_status: &purl_status::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        let status = package_status.find_related(status::Entity).one(tx).await?;

        let status = status.map(|e| e.slug).unwrap_or("unknown".to_string());

        Ok(Self {
            vulnerability: VulnerabilityHead::from_vulnerability_entity(vuln, None, tx).await?,
            status,
        })
    }
}
