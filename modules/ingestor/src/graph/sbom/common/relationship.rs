use anyhow::bail;
use sea_orm::{ActiveValue::Set, ConnectionTrait, DbErr, EntityTrait};
use sea_query::OnConflict;
use std::collections::HashSet;
use trustify_common::db::chunk::EntityChunkedIter;
use trustify_entity::{package_relates_to_package, relationship::Relationship};
use uuid::Uuid;

// Creator of relationships.
pub struct RelationshipCreator {
    sbom_id: Uuid,
    rels: Vec<package_relates_to_package::ActiveModel>,
}

impl RelationshipCreator {
    pub fn new(sbom_id: Uuid) -> Self {
        Self {
            sbom_id,

            rels: Vec::new(),
        }
    }

    pub fn with_capacity(sbom_id: Uuid, capacity_rel: usize) -> Self {
        Self {
            sbom_id,

            rels: Vec::with_capacity(capacity_rel),
        }
    }

    pub fn relate(&mut self, left: String, rel: Relationship, right: String) {
        self.rels.push(package_relates_to_package::ActiveModel {
            sbom_id: Set(self.sbom_id),
            left_node_id: Set(left),
            relationship: Set(rel),
            right_node_id: Set(right),
        });
    }

    /// Pre-flight check to see if all relationships can be inserted.
    ///
    /// This expects a source of references to check against. If creating a fresh set of nodes and
    /// relationships, these sources would most likely be the creators (like [`super::PackageCreator`]).
    /// If nodes already exist in the database, those nodes would need to be extracted and provided.
    pub fn validate(&self, sources: References) -> Result<(), anyhow::Error> {
        for rel in &self.rels {
            if let Set(left) = &rel.left_node_id {
                if !sources.refs.contains(left.as_str()) {
                    bail!("Invalid SPDX reference: {left}");
                }
            }
            if let Set(right) = &rel.right_node_id {
                if !sources.refs.contains(right.as_str()) {
                    bail!("Invalid SPDX reference: {right}");
                }
            }
        }

        Ok(())
    }

    pub async fn create(self, db: &impl ConnectionTrait) -> Result<(), DbErr> {
        for batch in &self.rels.into_iter().chunked() {
            package_relates_to_package::Entity::insert_many(batch)
                .on_conflict(
                    OnConflict::columns([
                        package_relates_to_package::Column::SbomId,
                        package_relates_to_package::Column::LeftNodeId,
                        package_relates_to_package::Column::Relationship,
                        package_relates_to_package::Column::RightNodeId,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .do_nothing()
                .exec(db)
                .await?;
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct References<'a> {
    pub refs: HashSet<&'a str>,
}

impl<'a> IntoIterator for References<'a> {
    type Item = &'a str;
    type IntoIter = std::collections::hash_set::IntoIter<&'a str>;

    fn into_iter(self) -> Self::IntoIter {
        self.refs.into_iter()
    }
}

impl<'a> References<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_source<S>(mut self, source: &'a S) -> Self
    where
        S: ReferenceSource<'a> + 'a,
    {
        self.refs.extend(source.references());
        self
    }
}

/// A source of SBOM node references for validating.
pub trait ReferenceSource<'a> {
    fn references(&'a self) -> impl IntoIterator<Item = &'a str>;
}

impl<'a> ReferenceSource<'a> for &'a [&'a str] {
    fn references(&'a self) -> impl IntoIterator<Item = &'a str> {
        self.iter().copied()
    }
}

impl<'a, const N: usize> ReferenceSource<'a> for [&'a str; N] {
    fn references(&'a self) -> impl IntoIterator<Item = &'a str> {
        self.iter().copied()
    }
}
