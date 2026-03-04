#ifndef MOBILE_AI_RUNTIME_H
#define MOBILE_AI_RUNTIME_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
  #define MAI_API __declspec(dllimport)
#else
  #define MAI_API
#endif

typedef struct RuntimeHandle RuntimeHandle;

/**
 * Token callback invoked for each emitted token.
 *
 * A final invocation with `token == NULL` indicates completion (or cancellation)
 * and that user callback state can be released.
 */
typedef void (*TokenCallback)(const char *token, void *user_data);

/**
 * Initialize runtime with optional JSON config.
 * Pass NULL or "{}" for defaults.
 * Returns NULL on failure.
 */
MAI_API RuntimeHandle *mai_runtime_init(const char *config_json);

/**
 * Destroy runtime and cancel active completions.
 */
MAI_API void mai_runtime_destroy(RuntimeHandle *handle);

/**
 * Load model by model ID from registry.
 * Returns 0 on success, negative error code on failure.
 */
MAI_API int mai_load_model(RuntimeHandle *handle, const char *model_id);

/**
 * Unload currently loaded model.
 * Returns 0 on success, negative error code on failure.
 */
MAI_API int mai_unload_model(RuntimeHandle *handle);

/**
 * Start token streaming completion.
 * Non-blocking: returns immediately.
 *
 * - `completion_id` is an output pointer for cancellation.
 * - callback receives tokens and one final NULL token on completion.
 *
 * Returns 0 on success, negative error code on failure.
 */
MAI_API int mai_chat_completion(
    RuntimeHandle *handle,
    const char *prompt,
    TokenCallback callback,
    void *user_data,
    uint64_t *completion_id
);

/**
 * Cancel an in-flight completion.
 * Returns 0 on success, -4 if completion id not found.
 */
MAI_API int mai_cancel_completion(RuntimeHandle *handle, uint64_t completion_id);

/**
 * Start async model download and return a new job id in `out_job_id`.
 * `request_json` must match the download request schema.
 * Returns 0 on success, negative error code on failure.
 */
MAI_API int mai_download_start(
    RuntimeHandle *handle,
    const char *request_json,
    char **out_job_id
);

/**
 * Return one download job snapshot as JSON.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_download_status_json(RuntimeHandle *handle, const char *job_id);

/**
 * Return all download jobs as JSON.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_download_list_json(RuntimeHandle *handle);

/**
 * Retry a download job and return a new job id in `out_new_job_id`.
 * Returns 0 on success, -4 when original job id is not found.
 */
MAI_API int mai_download_retry(
    RuntimeHandle *handle,
    const char *job_id,
    char **out_new_job_id
);

/**
 * Cancel a queued/running download job.
 * Returns 0 on success, -4 when job is not found.
 */
MAI_API int mai_download_cancel(RuntimeHandle *handle, const char *job_id);

/**
 * Delete a download job from tracker.
 * When `delete_file` is true, also remove the destination file.
 */
MAI_API int mai_download_delete(RuntimeHandle *handle, const char *job_id, bool delete_file);

/**
 * Return runtime observability metrics as JSON.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_metrics_json(RuntimeHandle *handle);

/**
 * Return catalog JSON used by the runtime.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_model_catalog_json(RuntimeHandle *handle);

/**
 * Search Hugging Face-compatible models and return JSON list.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_hub_search_models_json(RuntimeHandle *handle, const char *request_json);

/**
 * Returns a heap-allocated JSON string (UTF-8) for device profile.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_device_profile_json(RuntimeHandle *handle);

/**
 * Returns the last native error message captured by the runtime.
 * Caller must free with `mai_free_string`.
 */
MAI_API char *mai_last_error_message(void);

/**
 * Free string returned by this API.
 */
MAI_API void mai_free_string(char *s);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // MOBILE_AI_RUNTIME_H
