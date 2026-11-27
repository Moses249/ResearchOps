use anyhow::{bail, Context, Result};
use reqwest::{redirect::Policy, Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionConfig {
    pub base_url: String,
    pub researcher_model: String,
    pub verifier_model: String,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Clone)]
pub struct Provider {
    pub generation: String,
    pub tested: bool,
    config: ConnectionConfig,
    client: Client,
}

#[derive(Serialize)]
pub struct ConnectionView {
    pub configured: bool,
    pub tested: bool,
    pub base_url: String,
    pub researcher_model: String,
    pub verifier_model: String,
    pub has_key: bool,
}

impl Provider {
    pub fn new(mut config: ConnectionConfig) -> Result<Self> {
        config.base_url = config.base_url.trim().trim_end_matches('/').to_string();
        let url = Url::parse(&config.base_url).context("invalid API base URL")?;
        let local = url
            .host_str()
            .is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "[::1]" || h == "::1");
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            bail!("use HTTPS, or HTTP on localhost for a local model");
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || config.base_url.len() > 2000
        {
            bail!("API base URL must not include credentials, query parameters or a fragment");
        }
        for model in [&mut config.researcher_model, &mut config.verifier_model] {
            *model = model.trim().to_string();
            if model.is_empty() || model.len() > 200 {
                bail!("both agent model names are required (max 200 bytes)");
            }
        }
        config.api_key = config.api_key.trim().to_string();
        if config.api_key.len() > 4096 || config.api_key.contains(['\r', '\n']) {
            bail!("invalid API key");
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .build()?;
        Ok(Self {
            generation: Uuid::new_v4().to_string(),
            tested: false,
            config,
            client,
        })
    }

    pub fn view(&self) -> ConnectionView {
        ConnectionView {
            configured: true,
            tested: self.tested,
            base_url: self.config.base_url.clone(),
            researcher_model: self.config.researcher_model.clone(),
            verifier_model: self.config.verifier_model.clone(),
            has_key: !self.config.api_key.is_empty(),
        }
    }

    pub fn redact(&self, text: &str) -> String {
        if self.config.api_key.is_empty() {
            text.into()
        } else {
            text.replace(&self.config.api_key, "[REDACTED]")
        }
    }

    pub async fn ask(&self, verifier: bool, system: &str, input: &Value) -> Result<Value> {
        self.ask_with_timeout(verifier, system, input, Duration::from_secs(30))
            .await
    }

    pub async fn ask_with_timeout(
        &self,
        verifier: bool,
        system: &str,
        input: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        let model = if verifier {
            &self.config.verifier_model
        } else {
            &self.config.researcher_model
        };
        let mut request=self.client.post(format!("{}/chat/completions",self.config.base_url)).timeout(timeout.min(Duration::from_secs(30))).json(&json!({
            "model":model,
            "messages":[{"role":"system","content":system},{"role":"user","content":input.to_string()}],
            "response_format":{"type":"json_object"}
        }));
        if !self.config.api_key.is_empty() {
            request = request.bearer_auth(&self.config.api_key);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("provider request failed or timed out"))?;
        if !response.status().is_success() {
            bail!("provider returned HTTP {}", response.status().as_u16());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("provider response read failed"))?
        {
            if bytes.len() + chunk.len() > 1_000_000 {
                bail!("provider response exceeded 1 MB");
            }
            bytes.extend_from_slice(&chunk);
        }
        let envelope: Value = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("provider returned invalid JSON"))?;
        let choice = &envelope["choices"][0];
        if choice["finish_reason"]
            .as_str()
            .is_some_and(|s| s != "stop")
        {
            bail!("provider response incomplete or refused");
        }
        let content = choice["message"]["content"]
            .as_str()
            .context("provider response has no text content")?;
        serde_json::from_str(&self.redact(content))
            .map_err(|_| anyhow::anyhow!("agent response must be a JSON object"))
    }

    pub async fn test(&self) -> Result<()> {
        for verifier in [false, true] {
            let result = self
                .ask(
                    verifier,
                    "Connection test. Return only the JSON object {\"ok\":true}.",
                    &json!({"test":true}),
                )
                .await?;
            if result.get("ok") != Some(&Value::Bool(true)) {
                bail!("model did not return the expected connection test object");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(base: &str) -> ConnectionConfig {
        ConnectionConfig {
            base_url: base.into(),
            researcher_model: "research".into(),
            verifier_model: "verify".into(),
            api_key: "secret-token".into(),
        }
    }
    #[test]
    fn rejects_unsafe_or_credential_bearing_endpoints() {
        for url in [
            "http://example.com/v1",
            "https://user:password@example.com/v1",
            "https://example.com/v1?key=secret",
            "file:///tmp/x",
        ] {
            assert!(Provider::new(config(url)).is_err());
        }
        assert!(Provider::new(config("http://127.0.0.1:8080/v1")).is_ok());
    }
    #[test]
    fn connection_view_never_serializes_the_key() {
        let provider = Provider::new(config("https://example.com/v1")).unwrap();
        assert!(!serde_json::to_string(&provider.view())
            .unwrap()
            .contains("secret-token"));
        assert_eq!(provider.redact("error secret-token"), "error [REDACTED]");
    }
}
