use std::collections::HashMap;

use inference_engine::InferenceBackend;
use model_manager::ModelBackend;

/// Build the backend map from compiled-in features.
/// Returns an error if no backends are available.
pub fn create_backends(
) -> Result<HashMap<ModelBackend, Box<dyn InferenceBackend>>, String> {
    #[allow(unused_mut)]
    let mut map: HashMap<ModelBackend, Box<dyn InferenceBackend>> = HashMap::new();

    #[cfg(feature = "llama-backend")]
    {
        let b = inference_engine::llama_backend::LlamaBackendWrapper::new()
            .map_err(|e| e.to_string())?;
        map.insert(ModelBackend::Llama, Box::new(b));
    }

    if map.is_empty() {
        return Err(
            "No inference backend compiled in. Rebuild with --features llama-backend"
                .into(),
        );
    }

    Ok(map)
}
