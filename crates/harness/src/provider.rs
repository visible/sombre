use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use ureq::Agent;

use crate::config::Endpoint;

#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub message: Value,
    pub prompt: Option<u64>,
    pub cost: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub status: Option<u16>,
    pub message: String,
    pub wait: Option<u64>,
}

impl Failure {
    fn denied(message: String) -> Self {
        Self {
            status: Some(401),
            message,
            wait: None,
        }
    }

    fn plain(message: String) -> Self {
        Self {
            message,
            status: None,
            wait: None,
        }
    }

    pub fn retry(&self) -> bool {
        self.status
            .is_none_or(|status| status == 429 || status >= 500)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    pub name: String,
}

fn client(seconds: u64) -> Agent {
    Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(seconds)))
        .build()
        .into()
}

fn key(endpoint: &Endpoint) -> Result<Option<&str>, String> {
    match endpoint.key.as_deref() {
        None if endpoint.name == crate::config::GATEWAY => Err(
            "no ai gateway key. set AI_GATEWAY_API_KEY or run /provider to save one".to_string(),
        ),
        key => Ok(key),
    }
}

fn sign<B>(request: ureq::RequestBuilder<B>, key: Option<&str>) -> ureq::RequestBuilder<B> {
    match key {
        Some(key) => request.header("Authorization", format!("Bearer {key}")),
        None => request,
    }
}

pub fn chat(
    endpoint: &Endpoint,
    model: &str,
    messages: &[Value],
    tools: &Value,
) -> Result<Reply, String> {
    let key = key(endpoint)?;
    let body = body(
        endpoint,
        &Ask {
            images: false,
            effort: None,
            messages,
            model,
            tools,
        },
    );
    let mut response = sign(
        client(600).post(format!("{}/chat/completions", endpoint.url)),
        key,
    )
    .send_json(&body)
    .map_err(|error| unreachable(&endpoint.name, &error))?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().map_err(|error| {
        format!(
            "{} sent a reply sombre couldn't read: {error}",
            endpoint.name
        )
    })?;
    let reply: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status >= 400 {
        return Err(fault(endpoint, status, &reply, &text));
    }
    parse(&reply).ok_or_else(|| format!("{} sent a reply with no message", endpoint.name))
}

#[derive(Debug, Default)]
pub struct Collect {
    content: String,
    calls: Vec<(String, String, String)>,
    prompt: Option<u64>,
    cost: Option<f64>,
}

impl Collect {
    pub fn feed(&mut self, chunk: &Value) -> Option<String> {
        if chunk["usage"].is_object() {
            self.prompt = chunk["usage"]["prompt_tokens"].as_u64().or(self.prompt);
            self.cost = chunk["usage"]["cost"].as_f64().or(self.cost);
        }
        let delta = &chunk["choices"][0]["delta"];
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(self.calls.len());
            while self.calls.len() <= index {
                self.calls
                    .push((String::new(), String::new(), String::new()));
            }
            let slot = &mut self.calls[index];
            if let Some(id) = call["id"].as_str() {
                slot.0.push_str(id);
            }
            if let Some(name) = call["function"]["name"].as_str() {
                slot.1.push_str(name);
            }
            if let Some(arguments) = call["function"]["arguments"].as_str() {
                slot.2.push_str(arguments);
            }
        }
        let text = delta["content"].as_str().filter(|text| !text.is_empty())?;
        self.content.push_str(text);
        Some(text.to_string())
    }

    pub fn finish(self) -> Reply {
        let content = if self.content.is_empty() {
            Value::Null
        } else {
            Value::String(self.content)
        };
        let mut message = json!({"role": "assistant", "content": content});
        let calls: Vec<Value> = self
            .calls
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, arguments)| {
                json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": if arguments.is_empty() { "{}".to_string() } else { arguments },
                    }
                })
            })
            .collect();
        if !calls.is_empty() {
            message["tool_calls"] = Value::Array(calls);
        }
        Reply {
            cost: self.cost,
            message,
            prompt: self.prompt,
        }
    }
}

pub struct Ask<'a> {
    pub model: &'a str,
    pub messages: &'a [Value],
    pub tools: &'a Value,
    pub effort: Option<&'a str>,
    pub images: bool,
}

fn pictures(message: &Value) -> Vec<String> {
    message["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|path| crate::image::url(std::path::Path::new(path)).ok())
        .collect()
}

pub fn stream(
    endpoint: &Endpoint,
    ask: &Ask,
    cancel: &AtomicBool,
    on: impl FnMut(&str),
) -> Result<Reply, Failure> {
    if endpoint.responses {
        respond(endpoint, ask, cancel, on)
    } else {
        complete(endpoint, ask, cancel, on)
    }
}

pub fn complete(
    endpoint: &Endpoint,
    ask: &Ask,
    cancel: &AtomicBool,
    mut on: impl FnMut(&str),
) -> Result<Reply, Failure> {
    let mut body = body(endpoint, ask);
    body["stream"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});
    let mut collect = Collect::default();
    events(
        endpoint,
        "chat/completions",
        &body,
        cancel,
        |chunk, data| {
            if chunk["error"].is_object() {
                return Err(Failure {
                    message: problem(&endpoint.name, 200, chunk, data),
                    status: Some(200),
                    wait: None,
                });
            }
            if let Some(text) = collect.feed(chunk) {
                on(&text);
            }
            Ok(())
        },
    )?;
    Ok(collect.finish())
}

fn respond(
    endpoint: &Endpoint,
    ask: &Ask,
    cancel: &AtomicBool,
    mut on: impl FnMut(&str),
) -> Result<Reply, Failure> {
    let mut gather = Gather::default();
    events(
        endpoint,
        "responses",
        &form(endpoint, ask),
        cancel,
        |event, _| {
            if let Some(text) = gather.feed(event) {
                on(&text);
            }
            match gather.failed.take() {
                Some(message) => Err(Failure {
                    message: format!("{}: {message}", endpoint.name),
                    status: Some(200),
                    wait: None,
                }),
                None => Ok(()),
            }
        },
    )?;
    Ok(gather.finish(ask.model))
}

fn events(
    endpoint: &Endpoint,
    path: &str,
    body: &Value,
    cancel: &AtomicBool,
    mut each: impl FnMut(&Value, &str) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let key = key(endpoint).map_err(Failure::denied)?;
    let mut response = sign(client(600).post(format!("{}/{path}", endpoint.url)), key)
        .send_json(body)
        .map_err(|error| Failure::plain(unreachable(&endpoint.name, &error)))?;
    let status = response.status().as_u16();
    if status >= 400 {
        let wait = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let reply: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        return Err(Failure {
            message: fault(endpoint, status, &reply, &text),
            status: Some(status),
            wait,
        });
    }
    let reader = BufReader::new(response.into_body().into_reader());
    for line in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let line = line.map_err(|error| {
            Failure::plain(format!("{} stopped mid reply: {error}", endpoint.name))
        })?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        each(&event, data)?;
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct Gather {
    content: String,
    calls: Vec<Value>,
    reasoning: Vec<Value>,
    prompt: Option<u64>,
    cost: Option<f64>,
    failed: Option<String>,
}

fn reason(error: &Value) -> String {
    error["message"]
        .as_str()
        .or_else(|| error["error"]["message"].as_str())
        .unwrap_or("the reply failed")
        .to_string()
}

impl Gather {
    pub fn feed(&mut self, event: &Value) -> Option<String> {
        match event["type"].as_str()? {
            "response.output_text.delta" => {
                let text = event["delta"].as_str().filter(|text| !text.is_empty())?;
                self.content.push_str(text);
                return Some(text.to_string());
            }
            "response.output_item.done" => {
                let item = &event["item"];
                match item["type"].as_str() {
                    Some("reasoning") => self.reasoning.push(item.clone()),
                    Some("function_call") => self.calls.push(json!({
                        "id": item["call_id"],
                        "type": "function",
                        "function": {
                            "name": item["name"],
                            "arguments": item["arguments"].as_str().filter(|arguments| !arguments.is_empty()).unwrap_or("{}"),
                        }
                    })),
                    _ => {}
                }
            }
            "response.completed" | "response.incomplete" => {
                let response = &event["response"];
                self.prompt = response["usage"]["input_tokens"].as_u64().or(self.prompt);
                let cost = &response["provider_metadata"]["gateway"]["cost"];
                self.cost = cost
                    .as_str()
                    .and_then(|cost| cost.parse::<f64>().ok())
                    .or_else(|| cost.as_f64())
                    .or_else(|| response["usage"]["cost"].as_f64())
                    .or(self.cost);
            }
            "response.failed" => self.failed = Some(reason(&event["response"]["error"])),
            "error" => self.failed = Some(reason(event)),
            _ => {}
        }
        None
    }

    pub fn finish(self, model: &str) -> Reply {
        let content = if self.content.is_empty() {
            Value::Null
        } else {
            Value::String(self.content)
        };
        let mut message = json!({"role": "assistant", "content": content});
        if !self.calls.is_empty() {
            message["tool_calls"] = Value::Array(self.calls);
        }
        if !self.reasoning.is_empty() {
            message["reasoning"] = json!({"model": model, "items": self.reasoning});
        }
        Reply {
            cost: self.cost,
            message,
            prompt: self.prompt,
        }
    }
}

pub fn items(messages: &[Value], model: &str, images: bool) -> (String, Vec<Value>) {
    let mut instructions: Vec<&str> = Vec::new();
    let mut input = Vec::new();
    for message in messages {
        match message["role"].as_str() {
            Some("system") => instructions.extend(message["content"].as_str()),
            Some("user") => {
                let urls = if images {
                    pictures(message)
                } else {
                    Vec::new()
                };
                let content = if urls.is_empty() {
                    message["content"].clone()
                } else {
                    let mut parts = vec![json!({"type": "input_text", "text": message["content"]})];
                    parts.extend(
                        urls.into_iter()
                            .map(|url| json!({"type": "input_image", "image_url": url})),
                    );
                    Value::Array(parts)
                };
                input.push(json!({"role": "user", "content": content}));
            }
            Some("assistant") => {
                let reasoning = &message["reasoning"];
                if reasoning["model"] == model {
                    input.extend(reasoning["items"].as_array().into_iter().flatten().cloned());
                }
                if let Some(text) = message["content"].as_str().filter(|text| !text.is_empty()) {
                    input.push(json!({"role": "assistant", "content": text}));
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call["id"],
                        "name": call["function"]["name"],
                        "arguments": call["function"]["arguments"],
                    }));
                }
            }
            Some("tool") => input.push(json!({
                "type": "function_call_output",
                "call_id": message["tool_call_id"],
                "output": message["content"].as_str().unwrap_or_default(),
            })),
            _ => {}
        }
    }
    (instructions.join("\n\n"), input)
}

pub fn form(endpoint: &Endpoint, ask: &Ask) -> Value {
    let (instructions, input) = items(ask.messages, ask.model, ask.images);
    let mut body = json!({
        "model": ask.model,
        "input": input,
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    if endpoint.name == crate::config::GATEWAY {
        body["caching"] = json!("auto");
    }
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions);
    }
    let tools: Vec<Value> = ask
        .tools
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| {
            let function = &tool["function"];
            json!({
                "type": "function",
                "name": function["name"],
                "description": function["description"],
                "parameters": function["parameters"],
            })
        })
        .collect();
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if let Some(effort) = ask.effort {
        body["reasoning"] = json!({"effort": effort});
    }
    body
}

pub fn body(endpoint: &Endpoint, ask: &Ask) -> Value {
    let messages: Vec<Value> = ask
        .messages
        .iter()
        .map(|message| {
            let mut message = message.clone();
            let urls = if ask.images {
                pictures(&message)
            } else {
                Vec::new()
            };
            if let Some(fields) = message.as_object_mut() {
                fields.remove("reasoning");
                fields.remove("images");
            }
            if !urls.is_empty() {
                let mut parts = vec![json!({"type": "text", "text": message["content"]})];
                parts.extend(
                    urls.into_iter()
                        .map(|url| json!({"type": "image_url", "image_url": {"url": url}})),
                );
                message["content"] = Value::Array(parts);
            }
            message
        })
        .collect();
    let mut body = json!({"model": ask.model, "messages": messages});
    if ask.tools.as_array().is_some_and(|tools| !tools.is_empty()) {
        body["tools"] = ask.tools.clone();
    }
    if let Some(effort) = ask.effort {
        body["reasoning_effort"] = json!(effort);
    }
    if endpoint.name == crate::config::GATEWAY {
        body["providerOptions"] = json!({"gateway": {"caching": "auto"}});
    }
    body
}

pub fn parse(reply: &Value) -> Option<Reply> {
    let raw = &reply["choices"][0]["message"];
    if !raw.is_object() {
        return None;
    }
    let mut message = json!({"role": "assistant", "content": raw["content"].clone()});
    if let Some(calls) = raw["tool_calls"]
        .as_array()
        .filter(|calls| !calls.is_empty())
    {
        message["tool_calls"] = Value::Array(
            calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call["id"],
                        "type": "function",
                        "function": {
                            "name": call["function"]["name"],
                            "arguments": call["function"]["arguments"].as_str().unwrap_or("{}"),
                        }
                    })
                })
                .collect(),
        );
    }
    Some(Reply {
        cost: reply["usage"]["cost"].as_f64(),
        message,
        prompt: reply["usage"]["prompt_tokens"].as_u64(),
    })
}

fn fault(endpoint: &Endpoint, status: u16, reply: &Value, text: &str) -> String {
    if endpoint.key.is_none() && matches!(status, 401 | 403) {
        return format!(
            "{} needs an api key. pick it in /provider to add one",
            endpoint.name
        );
    }
    problem(&endpoint.name, status, reply, text)
}

pub fn unreachable(name: &str, error: &ureq::Error) -> String {
    match error {
        ureq::Error::Io(problem) if problem.kind() == std::io::ErrorKind::ConnectionRefused => {
            format!("{name} isn't answering. check that it's running")
        }
        ureq::Error::Timeout(_) => format!("{name} took too long to answer"),
        ureq::Error::HostNotFound => {
            format!("couldn't find {name}. check your connection and its url")
        }
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed => {
            format!("couldn't reach {name}. check your connection")
        }
        other => format!("couldn't reach {name}: {other}"),
    }
}

pub fn problem(name: &str, status: u16, reply: &Value, text: &str) -> String {
    let detail = reply["error"]["message"]
        .as_str()
        .or_else(|| reply["message"].as_str())
        .map_or_else(|| text.trim().chars().take(200).collect(), str::to_string);
    match status {
        401 | 403 if detail.is_empty() => {
            format!("{name} rejected the key. run /provider to update it")
        }
        _ if detail.is_empty() => format!("{name} answered with an error ({status})"),
        _ => format!("{name}: {detail}"),
    }
}

pub fn models(endpoint: &Endpoint) -> Result<Vec<Listed>, String> {
    let key = key(endpoint)?;
    let mut response = sign(client(20).get(format!("{}/models", endpoint.url)), key)
        .call()
        .map_err(|error| unreachable(&endpoint.name, &error))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .with_config()
        .limit(20 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| format!("couldn't read {}'s models: {error}", endpoint.name))?;
    let reply: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status >= 400 {
        return Err(fault(endpoint, status, &reply, &text));
    }
    Ok(listed(&reply))
}

pub fn listed(reply: &Value) -> Vec<Listed> {
    reply["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["type"].as_str().is_none_or(|kind| kind == "language"))
        .filter(|entry| {
            entry["tags"]
                .as_array()
                .is_none_or(|tags| tags.iter().any(|tag| tag == "tool-use"))
        })
        .filter_map(|entry| {
            let id = entry["id"].as_str()?;
            if id.ends_with("-batch") {
                return None;
            }
            Some(Listed {
                id: id.to_string(),
                name: entry["name"].as_str().unwrap_or(id).to_string(),
            })
        })
        .collect()
}

pub fn window(endpoint: &Endpoint, model: &str) -> Option<u64> {
    let key = key(endpoint).ok()?;
    let mut response = sign(
        client(10).get(format!("{}/models/{model}", endpoint.url)),
        key,
    )
    .call()
    .ok()?;
    if response.status().as_u16() >= 400 {
        return None;
    }
    let reply: Value = response.body_mut().read_json().ok()?;
    reply["context_window"]
        .as_u64()
        .or_else(|| reply["data"]["context_window"].as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALL: [&str; 4] = [
        r#"{"type":"response.output_item.done","sequence_number":9,"output_index":0,"item":{"type":"reasoning","id":"rs_1794369615825_3guwzzq8azt","summary":[],"content":[{"type":"reasoning_text","text":" need to run ls."}]}}"#,
        r#"{"type":"response.function_call_arguments.delta","sequence_number":17,"item_id":"fc_1794369615896_soo30908m9s","output_index":1,"delta":"ls"}"#,
        r#"{"type":"response.output_item.done","sequence_number":23,"output_index":1,"item":{"type":"function_call","id":"fc_1794369615896_soo30908m9s","call_id":"chatcmpl-tool-cb1439233af2469bb9c0cca5f5e4527d","name":"bash","arguments":"{\n  \"command\": \"ls -la\"\n}","status":"completed"}}"#,
        r#"{"type":"response.completed","sequence_number":24,"response":{"id":"gen_01M74C6XKH60H4H08W1FJXYMG2","status":"completed","usage":{"input_tokens":137,"output_tokens":33,"total_tokens":170,"input_tokens_details":{"cached_tokens":64},"output_tokens_details":{"reasoning_tokens":0}},"provider_metadata":{"gateway":{"cost":"0.0000302","marketCost":"0.0000302","gatewayCost":"0.0001302"}}}}"#,
    ];
    const TEXT: [&str; 4] = [
        r#"{"type":"response.output_item.added","sequence_number":45,"output_index":1,"item":{"type":"message","id":"msg_1794369648158_dlw2xukpa44","status":"in_progress","role":"assistant","content":[{"type":"output_text","text":"","annotations":[],"logprobs":[]}]}}"#,
        r#"{"type":"response.output_text.delta","sequence_number":47,"item_id":"msg_1794369648158_dlw2xukpa44","output_index":1,"content_index":0,"delta":"a.txt","logprobs":[]}"#,
        r#"{"type":"response.output_text.delta","sequence_number":49,"item_id":"msg_1794369648158_dlw2xukpa44","output_index":1,"content_index":0,"delta":"\nb.txt","logprobs":[]}"#,
        r#"{"type":"response.output_item.done","sequence_number":54,"output_index":1,"item":{"type":"message","id":"msg_1794369648158_dlw2xukpa44","status":"completed","role":"assistant","content":[{"type":"output_text","text":"a.txt\nb.txt","annotations":[],"logprobs":[]}]}}"#,
    ];

    fn gather(lines: &[&str]) -> (Gather, Vec<String>) {
        let mut gather = Gather::default();
        let streamed = lines
            .iter()
            .filter_map(|line| {
                gather.feed(&serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}")))
            })
            .collect();
        (gather, streamed)
    }

    #[test]
    fn reads_a_responses_tool_call_into_a_chat_message() {
        let (gather, streamed) = gather(&CALL);
        assert!(streamed.is_empty());
        let reply = gather.finish("openai/gpt-oss-120b");
        assert_eq!(reply.prompt, Some(137));
        assert_eq!(reply.cost, Some(0.000_030_2));
        let call = &reply.message["tool_calls"][0];
        assert_eq!(call["id"], "chatcmpl-tool-cb1439233af2469bb9c0cca5f5e4527d");
        assert_eq!(call["function"]["name"], "bash");
        assert_eq!(
            call["function"]["arguments"],
            "{\n  \"command\": \"ls -la\"\n}"
        );
        assert_eq!(reply.message["reasoning"]["model"], "openai/gpt-oss-120b");
        assert_eq!(
            reply.message["reasoning"]["items"][0]["id"],
            "rs_1794369615825_3guwzzq8azt"
        );
        assert!(reply.message["content"].is_null());
    }

    #[test]
    fn streams_responses_text() {
        let (gather, streamed) = gather(&TEXT);
        assert_eq!(streamed, ["a.txt", "\nb.txt"]);
        let reply = gather.finish("m");
        assert_eq!(reply.message["content"], "a.txt\nb.txt");
        assert!(reply.message.get("tool_calls").is_none());
        assert!(reply.message.get("reasoning").is_none());
    }

    #[test]
    fn reports_failed_responses() {
        let (mut failed, _) = gather(&[
            r#"{"type":"response.failed","response":{"status":"failed","error":{"code":"server_error","message":"the provider went away"}}}"#,
        ]);
        assert_eq!(
            failed.failed.take().as_deref(),
            Some("the provider went away")
        );
        let (mut error, _) =
            gather(&[r#"{"type":"error","code":"rate_limit","message":"slow down"}"#]);
        assert_eq!(error.failed.take().as_deref(), Some("slow down"));
    }

    #[test]
    fn turns_a_chat_history_into_responses_items() {
        let (reply, _) = gather(&CALL);
        let assistant = reply.finish("openai/gpt-oss-120b").message;
        let messages = [
            json!({"role": "system", "content": "You are terse."}),
            json!({"role": "user", "content": "List the files here using the bash tool."}),
            assistant,
            json!({"role": "tool", "tool_call_id": "chatcmpl-tool-cb1439233af2469bb9c0cca5f5e4527d", "content": "a.txt\nb.txt"}),
            json!({"role": "assistant", "content": "a.txt and b.txt"}),
        ];
        let (instructions, input) = items(&messages, "openai/gpt-oss-120b", true);
        assert_eq!(instructions, "You are terse.");
        let kinds: Vec<&str> = input
            .iter()
            .map(|item| {
                item["type"]
                    .as_str()
                    .or_else(|| item["role"].as_str())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "user",
                "reasoning",
                "function_call",
                "function_call_output",
                "assistant"
            ]
        );
        assert_eq!(input[2]["call_id"], input[3]["call_id"]);
        assert_eq!(input[3]["output"], "a.txt\nb.txt");
        let (_, other) = items(&messages, "anthropic/claude-sonnet-5.5", true);
        assert!(other.iter().all(|item| item["type"] != "reasoning"));
    }

    #[test]
    fn builds_a_responses_request() {
        let tools = json!([{"type": "function", "function": {"name": "bash", "description": "Run a shell command", "parameters": {"type": "object"}}}]);
        let messages = [json!({"role": "user", "content": "hi"})];
        let ask = Ask {
            images: false,
            effort: Some("high"),
            messages: &messages,
            model: "openai/gpt-oss-120b",
            tools: &tools,
        };
        let gateway = Endpoint {
            key: None,
            name: "gateway".to_string(),
            responses: true,
            source: None,
            url: String::new(),
        };
        let request = form(&gateway, &ask);
        assert_eq!(
            request["tools"][0],
            json!({"type": "function", "name": "bash", "description": "Run a shell command", "parameters": {"type": "object"}})
        );
        assert_eq!(request["reasoning"]["effort"], "high");
        assert_eq!(request["caching"], "auto");
        assert_eq!(request["store"], false);
        assert_eq!(request["include"][0], "reasoning.encrypted_content");
        assert!(request.get("instructions").is_none());
        let other = form(
            &Endpoint {
                name: "abliteration".to_string(),
                ..gateway.clone()
            },
            &ask,
        );
        assert!(other.get("caching").is_none());
        assert_eq!(other["reasoning"]["effort"], "high");
        let chat = body(
            &Endpoint {
                key: None,
                name: "local".to_string(),
                responses: false,
                source: None,
                url: String::new(),
            },
            &Ask {
                messages: &[
                    json!({"role": "assistant", "content": "x", "reasoning": {"model": "m", "items": []}}),
                ],
                ..ask
            },
        );
        assert!(chat["messages"][0].get("reasoning").is_none());
    }

    #[test]
    fn keeps_only_what_the_next_request_needs() {
        let reply = json!({
            "choices": [{"index": 0, "message": {
                "role": "assistant",
                "content": null,
                "reasoning": "thinking",
                "provider_metadata": {"x": 1},
                "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}]
            }}],
            "usage": {"prompt_tokens": 132, "completion_tokens": 48, "cost": 0.0000372}
        });
        let parsed = parse(&reply).unwrap_or_else(|| panic!("no reply"));
        assert_eq!(parsed.prompt, Some(132));
        assert_eq!(parsed.cost, Some(0.0000372));
        assert_eq!(
            parsed.message,
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}]})
        );
    }

    #[test]
    fn collects_a_streamed_reply_with_tool_calls() {
        let mut collect = Collect::default();
        let chunks = [
            json!({"choices": [{"delta": {"role": "assistant", "content": "Look"}}]}),
            json!({"choices": [{"delta": {"content": "ing."}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "c1", "function": {"name": "bash", "arguments": "{\"comm"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "and\": \"ls\"}"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 1, "id": "c2", "function": {"name": "list", "arguments": ""}}]}}]}),
            json!({"choices": [], "usage": {"prompt_tokens": 90, "cost": 0.001}}),
        ];
        let texts: Vec<String> = chunks
            .iter()
            .filter_map(|chunk| collect.feed(chunk))
            .collect();
        assert_eq!(texts, ["Look", "ing."]);
        let reply = collect.finish();
        assert_eq!(reply.prompt, Some(90));
        assert_eq!(reply.cost, Some(0.001));
        assert_eq!(reply.message["content"], "Looking.");
        assert_eq!(
            reply.message["tool_calls"][0]["function"]["arguments"],
            "{\"command\": \"ls\"}"
        );
        assert_eq!(reply.message["tool_calls"][1]["function"]["name"], "list");
        assert_eq!(
            reply.message["tool_calls"][1]["function"]["arguments"],
            "{}"
        );
    }

    #[test]
    fn collects_a_plain_streamed_reply() {
        let mut collect = Collect::default();
        collect.feed(&json!({"choices": [{"delta": {"content": "hi"}}]}));
        assert_eq!(
            collect.finish().message,
            json!({"role": "assistant", "content": "hi"})
        );
    }

    #[test]
    fn retries_only_busy_or_unreachable_providers() {
        let failure = |status: Option<u16>| Failure {
            message: String::new(),
            status,
            wait: None,
        };
        assert!(failure(Some(429)).retry());
        assert!(failure(Some(503)).retry());
        assert!(failure(None).retry());
        assert!(!failure(Some(401)).retry());
        assert!(!failure(Some(400)).retry());
    }

    #[test]
    fn network_failures_read_as_plain_sentences() {
        let refused = ureq::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert_eq!(
            unreachable("ollama", &refused),
            "ollama isn't answering. check that it's running"
        );
        assert_eq!(
            unreachable("gateway", &ureq::Error::HostNotFound),
            "couldn't find gateway. check your connection and its url"
        );
        assert!(!unreachable("gateway", &refused).contains("os error"));
    }

    #[test]
    fn a_missing_gateway_key_fails_at_once_without_retrying() {
        let gateway = Endpoint {
            key: None,
            name: crate::config::GATEWAY.to_string(),
            responses: false,
            source: None,
            url: "http://127.0.0.1:9".to_string(),
        };
        let tools = json!([]);
        let ask = Ask {
            images: false,
            effort: None,
            messages: &[],
            model: "m",
            tools: &tools,
        };
        let failed = stream(&gateway, &ask, &AtomicBool::new(false), |_| {});
        assert!(failed.as_ref().is_err_and(
            |failure| !failure.retry() && failure.message.starts_with("no ai gateway key")
        ));
    }

    #[test]
    fn turns_on_caching_only_for_the_gateway() {
        let endpoint = |name: &str| Endpoint {
            key: None,
            name: name.to_string(),
            responses: false,
            source: None,
            url: String::new(),
        };
        let tools = json!([]);
        let mut ask = Ask {
            images: false,
            effort: None,
            messages: &[],
            model: "m",
            tools: &tools,
        };
        let gateway = body(&endpoint("gateway"), &ask);
        assert_eq!(gateway["providerOptions"]["gateway"]["caching"], "auto");
        assert!(gateway.get("reasoning_effort").is_none());
        let custom = body(&endpoint("local"), &ask);
        assert!(custom.get("providerOptions").is_none());
        ask.effort = Some("high");
        assert_eq!(body(&endpoint("local"), &ask)["reasoning_effort"], "high");
    }

    #[test]
    fn reads_plain_replies() {
        let reply = json!({"choices": [{"message": {"role": "assistant", "content": "hi"}}]});
        assert_eq!(
            parse(&reply).map(|parsed| parsed.message),
            Some(json!({"role": "assistant", "content": "hi"}))
        );
        assert_eq!(parse(&json!({"choices": []})), None);
    }

    #[test]
    fn explains_errors_from_the_provider() {
        let reply = json!({"error": {"message": "Free tier users do not have access to this model.", "type": "no_providers_available"}});
        assert_eq!(
            problem("gateway", 403, &reply, ""),
            "gateway: Free tier users do not have access to this model."
        );
        assert_eq!(
            problem("local", 401, &Value::Null, ""),
            "local rejected the key. run /provider to update it"
        );
        assert_eq!(
            problem("local", 500, &Value::Null, "upstream broke"),
            "local: upstream broke"
        );
    }

    #[test]
    fn lists_tool_models_and_skips_batch_variants() {
        let reply = json!({"data": [
            {"id": "anthropic/claude-sonnet-5.5", "name": "Claude Sonnet 5.5", "type": "language", "tags": ["tool-use"]},
            {"id": "anthropic/claude-sonnet-5.5-batch", "name": "Batch", "type": "language", "tags": ["tool-use"]},
            {"id": "openai/text-embedding-4", "type": "embedding"},
            {"id": "plain/no-tools", "type": "language", "tags": ["vision"]},
            {"id": "local-model", "object": "model"}
        ]});
        let ids: Vec<String> = listed(&reply).into_iter().map(|model| model.id).collect();
        assert_eq!(ids, ["anthropic/claude-sonnet-5.5", "local-model"]);
    }
}
