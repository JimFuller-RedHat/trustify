use crate::graph::sbom::spdx::{parse_spdx, Information};
use crate::service::Error;
use anyhow::anyhow;
use bytes::Bytes;
use futures::Stream;
use std::time::Instant;
use trustify_module_storage::service::{StorageBackend, SyncAdapter};
use uuid::Uuid;

impl super::IngestorService {
    pub async fn ingest_sbom<S, E>(&self, source: &str, stream: S) -> Result<Uuid, Error>
    where
        E: std::error::Error,
        S: Stream<Item = Result<Bytes, E>>,
    {
        let start = Instant::now();

        let digest = self
            .storage
            .store(stream)
            .await
            .map_err(|err| Error::Storage(anyhow!("{err}")))?;
        let sha256 = hex::encode(digest);
        let storage = SyncAdapter::new(self.storage.clone());
        let data = storage
            .retrieve(sha256.clone())
            .await
            .map_err(Error::Storage)?
            .ok_or_else(|| Error::Storage(anyhow!("File went missing during upload")))?;

        log::debug!("Source: {source}");

        // FIXME: consider adding a report entry in case of "fixing" things
        let (spdx, _) = parse_spdx(data)?;

        log::info!(
            "Storing: {}",
            spdx.document_creation_information.document_name
        );

        let tx = self.graph.transaction().await?;

        let document_id = &spdx.document_creation_information.spdx_document_namespace;

        let sbom = self
            .graph
            .ingest_sbom(source, &sha256, document_id, Information(&spdx), &tx)
            .await?;

        sbom.ingest_spdx(spdx, &tx).await.map_err(Error::Generic)?;

        tx.commit().await?;

        let duration = Instant::now() - start;
        log::info!("Ingested - took {}", humantime::Duration::from(duration));

        Ok(sbom.sbom.sbom_id)
    }
}
