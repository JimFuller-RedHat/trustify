use crate::{
    advisory::model::AdvisoryHead,
    package::model::{PackageHead, PackageVersionHead, QualifiedPackageHead},
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
    advisory, base_purl, organization, package_status, qualified_purl, status, version_range,
    versioned_purl, vulnerability,
};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct QualifiedPackageDetails {
    #[serde(flatten)]
    pub head: QualifiedPackageHead,
    pub version: PackageVersionHead,
    pub base: PackageHead,
    pub advisories: Vec<QualifiedPackageAdvisory>,
    // TODO link to advisories, sboms, etc
}

impl QualifiedPackageDetails {
    pub async fn from_entity(
        package: Option<base_purl::Model>,
        package_version: Option<versioned_purl::Model>,
        qualified_package: &qualified_purl::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        let package_version = if let Some(package_version) = package_version {
            package_version
        } else {
            qualified_package
                .find_related(versioned_purl::Entity)
                .one(tx)
                .await?
                .ok_or(Error::Data(
                    "underlying package-version missing".to_string(),
                ))?
        };

        let package = if let Some(package) = package {
            package
        } else {
            package_version
                .find_related(base_purl::Entity)
                .one(tx)
                .await?
                .ok_or(Error::Data("underlying package missing".to_string()))?
        };

        let statuses = package_status::Entity::find()
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
            .filter(package_status::Column::PackageId.eq(package.id))
            .filter(SimpleExpr::FunctionCall(
                Func::cust(VersionMatches)
                    .arg(Expr::col(versioned_purl::Column::Version))
                    .arg(Expr::col((version_range::Entity, Asterisk))),
            ))
            .all(tx)
            .await?;

        Ok(QualifiedPackageDetails {
            head: QualifiedPackageHead::from_entity(
                &package,
                &package_version,
                qualified_package,
                tx,
            )
            .await?,
            version: PackageVersionHead::from_entity(&package, &package_version, tx).await?,
            base: PackageHead::from_entity(&package, tx).await?,
            advisories: QualifiedPackageAdvisory::from_entities(statuses, tx).await?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct QualifiedPackageAdvisory {
    #[serde(flatten)]
    pub head: AdvisoryHead,
    pub status: Vec<QualifiedPackageStatus>,
}

impl QualifiedPackageAdvisory {
    pub async fn from_entities(
        statuses: Vec<package_status::Model>,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Vec<Self>, Error> {
        let vulns = statuses.load_one(vulnerability::Entity, tx).await?;

        let advisories = statuses.load_one(advisory::Entity, tx).await?;

        let mut results: Vec<QualifiedPackageAdvisory> = Vec::new();

        for ((vuln, advisory), status) in vulns.iter().zip(advisories.iter()).zip(statuses.iter()) {
            if let (Some(vulnerability), Some(advisory)) = (vuln, advisory) {
                let qualified_package_status =
                    QualifiedPackageStatus::from_entity(vulnerability, status, tx).await?;

                if let Some(entry) = results.iter_mut().find(|e| e.head.uuid == advisory.id) {
                    entry.status.push(qualified_package_status)
                } else {
                    let organization = advisory.find_related(organization::Entity).one(tx).await?;

                    results.push(Self {
                        head: AdvisoryHead::from_advisory(advisory, organization, tx).await?,
                        status: vec![qualified_package_status],
                    })
                }
            }
        }

        Ok(results)
    }
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct QualifiedPackageStatus {
    pub vulnerability: VulnerabilityHead,
    pub status: String,
}

impl QualifiedPackageStatus {
    pub async fn from_entity(
        vuln: &vulnerability::Model,
        package_status: &package_status::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        let status = status::Entity::find_by_id(package_status.status_id)
            .one(tx)
            .await?;

        let status = status.map(|e| e.slug).unwrap_or("unknown".to_string());

        Ok(Self {
            vulnerability: VulnerabilityHead::from_vulnerability_entity(vuln),
            status,
        })
    }
}
