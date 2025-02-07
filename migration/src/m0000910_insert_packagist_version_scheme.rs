use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
#[allow(deprecated)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        insert(
            db,
            "packagist",
            "PHP Packagist",
            Some("https://packagist.org/about#managing-package-versions"),
        )
        .await?;

        db.execute_unprepared(include_str!(
            "m0000910_insert_packagist_version_scheme/version_matches.sql"
        ))
        .await
        .map(|_| ())?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(include_str!("m0000850_python_version/version_matches.sql"))
            .await
            .map(|_| ())?;

        delete(db, "packagist").await?;

        Ok(())
    }
}

async fn insert(
    db: &SchemaManagerConnection<'_>,
    id: &str,
    name: &str,
    description: Option<&str>,
) -> Result<(), DbErr> {
    db.execute(
        db.get_database_backend().build(
            Query::insert()
                .into_table(VersionScheme::Table)
                .columns([
                    VersionScheme::Id,
                    VersionScheme::Name,
                    VersionScheme::Description,
                ])
                .values([
                    SimpleExpr::Value(Value::String(Some(Box::new(id.to_string())))),
                    SimpleExpr::Value(Value::String(Some(Box::new(name.to_string())))),
                    SimpleExpr::Value(Value::String(description.map(|e| Box::new(e.to_string())))),
                ])
                .map_err(|e| DbErr::Custom(e.to_string()))?,
        ),
    )
    .await?;
    Ok(())
}

async fn delete(db: &SchemaManagerConnection<'_>, version_scheme: &str) -> Result<(), DbErr> {
    db.execute(
        db.get_database_backend().build(
            Query::delete()
                .from_table(VersionScheme::Table)
                .and_where(Expr::col(VersionScheme::Id).eq(version_scheme.to_string())),
        ),
    )
    .await?;
    Ok(())
}

#[derive(DeriveIden)]
enum VersionScheme {
    Table,
    Id,
    Name,
    Description,
}
