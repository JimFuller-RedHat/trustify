use async_graphql::SimpleObject;
use sea_orm::entity::prelude::*;
use sea_orm::LinkDef;
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, SimpleObject)]
#[sea_orm(table_name = "sbom")]
#[graphql(concrete(name = "Sbom", params()))]
pub struct Model {
    #[sea_orm(primary_key)]
    pub sbom_id: Uuid,
    pub node_id: String,

    pub location: String,
    pub sha256: String,
    pub document_id: String,

    pub published: Option<OffsetDateTime>,
    pub authors: Vec<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::sbom_package::Entity")]
    Packages,
    #[sea_orm(has_one = "super::sbom_node::Entity")]
    Node,
}

pub struct SbomNodeLink;

impl Linked for SbomNodeLink {
    type FromEntity = Entity;
    type ToEntity = super::sbom_node::Entity;

    fn link(&self) -> Vec<LinkDef> {
        vec![super::sbom_node::Relation::SbomNode.def().rev()]
    }
}

impl Related<super::sbom_package::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Packages.def()
    }
}

impl Related<super::sbom_node::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Node.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
