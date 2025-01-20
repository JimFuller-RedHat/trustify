use crate::test::caller;
use actix_http::Request;
use actix_web::test::TestRequest;
use serde_json::{json, Value};
use test_context::test_context;
use test_log::test;
use trustify_test_context::{call::CallService, TrustifyContext};

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_retrieve_analysis_endpoint(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    //should match multiple components
    let uri = "/api/v2/analysis/root-component?q=B";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    if response["items"][0]["purl"]
        .as_array()
        .unwrap()
        .contains(&Value::from("pkg:rpm/redhat/BB@0.0.0"))
        || response["items"][1]["purl"]
            .as_array()
            .unwrap()
            .contains(&Value::from("pkg:rpm/redhat/BB@0.0.0"))
    {
        assert_eq!(&response["total"], 2);
    } else {
        panic!("one of the items component should have matched.");
    }
    log::info!("{:?}", response);

    //should match a single component
    let uri = "/api/v2/analysis/root-component?q=BB";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/BB@0.0.0"])
    );
    assert_eq!(
        response["items"][0]["ancestors"][0]["purl"],
        Value::from(["pkg:rpm/redhat/AA@0.0.0?arch=src"])
    );

    assert_eq!(&response["total"], 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_retrieve_by_name_analysis_endpoint(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let uri = "/api/v2/analysis/root-component/B";

    let request: Request = TestRequest::get().uri(uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/B@0.0.0"])
    );
    assert_eq!(
        response["items"][0]["ancestors"][0]["purl"],
        Value::from(["pkg:rpm/redhat/A@0.0.0?arch=src"])
    );
    assert_eq!(&response["total"], 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_retrieve_by_purl_analysis_endpoint(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let uri = "/api/v2/analysis/root-component/pkg%3A%2F%2Frpm%2Fredhat%2FB%400.0.0";

    let request: Request = TestRequest::get().uri(uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/B@0.0.0"])
    );
    assert_eq!(
        response["items"][0]["ancestors"][0]["purl"],
        Value::from(["pkg:rpm/redhat/A@0.0.0?arch=src"])
    );
    assert_eq!(&response["total"], 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_quarkus_retrieve_analysis_endpoint(
    ctx: &TrustifyContext,
) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let uri = "/api/v2/analysis/root-component?q=spymemcached";

    let request: Request = TestRequest::get().uri(uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:maven/net.spy/spymemcached@2.12.1?type=jar"])
    );
    assert_eq!(
        response["items"][0]["document_id"],
        "https://access.redhat.com/security/data/sbom/spdx/quarkus-bom-3.2.11.Final-redhat-00001"
    );
    assert_eq!(
        response["items"][0]["ancestors"][0]["purl"],
        Value::from(["pkg:maven/com.redhat.quarkus.platform/quarkus-bom@3.2.11.Final-redhat-00001?repository_url=https://maven.repository.redhat.com/ga/&type=pom"])
    );

    assert_eq!(&response["total"], 2);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_status_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    // prime the graph hashmap
    let uri = "/api/v2/analysis/root-component?q=BB";
    let load1 = TestRequest::get().uri(uri).to_request();
    let _response: Value = app.call_and_read_body_json(load1).await;

    let uri = "/api/v2/analysis/status";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(response["sbom_count"], 1);
    assert_eq!(response["graph_count"], 1);

    // ingest duplicate sbom which has different date
    ctx.ingest_documents(["spdx/simple-dup.json"]).await?;

    let uri = "/api/v2/analysis/status";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(response["sbom_count"], 2);
    assert_eq!(response["graph_count"], 1);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_dep_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let uri = "/api/v2/analysis/dep?q=A";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    println!("Result: {response:#?}");

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/A@0.0.0?arch=src"]),
    );
    assert_eq!(
        response["items"][0]["deps"][0]["purl"],
        Value::from(["pkg:rpm/redhat/EE@0.0.0?arch=src"]),
    );
    assert_eq!(
        response["items"][0]["deps"][1]["purl"],
        Value::from(["pkg:rpm/redhat/B@0.0.0"]),
    );

    assert_eq!(&response["total"], 2);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_dep_by_name_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let uri = "/api/v2/analysis/dep/A";

    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/A@0.0.0?arch=src"]),
    );
    assert_eq!(
        response["items"][0]["deps"][0]["purl"],
        Value::from(["pkg:rpm/redhat/EE@0.0.0?arch=src"]),
    );
    assert_eq!(
        response["items"][0]["deps"][1]["purl"],
        Value::from(["pkg:rpm/redhat/B@0.0.0"]),
    );

    assert_eq!(&response["total"], 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_simple_dep_by_purl_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    let uri = "/api/v2/analysis/dep/pkg%3A%2F%2Frpm%2Fredhat%2FAA%400.0.0%3Farch%3Dsrc";

    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:rpm/redhat/AA@0.0.0?arch=src"]),
    );
    assert_eq!(
        response["items"][0]["deps"][0]["purl"],
        Value::from(["pkg:rpm/redhat/BB@0.0.0"]),
    );
    assert_eq!(&response["total"], 1);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_quarkus_dep_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let uri = "/api/v2/analysis/dep?q=spymemcached";

    let request: Request = TestRequest::get().uri(uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        Value::from(["pkg:maven/net.spy/spymemcached@2.12.1?type=jar"]),
    );
    assert_eq!(&response["total"], 2);
    Ok(())
}

/// find a component by purl
#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn quarkus_component_by_purl(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let uri = format!(
        "/api/v2/analysis/component/{}",
        urlencoding::encode("pkg:maven/com.redhat.quarkus.platform/quarkus-bom@3.2.11.Final-redhat-00001?repository_url=https://maven.repository.redhat.com/ga/&type=pom")
    );

    let request: Request = TestRequest::get().uri(&uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["purl"],
        json!(["pkg:maven/com.redhat.quarkus.platform/quarkus-bom@3.2.11.Final-redhat-00001?repository_url=https://maven.repository.redhat.com/ga/&type=pom"]),
    );
    assert_eq!(
        response["items"][0]["cpe"],
        json!(["cpe:/a:redhat:quarkus:3.2:*:el8:*"]),
    );
    assert_eq!(&response["total"], 1);

    Ok(())
}

/// find a component by cpe
#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn quarkus_component_by_cpe(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents([
        "spdx/quarkus-bom-3.2.11.Final-redhat-00001.json",
        "spdx/quarkus-bom-3.2.12.Final-redhat-00002.json",
    ])
    .await?;

    let uri = format!(
        "/api/v2/analysis/component/{}",
        urlencoding::encode("cpe:/a:redhat:quarkus:3.2::el8")
    );

    let request: Request = TestRequest::get().uri(&uri).to_request();

    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(
        response["items"][0]["cpe"],
        json!(["cpe:/a:redhat:quarkus:3.2:*:el8:*"]),
    );
    assert_eq!(
        response["items"][0]["purl"],
        json!(["pkg:maven/com.redhat.quarkus.platform/quarkus-bom@3.2.11.Final-redhat-00001?repository_url=https://maven.repository.redhat.com/ga/&type=pom"]),
    );
    assert_eq!(&response["total"], 2);

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn test_retrieve_query_params_endpoint(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/simple.json"]).await?;

    // filter on node_id
    let uri = "/api/v2/analysis/dep?q=node_id%3DSPDXRef-A";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(response["items"][0]["name"], "A");
    assert_eq!(&response["total"], 1);

    // filter on node_id
    let uri = "/api/v2/analysis/root-component?q=node_id%3DSPDXRef-B";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(response["items"][0]["name"], "B");
    assert_eq!(&response["total"], 1);

    // filter on node_id & name
    let uri = "/api/v2/analysis/root-component?q=node_id%3DSPDXRef-B%26name%3DB";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(response["items"][0]["name"], "B");
    assert_eq!(&response["total"], 1);

    // filter on sbom_id (which has urn:uuid: prefix)
    let sbom_id = response["items"][0]["sbom_id"].as_str().unwrap();
    let uri = format!("/api/v2/analysis/root-component?q=sbom_id={}", sbom_id);
    let request: Request = TestRequest::get().uri(uri.clone().as_str()).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(&response["total"], 8);

    // negative test
    let uri =
        "/api/v2/analysis/root-component?q=sbom_id=urn:uuid:99999999-9999-9999-9999-999999999999";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    assert_eq!(&response["total"], 0);

    // negative test
    let uri = "/api/v2/analysis/root-component?q=node_id%3DSPDXRef-B%26name%3DA";
    let request: Request = TestRequest::get().uri(uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;

    assert_eq!(&response["total"], 0);
    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn issue_tc_2050(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["cyclonedx/openssl-3.0.7-18.el9_2.cdx_1.6.sbom.json"])
        .await?;

    // Find all deps of src rpm
    let src = "pkg:rpm/redhat/openssl@3.0.7-18.el9_2?arch=src";
    let uri = format!("/api/v2/analysis/dep/{}", urlencoding::encode(src));
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(
        35,
        response["items"][0]["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["relationship"] == "GeneratedFrom")
            .count()
    );

    // Ensure binary rpm GeneratedFrom src rpm
    let x86 = "pkg:rpm/redhat/openssl@3.0.7-18.el9_2?arch=x86_64";
    let uri = format!(
        "/api/v2/analysis/root-component/{}",
        urlencoding::encode(x86)
    );
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(
        "GeneratedFrom",
        response["items"][0]["ancestors"][0]["relationship"]
    );
    assert_eq!(
        Value::from(vec![Value::from(src)]),
        response["items"][0]["ancestors"][0]["purl"]
    );

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn issue_tc_2051(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["spdx/openssl-3.0.7-18.el9_2.spdx.json"])
        .await?;

    // Find all deps of src rpm
    let src = "pkg:rpm/redhat/openssl@3.0.7-18.el9_2?arch=src";
    let uri = format!("/api/v2/analysis/dep/{}", urlencoding::encode(src));
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(
        35,
        response["items"][0]["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["relationship"] == "GeneratedFrom")
            .count()
    );

    // Ensure binary rpm GeneratedFrom src rpm
    let x86 = "pkg:rpm/redhat/openssl@3.0.7-18.el9_2?arch=x86_64";
    let uri = format!(
        "/api/v2/analysis/root-component/{}",
        urlencoding::encode(x86)
    );
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(
        "GeneratedFrom",
        response["items"][0]["ancestors"][0]["relationship"]
    );
    assert_eq!(
        Value::from(vec![Value::from(src)]),
        response["items"][0]["ancestors"][0]["purl"]
    );

    Ok(())
}

#[test_context(TrustifyContext)]
#[test(actix_web::test)]
async fn issue_tc_2052(ctx: &TrustifyContext) -> Result<(), anyhow::Error> {
    let app = caller(ctx).await?;
    ctx.ingest_documents(["cyclonedx/66FF73123BB3489.json"])
        .await?;

    // Find all deps of parent
    let parent = "pkg:oci/openshift-ose-console@sha256:94a0d7feec34600a858c8e383ee0e8d5f4a077f6bbc327dcad8762acfcf40679";
    let uri = format!("/api/v2/analysis/dep/{}", urlencoding::encode(parent));
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(1, response["items"][0]["deps"].as_array().unwrap().len());

    // Ensure child is variant of src
    let child = "pkg:oci/ose-console@sha256:c2d69e860b7457eb42f550ba2559a0452ec3e5c9ff6521d758c186266247678e?arch=s390x&os=linux&tag=v4.14.0-202412110104.p0.g350e1ea.assembly.stream.el8";
    let uri = format!(
        "/api/v2/analysis/root-component/{}",
        urlencoding::encode(child)
    );
    let request: Request = TestRequest::get().uri(&uri).to_request();
    let response: Value = app.call_and_read_body_json(request).await;
    log::debug!("{response:#?}");
    assert_eq!(
        "VariantOf",
        response["items"][0]["ancestors"][0]["relationship"]
    );
    assert_eq!(
        Value::from(vec![Value::from(parent)]),
        response["items"][0]["ancestors"][0]["purl"]
    );

    Ok(())
}
