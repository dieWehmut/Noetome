#[cfg(test)]
mod tests {
    #[test]
    fn selected_gateway_accepts_only_exact_live_endpoint_and_selected_model() {
        let mut value = config_value();
        value["transport_mode"] = serde_json::json!("live");
        value["endpoint"] = serde_json::json!("https://ai.1343263.xyz/v1/responses");
        value["model"] = serde_json::json!("deepseek-flash");
        let config: super::ApiConfig = serde_json::from_value(value.clone()).unwrap();
        super::validate(&config).unwrap();
        for endpoint in [
            "https://ai.1343263.xyz:443/v1/responses",
            "http://ai.1343263.xyz/v1/responses",
            "https://ai.1343263.xyz/v1/responses/",
            "https://ai.1343263.xyz/v1/responses?x=1",
            "https://ai.1343263.xyz/v1/responses#x",
            "https://user@ai.1343263.xyz/v1/responses",
            "https://AI.1343263.xyz/v1/responses",
            "https://ai.1343263.xyz.other.example/v1/responses",
            "https://ai.1343263.xyz/v1/chat/completions",
        ] {
            let mut invalid = value.clone();
            invalid["endpoint"] = serde_json::json!(endpoint);
            let config = serde_json::from_value(invalid).unwrap();
            assert_eq!(
                super::validate(&config).unwrap_err().code(),
                "EXTRACT_API_ENDPOINT",
                "{endpoint}"
            );
        }
        for model in [
            "deepseek-chat",
            "deepseek-flash ",
            "DeepSeek-Flash",
            "noetome-test-model",
        ] {
            let mut invalid = value.clone();
            invalid["model"] = serde_json::json!(model);
            let config = serde_json::from_value(invalid).unwrap();
            assert_eq!(
                super::validate(&config).unwrap_err().code(),
                "EXTRACT_API_CONFIG",
                "{model}"
            );
        }
    }
    fn config_value() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"provider":"openai_responses","transport_mode":"local_http_test",
            "endpoint":"http://127.0.0.1:1234/v1/responses","model":"noetome-test-model","credential_env":"NOETOME_TEST_CREDENTIAL",
            "required_input":"text","capabilities":{"text":true,"structured_output":true,"vision":false},
            "selection":[{"kind":"text","ordinal":0}],"limits":{"max_units":1,"max_input_bytes":4096,"max_response_bytes":65536,"max_output_tokens":2048,"timeout_ms":3000},
            "budget":{"max_requests":1,"max_total_tokens":65536,"max_cost_usd":null}})
    }
    #[test]
    fn strict_config_rejects_duplicates_extra_missing_and_nullable_omissions() {
        let value = config_value();
        let config: super::ApiConfig = serde_json::from_value(value.clone()).unwrap();
        super::validate(&config).unwrap();
        let text = serde_json::to_string(&value).unwrap();
        assert!(
            serde_json::from_str::<super::ApiConfig>(&text.replace(
                "\"schema_version\":1",
                "\"schema_version\":1,\"schema_version\":1"
            ))
            .is_err()
        );
        assert!(serde_json::from_str::<super::ApiConfig>(&format!("{text} {{}}")).is_err());
        for field in ["endpoint", "capabilities", "budget", "required_input"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<super::ApiConfig>(missing).is_err());
        }
        let mut missing = value.clone();
        missing["budget"]
            .as_object_mut()
            .unwrap()
            .remove("max_cost_usd");
        assert!(serde_json::from_value::<super::ApiConfig>(missing).is_err());
        let mut extra = value.clone();
        extra["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<super::ApiConfig>(extra).is_err());
        for (field, bad) in [
            ("schema_version", serde_json::json!(2)),
            ("provider", serde_json::json!("other")),
            ("transport_mode", serde_json::json!("other")),
            ("required_input", serde_json::json!("other")),
            ("credential_env", serde_json::json!("lowercase")),
        ] {
            let mut invalid = value.clone();
            invalid[field] = bad;
            let cfg: super::ApiConfig = serde_json::from_value(invalid).unwrap();
            assert_eq!(
                super::validate(&cfg).unwrap_err().code(),
                "EXTRACT_API_CONFIG"
            );
        }
    }
    #[test]
    fn selected_guard_does_not_consume_siblings_and_rejects_hardlinks_and_size() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target")
            .join(format!("api-config-unit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("config.json");
        std::fs::write(&path, serde_json::to_vec(&config_value()).unwrap()).unwrap();
        let sibling = root.join("unselected-private.bin");
        std::fs::write(&sibling, b"not a configuration").unwrap();
        let sibling_guard = crate::project::read_guard(&sibling).unwrap();
        let selected = super::SelectedConfig::open(&path).unwrap();
        selected.verify().unwrap();
        #[cfg(windows)]
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        drop(selected);
        drop(sibling_guard);
        let alias = root.join("alias.json");
        std::fs::hard_link(&path, &alias).unwrap();
        let err = super::SelectedConfig::open(&path).err().unwrap();
        assert_eq!(err.code(), "EXTRACT_API_CONFIG");
        std::fs::remove_file(&alias).unwrap();
        std::fs::write(&path, vec![b' '; 65537]).unwrap();
        assert_eq!(
            super::SelectedConfig::open(&path).err().unwrap().code(),
            "EXTRACT_API_CONFIG"
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(sibling).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn endpoint_allows_only_explicit_numeric_loopback_or_official_live() {
        assert!(super::valid_endpoint(
            "local_http_test",
            "http://127.0.0.1:1234/v1/responses"
        ));
        assert!(super::valid_endpoint(
            "local_http_test",
            "http://[::1]:1234/v1/responses"
        ));
        for value in [
            "http://localhost:1234/v1/responses",
            "http://127.1:1234/v1/responses",
            "http://127.0.0.1/v1/responses",
            "http://127.0.0.1:1234/v1/responses?x=1",
            "http://user@127.0.0.1:1234/v1/responses",
        ] {
            assert!(!super::valid_endpoint("local_http_test", value), "{value}");
        }
        assert!(super::valid_endpoint(
            "live",
            "https://api.openai.com/v1/responses"
        ));
        assert!(!super::valid_endpoint(
            "live",
            "https://api.openai.com:443/v1/responses"
        ));
    }
}
use crate::{
    extraction_model::{Budget, ExtractionLimits, FakeCapabilities, Selection},
    project::{self, ProjectError},
    sidecar_workspace::check_links,
    text::sha256,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApiConfig {
    pub(crate) schema_version: u32,
    pub(crate) provider: String,
    pub(crate) transport_mode: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) credential_env: String,
    pub(crate) required_input: String,
    pub(crate) capabilities: FakeCapabilities,
    pub(crate) selection: Vec<Selection>,
    pub(crate) limits: ExtractionLimits,
    pub(crate) budget: Budget,
}
const SELECTED_GATEWAY_ENDPOINT: &str = "https://ai.1343263.xyz/v1/responses";

#[derive(Clone, Copy)]
pub(crate) enum RequestStrategy {
    OpenAiStrict,
    TangmaoDeepseek,
}
impl RequestStrategy {
    pub(crate) fn version(self) -> &'static str {
        match self {
            Self::OpenAiStrict => "openai-responses-strict-json-schema-v1",
            Self::TangmaoDeepseek => "tangmao-deepseek-responses-json-schema-v1",
        }
    }
}
impl ApiConfig {
    // Called only after SelectedConfig validates the exact endpoint/model pair.
    pub(crate) fn request_strategy(&self) -> RequestStrategy {
        if self.transport_mode == "live" && self.endpoint == SELECTED_GATEWAY_ENDPOINT {
            RequestStrategy::TangmaoDeepseek
        } else {
            RequestStrategy::OpenAiStrict
        }
    }
}
fn config_error() -> ProjectError {
    ProjectError::new(
        "EXTRACT_API_CONFIG",
        2,
        "所选配置不符合严格文本API契约或64 KiB边界。",
    )
}
fn changed() -> ProjectError {
    ProjectError::new(
        "EXTRACT_API_CONFIG_CHANGED",
        3,
        "所选配置身份、mtime或字节发生变化。",
    )
}

fn valid_endpoint(mode: &str, endpoint: &str) -> bool {
    if mode == "live" {
        return matches!(endpoint, "https://api.openai.com/v1/responses")
            || endpoint == SELECTED_GATEWAY_ENDPOINT;
    }
    if mode != "local_http_test" {
        return false;
    }
    // Check the actual spelling before URL normalization; abbreviated and named
    // loopback hosts must never acquire authority by parser canonicalization.
    let Some(tail) = endpoint.strip_prefix("http://") else {
        return false;
    };
    let Some(authority) = tail.strip_suffix("/v1/responses") else {
        return false;
    };
    let port = authority
        .strip_prefix("127.0.0.1:")
        .or_else(|| authority.strip_prefix("[::1]:"));
    port.is_some_and(|p| {
        !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u16>().is_ok_and(|n| n > 0)
    })
}
fn validate(config: &ApiConfig) -> Result<(), ProjectError> {
    let l = &config.limits;
    let name = &config.credential_env;
    let cost_ok = config.budget.max_cost_usd.as_ref().is_none_or(|s| {
        !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            && s.bytes().filter(|&b| b == b'.').count() <= 1
            && s.parse::<f64>().is_ok_and(|n| n.is_finite() && n > 0.0)
    });
    if config.schema_version != 1
        || config.provider != "openai_responses"
        || !matches!(config.transport_mode.as_str(), "live" | "local_http_test")
        || !matches!(config.required_input.as_str(), "text" | "vision")
        || config.model.trim().is_empty()
        || config.model.len() > 256
        || config.model.chars().any(char::is_control)
        || !(2..=64).contains(&name.len())
        || !name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_uppercase() || b == b'_')
        || !name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || !(1..=4).contains(&l.max_units)
        || config.selection.is_empty()
        || config.selection.len() > l.max_units
        || !(1..=65536).contains(&l.max_input_bytes)
        || !(1..=1048576).contains(&l.max_response_bytes)
        || !(16..=8192).contains(&l.max_output_tokens)
        || !(1..=60000).contains(&l.timeout_ms)
        || config.budget.max_requests > 1
        || !cost_ok
    {
        return Err(config_error());
    }
    if !valid_endpoint(&config.transport_mode, &config.endpoint) {
        return Err(ProjectError::new(
            "EXTRACT_API_ENDPOINT",
            2,
            "端点必须为明确的官方/所选网关HTTPS Responses或数字环回HTTP测试地址。",
        ));
    }
    if config.endpoint == SELECTED_GATEWAY_ENDPOINT && config.model != "deepseek-flash" {
        return Err(config_error());
    }
    if config.transport_mode == "local_http_test" && !config.model.starts_with("noetome-test-") {
        return Err(config_error());
    }
    Ok(())
}

pub(crate) struct SelectedConfig {
    pub(crate) path: PathBuf,
    pub(crate) config: ApiConfig,
    file: File,
    identity: same_file::Handle,
    modified: SystemTime,
    sha: String,
    len: u64,
}
impl SelectedConfig {
    pub(crate) fn fingerprint(&self) -> crate::sidecar_model::Fingerprint {
        crate::sidecar_model::Fingerprint {
            sha256: self.sha.clone(),
            byte_length: self.len,
        }
    }
    pub(crate) fn open(path: &Path) -> Result<Self, ProjectError> {
        let path = project::check_components(path).map_err(|_| config_error())?;
        let meta = project::checked_file(&path).map_err(|_| config_error())?;
        if meta.len() > 65536 {
            return Err(config_error());
        }
        let mut file = project::read_guard(&path).map_err(|_| config_error())?;
        check_links(&file, "EXTRACT_API_CONFIG").map_err(|_| config_error())?;
        let identity = same_file::Handle::from_file(file.try_clone().map_err(|_| config_error())?)
            .map_err(|_| config_error())?;
        if same_file::Handle::from_path(&path).map_err(|_| config_error())? != identity {
            return Err(config_error());
        }
        let modified = file
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|_| config_error())?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| config_error())?;
        if bytes.len() > 65536 {
            return Err(config_error());
        }
        let config: ApiConfig = serde_json::from_slice(&bytes).map_err(|_| config_error())?;
        validate(&config)?;
        let value = Self {
            path,
            config,
            file,
            identity,
            modified,
            sha: sha256(&bytes),
            len: bytes.len() as u64,
        };
        value.verify()?;
        Ok(value)
    }
    pub(crate) fn verify(&self) -> Result<(), ProjectError> {
        project::check_components(&self.path).map_err(|_| changed())?;
        project::checked_file(&self.path).map_err(|_| changed())?;
        check_links(&self.file, "EXTRACT_API_CONFIG_CHANGED").map_err(|_| changed())?;
        if same_file::Handle::from_path(&self.path).map_err(|_| changed())? != self.identity {
            return Err(changed());
        }
        let mut reader = self.file.try_clone().map_err(|_| changed())?;
        reader.seek(SeekFrom::Start(0)).map_err(|_| changed())?;
        let mut bytes = Vec::new();
        reader
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| changed())?;
        if bytes.len() as u64 != self.len
            || sha256(&bytes) != self.sha
            || self
                .file
                .metadata()
                .and_then(|m| m.modified())
                .map_err(|_| changed())?
                != self.modified
            || same_file::Handle::from_path(&self.path).map_err(|_| changed())? != self.identity
        {
            return Err(changed());
        }
        Ok(())
    }
    pub(crate) fn credential(&self) -> Result<String, ProjectError> {
        let fail = || {
            ProjectError::new(
                "EXTRACT_API_CREDENTIAL",
                2,
                "所选外部凭据缺失、无效或不符合本地人工测试前缀。",
            )
        };
        let value = std::env::var(&self.config.credential_env).map_err(|_| fail())?;
        if value.is_empty()
            || value.len() > 8192
            || value.chars().any(|c| c.is_control() || c.is_whitespace())
            || !value.is_ascii()
            || (self.config.transport_mode == "local_http_test"
                && !value.starts_with("noetome-test-"))
        {
            return Err(fail());
        }
        Ok(value)
    }
}
