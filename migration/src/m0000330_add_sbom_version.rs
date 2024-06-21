use crate::m0000250_create_sbom_package::SbomPackage;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(SbomPackage::Table)
                    .add_column(ColumnDef::new(SbomPackage::Version).string())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(SbomPackage::Table)
                    .drop_column(SbomPackage::Version)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
