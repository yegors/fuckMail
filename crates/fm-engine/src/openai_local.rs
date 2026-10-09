//! Local OpenAI-compatible chat transport. Model IDs are server identities, not weight digests.
use crate::ollama::{failure, parse, request_local};
use fm_assist::ollama::{openai_request, Provider, Settings};
use fm_core::Result;
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};

/// The server root of a loopback endpoint, without any trailing `/v1`.
fn endpoint_base(endpoint: &str) -> Result<&str> {
    Provider::OpenaiCompatible
        .endpoint_base(endpoint)
        .ok_or_else(|| {
            failure(
                "invalid_request",
                "Use a loopback HTTP endpoint with an explicit port, optionally ending /v1",
            )
        })
}
/// Lists the models a local OpenAI-compatible server advertises at
/// `/v1/models` (at most 512, duplicates dropped). There are no weight
/// digests to check, so every listed model is eligible.
pub async fn discover(endpoint: &str, cancel: &CancellationToken) -> Result<Value> {
    let result = request_local(
        endpoint_base(endpoint)?,
        "/v1/models",
        "Local model server",
        false,
        None,
        cancel,
    )
    .await?;
    let models = result["data"]
        .as_array()
        .filter(|models| models.len() <= 512)
        .ok_or_else(|| {
            failure(
                "invalid_assistance_result",
                "The local server did not return a bounded model list",
            )
        })?;
    let mut names = std::collections::BTreeSet::new();
    let mut list = Vec::new();
    for model in models {
        let name = model["id"]
            .as_str()
            .filter(|name| {
                !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
            })
            .ok_or_else(|| {
                failure(
                    "invalid_assistance_result",
                    "The local server returned an invalid model ID",
                )
            })?;
        if names.insert(name) {
            list.push(json!({"name":name,"digest":"","eligible":true,"reason":"Model advertised by this local server"}));
        }
    }
    Ok(json!({"provider":"openai_compatible","models":list,"endpoint":endpoint}))
}
/// Confirms the server still advertises the model `name`; otherwise fails
/// with `model_unavailable`.
pub async fn qualify(endpoint: &str, name: &str, cancel: &CancellationToken) -> Result<()> {
    let result = discover(endpoint, cancel).await?;
    if !result["models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["name"] == name)
    {
        return Err(failure(
            "model_unavailable",
            "The selected model is no longer available from this local server",
        ));
    }
    Ok(())
}
/// Analyses one email with a single `/v1/chat/completions` request, like
/// [`crate::ollama::infer`]. `measure` gets token usage only; this API
/// reports no timings.
pub async fn infer(
    settings: &Settings,
    model: &str,
    email: &Value,
    labels: &Value,
    cancel: &CancellationToken,
    measure: &std::sync::Mutex<Value>,
) -> Result<Value> {
    let body = openai_request(settings, model, email, labels);
    parse(
        &complete(settings, model, body, cancel, measure).await?,
        labels,
    )
}
/// Sends one prepared `/v1/chat/completions` body and returns the reply
/// text, checked as [`infer`] checks it. `measure` gets token usage only.
pub(crate) async fn complete(
    settings: &Settings,
    model: &str,
    body: Value,
    cancel: &CancellationToken,
    measure: &std::sync::Mutex<Value>,
) -> Result<String> {
    let result = request_local(
        endpoint_base(&settings.endpoint)?,
        "/v1/chat/completions",
        "Local model server",
        true,
        Some(body),
        cancel,
    )
    .await?;
    // This non-streaming API reports token usage but no timing.
    *measure.lock().unwrap() = json!({
        "first_token_ms": null,
        "output_tokens": result["usage"]["completion_tokens"],
        "generation_ms": null,
        "raw": result["choices"][0]["message"]["content"],
    });
    response_content(&result, model).map(str::to_owned)
}
/// Returns the text of the single choice, refusing a reply from another
/// model, a tool or function call, or anything but a complete `stop`.
fn response_content<'a>(result: &'a Value, model: &str) -> Result<&'a str> {
    if result["model"] != model {
        return Err(failure(
            "model_changed",
            "The local server responded with a different model",
        ));
    }
    let choices = result["choices"]
        .as_array()
        .filter(|choices| choices.len() == 1)
        .ok_or_else(|| {
            failure(
                "invalid_assistance_result",
                "The local server must return exactly one response",
            )
        })?;
    let choice = &choices[0];
    if choice["message"]
        .get("tool_calls")
        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        || choice["message"]
            .get("function_call")
            .is_some_and(|v| !v.is_null())
    {
        return Err(failure(
            "invalid_assistance_result",
            "The model requested a tool; mail analysis cannot use tools",
        ));
    }
    if choice["finish_reason"] == "length" {
        return Err(failure(
            "assistance_output_limit",
            "The model reached its response limit before finishing. Try again.",
        ));
    }
    if choice["finish_reason"] != "stop" || choice["message"]["role"] != "assistant" {
        return Err(failure(
            "invalid_assistance_result",
            "The local server stopped before completing the response. Try again.",
        ));
    }
    choice["message"]["content"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            failure(
                "invalid_assistance_result",
                "The model returned no analysis text. Try again.",
            )
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use fm_assist::ollama::Model;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    fn output() -> Value {
        json!({"summary":"The design is approved. Reply with your availability for Friday.","label":"work","sender":"human"})
    }
    fn response() -> Value {
        json!({"model":"local-model","usage":{"completion_tokens":42},"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":output().to_string(),"tool_calls":null}}]})
    }
    /// A loopback HTTP server that answers each request in turn with one of
    /// `replies` and returns the request lines and JSON bodies it received.
    pub(crate) async fn server(
        replies: Vec<(u16, String)>,
    ) -> (String, tokio::task::JoinHandle<Vec<(String, Value)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let (socket, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut socket = BufReader::new(socket);
                let mut first = String::new();
                socket.read_line(&mut first).await.unwrap();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    socket.read_line(&mut header).await.unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    assert!(!header.is_empty());
                    if let Some(value) = header.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(length <= 256 * 1024);
                let mut data = vec![0; length];
                socket.read_exact(&mut data).await.unwrap();
                requests.push((
                    first.trim().to_string(),
                    if data.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(&data).unwrap()
                    },
                ));
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (endpoint, handle)
    }
    #[tokio::test]
    async fn compatible_transport_sends_one_request_with_the_prompt_and_schema() {
        let (endpoint, server) = server(vec![(200, response().to_string())]).await;
        let settings = Settings {
            provider: Provider::OpenaiCompatible,
            endpoint: format!("{endpoint}/v1"),
            model: Some(Model {
                name: "local-model".into(),
                digest: String::new(),
            }),
            ..Settings::default()
        };
        settings.validate().unwrap();
        let email = json!({"from":"alex@example.test","subject":"Friday","text":"Literal {{labels}} remains email text.\nPlease reply Friday.","shortened":false});
        let labels = json!([{"id":"work","name":"Work","description":""}]);
        let measure = std::sync::Mutex::new(Value::Null);
        let result = crate::ollama::infer(
            &settings,
            &email,
            &labels,
            &CancellationToken::new(),
            &measure,
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            json!({"summary":output()["summary"],"sender_kind":"human","category_ids":["work"]})
        );
        let measured = measure.lock().unwrap().clone();
        assert_eq!(
            measured["output_tokens"], 42,
            "token usage reaches the Test button"
        );
        assert!(measured["first_token_ms"].is_null() && measured["raw"].is_string());
        let requests = server.await.unwrap();
        assert_eq!(
            requests.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            ["POST /v1/chat/completions HTTP/1.1"],
            "no model checks around each email"
        );
        let body = &requests[0].1;
        assert_eq!(
            body,
            &openai_request(&settings, "local-model", &email, &labels)
        );
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["max_tokens"], fm_assist::ollama::OUTPUT_TOKENS);
        assert!(body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Literal {{labels}} remains email text."));
    }
    #[test]
    fn incomplete_mismatched_tool_and_empty_results_are_rejected() {
        let valid = response();
        assert!(response_content(&valid, "local-model").is_ok());
        for reason in ["length", "tool_calls", "content_filter", "error", ""] {
            let mut bad = valid.clone();
            bad["choices"][0]["finish_reason"] = reason.into();
            let error = response_content(&bad, "local-model").unwrap_err();
            assert_eq!(
                error.code,
                if reason == "length" {
                    "assistance_output_limit"
                } else {
                    "invalid_assistance_result"
                }
            );
        }
        assert!(response_content(&valid, "other-model").is_err());
        for value in [json!([{"id":"tool"}]), json!({}), json!("tool")] {
            let mut bad = valid.clone();
            bad["choices"][0]["message"]["tool_calls"] = value;
            assert!(response_content(&bad, "local-model").is_err());
        }
        let mut bad = valid.clone();
        bad["choices"][0]["message"]["content"] = "  ".into();
        assert!(response_content(&bad, "local-model").is_err());
        let mut bad = valid.clone();
        bad["choices"]
            .as_array_mut()
            .unwrap()
            .push(valid["choices"][0].clone());
        assert!(response_content(&bad, "local-model").is_err());
    }
    #[tokio::test]
    async fn transport_failures_and_cancellation_do_not_fallback() {
        for (status, body, code) in [
            (302, "{}".into(), "assistance_unavailable"),
            (401, "{}".into(), "assistance_unavailable"),
            (200, "x".repeat(256 * 1024 + 1), "assistance_limit"),
            (200, "not JSON".into(), "invalid_assistance_result"),
        ] {
            let (endpoint, server) = server(vec![(status, body)]).await;
            assert_eq!(
                discover(&endpoint, &CancellationToken::new())
                    .await
                    .unwrap_err()
                    .code,
                code
            );
            assert_eq!(server.await.unwrap().len(), 1);
        }
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            discover("http://127.0.0.1:9", &cancel)
                .await
                .unwrap_err()
                .code,
            "assistance_canceled"
        );
    }
}
