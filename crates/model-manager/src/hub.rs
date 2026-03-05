//! Hugging Face Hub integration — search models, filter GGUF files, construct download URLs.

use reqwest::Url;
use serde::{Deserialize, Serialize};

const HF_MODELS_API: &str = "https://huggingface.co/api/models";

// ── Request / Response types ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HubSearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub pipeline_tag: Option<String>,
    pub author: Option<String>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub only_gguf: Option<bool>,
    pub hf_token: Option<String>,
}

impl Default for HubSearchRequest {
    fn default() -> Self {
        Self {
            query: None,
            limit: Some(50),
            cursor: None,
            pipeline_tag: Some("text-generation".to_string()),
            author: None,
            sort: None,
            direction: None,
            only_gguf: Some(true),
            hf_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubModelFile {
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub download_url: String,
    pub quantization: Option<String>,
    /// "llama" for GGUF files, "mlx" for MLX safetensors repos.
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubModelSummary {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub tags: Vec<String>,
    pub gguf_files: Vec<HubModelFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubModelListResponse {
    pub object: String,
    pub data: Vec<HubModelSummary>,
    pub next_cursor: Option<String>,
}

// ── Internal HF API response shapes ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct HuggingFaceModelApi {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "modelId", default)]
    model_id: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    likes: Option<u64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    siblings: Vec<HuggingFaceSibling>,
}

#[derive(Debug, Clone, Deserialize)]
struct HuggingFaceSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Search the Hugging Face Hub for models, optionally filtering to GGUF-only.
pub async fn search_hf_models(request: HubSearchRequest) -> Result<HubModelListResponse, String> {
    let mut url = Url::parse(HF_MODELS_API).map_err(|e| e.to_string())?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("full", "true");

        let limit = request.limit.unwrap_or(50).clamp(1, 200);
        qp.append_pair("limit", &limit.to_string());

        if let Some(query) = request
            .query
            .as_ref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
        {
            qp.append_pair("search", query);
        }
        if let Some(pipeline_tag) = request
            .pipeline_tag
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("pipeline_tag", pipeline_tag);
        }
        if let Some(author) = request
            .author
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("author", author);
        }
        if let Some(sort) = request
            .sort
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("sort", sort);
        }
        if let Some(direction) = request
            .direction
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("direction", direction);
        }
        if let Some(cursor) = request
            .cursor
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("cursor", cursor);
        }
    }

    let client = reqwest::Client::new();
    let mut http = client.get(url);
    if let Some(token) = request
        .hf_token
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        http = http.bearer_auth(token);
    }

    let response = http.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("huggingface api returned {}", response.status()));
    }

    let next_cursor = parse_hf_next_cursor(
        response
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok()),
    );

    let raw: Vec<HuggingFaceModelApi> = response.json().await.map_err(|e| e.to_string())?;
    let mut models = raw
        .into_iter()
        .filter_map(map_hf_model_summary)
        .collect::<Vec<_>>();

    if request.only_gguf.unwrap_or(true) {
        models.retain(|m| !m.gguf_files.is_empty());
    }

    Ok(HubModelListResponse {
        object: "list".to_string(),
        data: models,
        next_cursor,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn map_hf_model_summary(raw: HuggingFaceModelApi) -> Option<HubModelSummary> {
    let model_id = raw.id.or(raw.model_id)?;
    let gguf_files = raw
        .siblings
        .into_iter()
        .filter(|s| is_gguf_file(&s.rfilename))
        .map(|s| HubModelFile {
            download_url: format!(
                "https://huggingface.co/{}/resolve/main/{}",
                model_id, s.rfilename
            ),
            quantization: infer_quantization_from_filename(&s.rfilename),
            filename: s.rfilename,
            size_bytes: s.size,
            backend: "llama".to_string(),
        })
        .collect::<Vec<_>>();

    Some(HubModelSummary {
        id: model_id,
        downloads: raw.downloads.unwrap_or(0),
        likes: raw.likes.unwrap_or(0),
        tags: raw.tags,
        gguf_files,
    })
}

pub fn is_gguf_file(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with(".gguf")
}

pub fn infer_quantization_from_filename(filename: &str) -> Option<String> {
    let upper = filename.to_ascii_uppercase();
    let known = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_1", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_1", "Q4_0",
        "Q3_K_M", "Q3_K_S", "Q2_K",
    ];
    known
        .iter()
        .find(|pattern| upper.contains(**pattern))
        .map(|s| (*s).to_string())
}

fn parse_hf_next_cursor(link_header: Option<&str>) -> Option<String> {
    let link_header = link_header?;
    for part in link_header.split(',') {
        if !part.contains("rel=\"next\"") {
            continue;
        }
        let start = part.find('<')?;
        let end = part.find('>')?;
        let url = &part[start + 1..end];
        let parsed = Url::parse(url).ok()?;
        for (key, value) in parsed.query_pairs() {
            if key == "cursor" {
                return Some(value.into_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_quantization_works() {
        assert_eq!(
            infer_quantization_from_filename("model-q4_k_m.gguf"),
            Some("Q4_K_M".to_string())
        );
        assert_eq!(
            infer_quantization_from_filename("model-q8_0.gguf"),
            Some("Q8_0".to_string())
        );
        assert_eq!(infer_quantization_from_filename("model.gguf"), None);
    }

    #[test]
    fn is_gguf_detection() {
        assert!(is_gguf_file("model.gguf"));
        assert!(is_gguf_file("Model.GGUF"));
        assert!(!is_gguf_file("model.bin"));
    }

    #[test]
    fn default_request_has_sensible_values() {
        let req = HubSearchRequest::default();
        assert_eq!(req.limit, Some(50));
        assert_eq!(req.only_gguf, Some(true));
        assert_eq!(req.pipeline_tag, Some("text-generation".to_string()));
    }
}
