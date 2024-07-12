use crate::purl::model::{BasePurlHead, PurlHead, VersionedPurlHead};
use crate::Error;
use sea_orm::LoaderTrait;
use serde::{Deserialize, Serialize};
use trustify_common::db::ConnectionOrTransaction;
use trustify_entity::{base_purl, qualified_purl, versioned_purl};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct VersionedPurlSummary {
    #[serde(flatten)]
    pub head: VersionedPurlHead,
    pub base: BasePurlHead,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub purls: Vec<PurlHead>,
}

impl VersionedPurlSummary {
    pub async fn from_entities_with_common_package(
        package: &base_purl::Model,
        package_versions: &Vec<versioned_purl::Model>,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Vec<Self>, Error> {
        let mut summaries = Vec::new();

        let qualified_packages = package_versions
            .load_many(qualified_purl::Entity, tx)
            .await?;

        for (package_version, qualified_packages) in
            package_versions.iter().zip(qualified_packages.iter())
        {
            summaries.push(Self {
                head: VersionedPurlHead::from_entity(package, package_version, tx).await?,
                base: BasePurlHead::from_entity(package, tx).await?,
                purls: PurlHead::from_entities(package, package_version, qualified_packages, tx)
                    .await?,
            })
        }

        Ok(summaries)
    }

    pub async fn from_entity(
        base_purl: &base_purl::Model,
        versioned_purl: &versioned_purl::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        Ok(Self {
            head: VersionedPurlHead::from_entity(base_purl, versioned_purl, tx).await?,
            base: BasePurlHead::from_entity(base_purl, tx).await?,
            purls: vec![],
        })
    }
}
