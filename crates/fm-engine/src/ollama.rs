//! Same-machine Ollama HTTP. Requests have no tools, proxy or redirects.
use fm_assist::ollama::{self, Settings};
use fm_core::{CoreError, Result};
use fm_google::transport::CancellationToken;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::time::Duration;
/// An engine error with `code` and `message` and no details.
pub fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}
/// Calls Ollama's `/api/{path}` through [`request_local`]; `chat` counts as
/// generation for the timeout.
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
/// One HTTP client for every local model call: no proxy, no redirects.
fn client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("local HTTP client")
    })
}
/// Sends one JSON request to a same-machine model server: GET without a
/// body, POST with one.
///
/// Endpoints that are not on this machine are refused. The reply must be
/// JSON of at most 256 KiB. Generation requests time out after 120 s
/// (`assistance_timeout`, a slow email) and others after 5 s
/// (`assistance_unavailable`). Cancellation returns `assistance_canceled`.
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
    let mut req = client()
        .request(
            if body.is_some() {
                Method::POST
            } else {
                Method::GET
            },
            format!("{endpoint}{path}"),
        )
        .timeout(Duration::from_secs(if generation { 120 } else { 5 }));
    if let Some(body) = body {
        req = req.header("Content-Type", "application/json").body(
            serde_json::to_vec(&body)
                .map_err(|_| failure("invalid_request", "Invalid model request"))?,
        );
    }
    let work = async {
        let mut response = req.send().await.map_err(|e| {
            if e.is_timeout() && generation {
                // One slow email, not an unavailable runner.
                failure(
                    "assistance_timeout",
                    &format!("{service} did not answer within 120 seconds"),
                )
            } else {
                failure(
                    "assistance_unavailable",
                    &format!("{service} is unavailable or timed out"),
                )
            }
        })?;
        if !response.status().is_success() {
            return Err(failure(
                "assistance_unavailable",
                &format!("{service} returned HTTP {}", response.status().as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            if e.is_timeout() && generation {
                failure(
                    "assistance_timeout",
                    &format!("{service} did not answer within 120 seconds"),
                )
            } else {
                failure(
                    "assistance_unavailable",
                    &format!("{service} response was interrupted"),
                )
            }
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
/// Lists an Ollama server's version and installed models (at most 512).
/// Cloud models and models without local weights are marked ineligible.
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
/// Confirm the selected model is installed, local and unchanged. The engine
/// runs this when the model is first used and after model errors, not per email.
pub async fn qualify(settings: &Settings, cancel: &CancellationToken) -> Result<()> {
    let model = settings.model.as_ref().ok_or_else(|| {
        failure(
            "assistance_unavailable",
            "Choose a model in Settings → Local LLM",
        )
    })?;
    if !settings.provider.is_ollama() {
        return crate::openai_local::qualify(&settings.endpoint, &model.name, cancel).await;
    }
    let list = discover(&settings.endpoint, cancel).await?;
    let entry = list["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == model.name)
        .ok_or_else(|| {
            failure(
                "model_unavailable",
                "The selected model is no longer installed",
            )
        })?;
    if entry["digest"] != model.digest {
        return Err(failure(
            "model_changed",
            "The model's weights changed; choose the installed model again",
        ));
    }
    if entry["eligible"] != true {
        return Err(failure(
            "model_unavailable",
            "The selected model is not eligible for local mail processing",
        ));
    }
    let detail = request(
        &settings.endpoint,
        "show",
        Some(json!({"model":model.name})),
        cancel,
    )
    .await?;
    if detail.get("remote_host").is_some_and(|v| !v.is_null())
        || detail.get("remote_model").is_some_and(|v| !v.is_null())
        || !detail["capabilities"]
            .as_array()
            .is_some_and(|caps| caps.iter().any(|c| c == "completion"))
        || !["gguf", "mlx", "safetensors"]
            .contains(&detail["details"]["format"].as_str().unwrap_or_default())
    {
        return Err(failure(
            "model_unavailable",
            "Model metadata does not establish installed local completion weights",
        ));
    }
    Ok(())
}
/// One request for one email. Returns the stored analysis
/// `{summary, sender_kind, category_ids}` and records what the provider
/// reported (`first_token_ms`, `output_tokens`, `generation_ms`, `raw`) in
/// `measure`. Only the synthetic Test shows `raw`; mail text is never logged.
pub async fn infer(
    settings: &Settings,
    email: &Value,
    labels: &Value,
    cancel: &CancellationToken,
    measure: &std::sync::Mutex<Value>,
) -> Result<Value> {
    let model = settings.model.as_ref().ok_or_else(|| {
        failure(
            "assistance_unavailable",
            "Choose a model in Settings → Local LLM",
        )
    })?;
    if !settings.provider.is_ollama() {
        return crate::openai_local::infer(settings, &model.name, email, labels, cancel, measure)
            .await;
    }
    let body = ollama::ollama_request(settings, &model.name, email, labels);
    let result = request(&settings.endpoint, "chat", Some(body), cancel).await?;
    *measure.lock().unwrap() = measurement(&result);
    let content = response_content(&result, &model.name)?;
    parse(content, labels)
}
/// One request summarizing a whole conversation with the built-in thread
/// prompts. `thread` is the prepared `{subject, messages, omitted}`. Returns
/// `{summary}`; `measure` is filled as by [`infer`].
pub async fn infer_thread(
    settings: &Settings,
    thread: &Value,
    cancel: &CancellationToken,
    measure: &std::sync::Mutex<Value>,
) -> Result<Value> {
    let model = settings.model.as_ref().ok_or_else(|| {
        failure(
            "assistance_unavailable",
            "Choose a model in Settings → Local LLM",
        )
    })?;
    if !settings.provider.is_ollama() {
        let body = ollama::thread_openai_request(&model.name, thread);
        let content =
            crate::openai_local::complete(settings, &model.name, body, cancel, measure).await?;
        return parse_thread(&content);
    }
    let body = ollama::thread_ollama_request(&model.name, thread);
    let result = request(&settings.endpoint, "chat", Some(body), cancel).await?;
    *measure.lock().unwrap() = measurement(&result);
    parse_thread(response_content(&result, &model.name)?)
}
/// Parses the model's conversation summary reply into `{summary}`.
pub(crate) fn parse_thread(content: &str) -> Result<Value> {
    ollama::parse_thread_reply(content)
        .map_err(|message| failure("invalid_assistance_result", &message))
}
/// Parses the model's JSON reply into the stored analysis. A label outside
/// `labels` makes the reply invalid.
pub(crate) fn parse(content: &str, labels: &Value) -> Result<Value> {
    ollama::parse_reply(content, &ollama::label_ids(labels))
        .map_err(|message| failure("invalid_assistance_result", &message))
}
/// Ollama reports nanosecond durations; the first token follows model loading
/// and prompt evaluation.
fn measurement(result: &Value) -> Value {
    let ms = |key: &str| result[key].as_u64().map(|ns| ns / 1_000_000);
    json!({
        "first_token_ms": ms("load_duration").zip(ms("prompt_eval_duration")).map(|(a, b)| a + b),
        "output_tokens": result["eval_count"],
        "generation_ms": ms("eval_duration"),
        "raw": result["message"]["content"],
    })
}
/// Returns the reply text, refusing a reply from another model, one that
/// asks for a tool, or one that was cut off or incomplete.
fn response_content<'a>(result: &'a Value, model: &str) -> Result<&'a str> {
    if result["model"] != model {
        return Err(failure(
            "model_changed",
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
            "The model reached its response limit before finishing",
        ));
    }
    if result["done"] != true {
        return Err(failure(
            "invalid_assistance_result",
            "Ollama stopped before completing the response",
        ));
    }
    result["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| failure("invalid_assistance_result", "The model returned no text"))
}

#[cfg(test)]
mod measurement_tests {
    use super::*;
    #[test]
    fn sample_timing_comes_from_the_reported_durations() {
        let reply = json!({"load_duration":1_200_000_000u64,"prompt_eval_duration":300_000_000u64,
            "eval_count":84,"eval_duration":2_000_000_000u64,"message":{"content":"not json"}});
        assert_eq!(
            measurement(&reply),
            json!({"first_token_ms":1500,"output_tokens":84,"generation_ms":2000,"raw":"not json"})
        );
        let bare = measurement(&json!({"message":{"content":"{}"}}));
        assert!(bare["first_token_ms"].is_null() && bare["output_tokens"].is_null());
    }
}

#[cfg(test)]
mod reply_tests {
    use super::*;
    #[test]
    fn only_a_complete_reply_from_the_selected_model_is_read() {
        let content = r#"{"summary":"You paid $49.","label":null,"sender":"bot"}"#;
        let ok = json!({"model":"m","done":true,"done_reason":"stop","message":{"content":content,"tool_calls":null}});
        assert_eq!(response_content(&ok, "m").unwrap(), content);
        assert_eq!(
            parse(content, &json!([])).unwrap()["sender_kind"],
            "automated"
        );
        let mut other = ok.clone();
        other["model"] = "x".into();
        assert_eq!(
            response_content(&other, "m").unwrap_err().code,
            "model_changed"
        );
        let mut long = ok.clone();
        long["done_reason"] = "length".into();
        assert_eq!(
            response_content(&long, "m").unwrap_err().code,
            "assistance_output_limit"
        );
        let mut tool = ok.clone();
        tool["message"]["tool_calls"] = json!([{"function":{}}]);
        assert!(response_content(&tool, "m").is_err());
        assert_eq!(
            parse(
                r#"{"summary":"x","label":"invented","sender":"bot"}"#,
                &json!([])
            )
            .unwrap_err()
            .code,
            "invalid_assistance_result"
        );
    }
}
