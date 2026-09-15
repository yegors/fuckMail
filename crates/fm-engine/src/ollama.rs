//! Same-machine Ollama HTTP. Requests have no tools, proxy or redirects.
use fm_assist::ollama::{self, Model, Settings};
use fm_core::{CoreError, Result};
use fm_google::transport::CancellationToken;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::time::Duration;
pub fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}
pub async fn request(
    endpoint: &str,
    path: &str,
    body: Option<Value>,
    cancel: &CancellationToken,
) -> Result<Value> {
    if !ollama::valid_endpoint(endpoint) {
        return Err(failure(
            "invalid_request",
            "Only same-machine Ollama endpoints are supported",
        ));
    }
    let endpoint = endpoint.replace("localhost:", "127.0.0.1:");
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(if path == "chat" { 120 } else { 5 }))
        .build()
        .map_err(|_| failure("assistance_unavailable", "Cannot initialize local HTTP"))?;
    let mut req = client.request(
        if body.is_some() {
            Method::POST
        } else {
            Method::GET
        },
        format!("{endpoint}/api/{path}"),
    );
    if let Some(body) = body {
        req = req.header("Content-Type", "application/json").body(
            serde_json::to_vec(&body)
                .map_err(|_| failure("invalid_request", "Invalid model request"))?,
        );
    }
    let work = async {
        let mut response = req.send().await.map_err(|_| {
            failure(
                "assistance_unavailable",
                "Ollama is unavailable or timed out",
            )
        })?;
        if !response.status().is_success() {
            return Err(failure(
                "assistance_unavailable",
                &format!("Ollama returned HTTP {}", response.status().as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| failure("assistance_unavailable", "Ollama response was interrupted"))?
        {
            if bytes.len() + chunk.len() > 256 * 1024 {
                return Err(failure(
                    "assistance_limit",
                    "Ollama response exceeds 256 KiB",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| failure("invalid_assistance_result", "Ollama returned invalid JSON"))
    };
    tokio::select! {biased;_=cancel.cancelled()=>Err(failure("assistance_canceled","Local model request canceled")),v=work=>v}
}
pub async fn discover(endpoint: &str, cancel: &CancellationToken) -> Result<Value> {
    let version = request(endpoint, "version", None, cancel).await?;
    let tags = request(endpoint, "tags", None, cancel).await?;
    let models = tags["models"].as_array().ok_or_else(|| {
        failure(
            "invalid_assistance_result",
            "Ollama did not return a model list",
        )
    })?;
    if models.len() > 512 {
        return Err(failure(
            "assistance_limit",
            "Ollama returned too many models",
        ));
    }
    let list:Vec<_>=models.iter().map(|m|{
  let name=m["name"].as_str().unwrap_or_default();
  let remote=name.to_lowercase().contains("cloud") || m.get("remote_host").is_some_and(|v|!v.is_null()) || m.get("remote_model").is_some_and(|v|!v.is_null());
  let local=!remote && m["size"].as_u64().unwrap_or(0)>1024*1024 && m["digest"].as_str().is_some_and(|s|s.len()==64);
  json!({"name":name,"digest":m["digest"],"size":m["size"],"eligible":local,"reason":if remote{"Cloud model"}else if !local{"Local weights unavailable"}else{"Local candidate; checked before use"}})
 }).collect();
    Ok(json!({"version":version["version"],"models":list,"endpoint":endpoint}))
}
async fn qualify(endpoint: &str, model: &Model, cancel: &CancellationToken) -> Result<()> {
    let list = discover(endpoint, cancel).await?;
    let entry = list["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == model.name)
        .ok_or_else(|| {
            failure(
                "assistance_unavailable",
                "Selected model is no longer installed",
            )
        })?;
    if entry["digest"] != model.digest {
        return Err(failure(
            "assistance_stale",
            "Model weights changed; select the installed model again",
        ));
    }
    if entry["eligible"] != true {
        return Err(failure(
            "assistance_unavailable",
            "Selected model is not eligible for local mail processing",
        ));
    }
    let detail = request(endpoint, "show", Some(json!({"model":model.name})), cancel).await?;
    if detail.get("remote_host").is_some_and(|v| !v.is_null())
        || detail.get("remote_model").is_some_and(|v| !v.is_null())
        || !detail["capabilities"]
            .as_array()
            .is_some_and(|caps| caps.iter().any(|c| c == "completion"))
        || !["gguf", "mlx", "safetensors"]
            .contains(&detail["details"]["format"].as_str().unwrap_or_default())
    {
        return Err(failure(
            "assistance_unavailable",
            "Model metadata does not establish installed local completion weights",
        ));
    }
    Ok(())
}
pub async fn infer(
    settings: &Settings,
    task: &str,
    payload: Value,
    categories: &[String],
    sources: &[String],
    cancel: &CancellationToken,
) -> Result<Value> {
    let model = settings
        .model(task)
        .ok_or_else(|| failure("assistance_unavailable", "Select a model for this task"))?;
    qualify(&settings.endpoint, model, cancel).await?;
    let schema = ollama::schema(task, categories, sources);
    let system = format!(
        "{}\nTask instructions:\n{}",
        ollama::ENVELOPE,
        settings.prompt(task)
    );
    let user = serde_json::to_string(&payload)
        .map_err(|_| failure("invalid_request", "Invalid source context"))?;
    if system.len() + user.len() + serde_json::to_vec(&schema).unwrap().len() > 12000 {
        return Err(failure(
            "assistance_limit",
            "Instructions, labels and text exceed the context budget",
        ));
    }
    let result=request(&settings.endpoint,"chat",Some(json!({"model":model.name,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"format":schema,"stream":false,"options":{"temperature":0,"num_ctx":16384,"num_predict":1024}})),cancel).await?;
    if result["model"] != model.name
        || result["done"] != true
        || result["done_reason"] == "length"
        || result["message"]
            .get("tool_calls")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err(failure(
            "invalid_assistance_result",
            "Model returned an incomplete result or a tool call",
        ));
    }
    let content = result["message"]["content"]
        .as_str()
        .ok_or_else(|| failure("invalid_assistance_result", "Model returned no text"))?;
    // A tag may have been replaced while generation was running.
    let list = discover(&settings.endpoint, cancel).await?;
    if !list["models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["name"] == model.name && m["digest"] == model.digest && m["eligible"] == true)
    {
        return Err(failure(
            "assistance_stale",
            "Selected model changed during generation",
        ));
    }
    ollama::validate(task, content, categories, sources)
        .map_err(|s| failure("invalid_assistance_result", &s))
}
