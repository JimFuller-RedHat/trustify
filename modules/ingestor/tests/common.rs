use trustify_module_analysis::config::AnalysisConfig;
use trustify_module_analysis::service::AnalysisService;
use trustify_module_ingestor::endpoints::{Config, configure};
use trustify_test_context::{
    TrustifyContext,
    call::{self, CallService},
};

pub async fn caller_with(
    ctx: &TrustifyContext,
    config: Config,
) -> anyhow::Result<impl CallService + '_> {
    let analysis = AnalysisService::new(AnalysisConfig::default());
    call::caller(|svc| {
        configure(
            svc,
            config,
            ctx.db.clone(),
            ctx.storage.clone(),
            Some(analysis),
        )
    })
    .await
}
