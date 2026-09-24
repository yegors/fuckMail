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
    request_local(
        endpoint,
        &format!("/api/{path}"),
        "Ollama",
        path == "chat",
        body,
        cancel,
    )
    .await
}
pub(crate) async fn request_local(
    endpoint: &str,
    path: &str,
    service: &str,
    generation: bool,
    body: Option<Value>,
    cancel: &CancellationToken,
) -> Result<Value> {
    if !ollama::valid_endpoint(endpoint) {
        return Err(failure(
            "invalid_request",
            "Only same-machine model endpoints are supported",
        ));
    }
    let endpoint = endpoint.replace("localhost:", "127.0.0.1:");
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(if generation { 120 } else { 5 }))
        .build()
        .map_err(|_| failure("assistance_unavailable", "Cannot initialize local HTTP"))?;
    let mut req = client.request(
        if body.is_some() {
            Method::POST
        } else {
            Method::GET
        },
        format!("{endpoint}{path}"),
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
                &format!("{service} is unavailable or timed out"),
            )
        })?;
        if !response.status().is_success() {
            return Err(failure(
                "assistance_unavailable",
                &format!("{service} returned HTTP {}", response.status().as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            failure(
                "assistance_unavailable",
                &format!("{service} response was interrupted"),
            )
        })? {
            if bytes.len() + chunk.len() > 256 * 1024 {
                return Err(failure(
                    "assistance_limit",
                    "Local model response exceeds 256 KiB",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            failure(
                "invalid_assistance_result",
                &format!("{service} returned invalid JSON"),
            )
        })
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
    if !settings.provider.is_ollama() {
        return crate::openai_local::infer(settings, task, payload, categories, sources, cancel)
            .await;
    }
    let model = settings
        .model(task)
        .ok_or_else(|| failure("assistance_unavailable", "Select a model for this task"))?;
    qualify(&settings.endpoint, model, cancel).await?;
    let mut body = ollama::render_request(settings.prompt(task), &payload, &model.name, sources)
        .map_err(|s| failure("invalid_request", &s))?;
    if serde_json::to_vec(&body).unwrap().len() > ollama::INITIAL_REQUEST_BYTES {
        return Err(failure(
            "assistance_limit",
            "Expanded request exceeds the context budget",
        ));
    }
    let generation = async {
        for attempt in 0..2 {
            let result = request(&settings.endpoint, "chat", Some(body.clone()), cancel).await?;
            match response_output(task, &result, &model.name, categories, sources) {
                Ok(output) => return Ok(output),
                Err(error)
                    if attempt == 0
                        && ["assistance_output_limit", "assistance_format"]
                            .contains(&error.code.as_str()) =>
                {
                    crate::diagnostics::event("Model", "format_retry", json!({"code":error.code,"attempt":1,"task":task}));
                    // Retry only format/completion failures once. The rejected
                    // output never enters the prompt, store or mail actions.
                    body["messages"].as_array_mut().unwrap().push(json!({"role":"user","content":"Return a complete JSON object with exactly sender_kind, category_ids, category_abstain, reason, summary, source_ids. Use a short factual prose summary, not a code block. Copy IDs from the supplied data only. With no matching label use category_ids:[]; if category_abstain is true the array must be empty. No reasoning or extra text."}));
                    if serde_json::to_vec(&body).unwrap().len() > 12000 {
                        return Err(failure(
                            "assistance_limit",
                            "Correction request exceeds the context budget",
                        ));
                    }
                }
                Err(mut error) => {
                    if error.code == "assistance_format" {
                        error.code = "invalid_assistance_result".into();
                    }
                    return Err(error);
                }
            }
        }
        unreachable!()
    };
    let output = tokio::time::timeout(Duration::from_secs(120), generation)
        .await
        .map_err(|_| {
            failure(
                "assistance_unavailable",
                "Ollama analysis timed out after 120 seconds",
            )
        })??;
    // A tag may have been replaced while generation or correction was running.
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
    Ok(output)
}
fn response_output(
    task: &str,
    result: &Value,
    model: &str,
    categories: &[String],
    sources: &[String],
) -> Result<Value> {
    if result["model"] != model {
        return Err(failure(
            "invalid_assistance_result",
            "Ollama responded with a different model",
        ));
    }
    if result["message"]
        .get("tool_calls")
        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err(failure(
            "invalid_assistance_result",
            "The model requested a tool; mail analysis cannot use tools",
        ));
    }
    if result["done_reason"] == "length" {
        return Err(failure(
            "assistance_output_limit",
            "The model reached its response limit before finishing. Try again.",
        ));
    }
    if result["done"] != true {
        return Err(failure(
            "assistance_format",
            "Ollama stopped before completing the response. Try again.",
        ));
    }
    let content = result["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            failure(
                "assistance_format",
                "The model returned no analysis text. Try again.",
            )
        })?;
    validate_output(task, content, categories, sources).map_err(|mut error| {
        error.code = "assistance_format".into();
        error
    })
}
pub(crate) fn validate_output(
    task: &str,
    content: &str,
    categories: &[String],
    sources: &[String],
) -> Result<Value> {
    let mut output = ollama::validate("message", content, categories, sources)
        .map_err(|s| failure("invalid_assistance_result", &s))?;
    // Legacy CLI operations project the same combined analysis after validation.
    if task == "summary" {
        output = json!({"summary":output["summary"],"source_ids":output["source_ids"]});
    } else if task == "classification" {
        output.as_object_mut().unwrap().remove("summary");
        output.as_object_mut().unwrap().remove("source_ids");
    }
    Ok(output)
}
