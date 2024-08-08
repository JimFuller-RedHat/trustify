use crate::{dataset::Instructions, openapi::write_openapi};
use clap::Parser;
use schemars::JsonSchema;
use std::{
    io::BufWriter,
    path::{Path, PathBuf},
};
use trustify_auth::auth::AuthConfig;

#[derive(Debug, Parser)]
pub struct GenerateSchema {
    #[arg(short, long, default_value = ".")]
    base: PathBuf,
}

impl GenerateSchema {
    pub async fn run(self) -> anyhow::Result<()> {
        // write schema

        self.write_schema::<Instructions>("xtask/schema/generate-dump.json")?;
        self.write_schema::<AuthConfig>("common/auth/schema/auth.json")?;

        // write openapi spec

        write_openapi(Some(&self.base))?;

        // done

        Ok(())
    }

    fn write_schema<S: JsonSchema>(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = self.base.join(path);

        let schema = schemars::schema_for!(S);
        serde_json::to_writer_pretty(BufWriter::new(std::fs::File::create(path)?), &schema)?;

        Ok(())
    }
}
