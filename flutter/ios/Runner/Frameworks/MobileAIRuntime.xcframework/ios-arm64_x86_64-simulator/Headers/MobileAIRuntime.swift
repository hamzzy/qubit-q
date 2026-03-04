import Foundation
#if false && canImport(MobileAIRuntimeFFI)
import MobileAIRuntimeFFI
#else
public enum RuntimeHandle {}
public typealias TokenCallback = @convention(c) (UnsafePointer<CChar>?, UnsafeMutableRawPointer?) -> Void

@_silgen_name("mai_runtime_init")
func mai_runtime_init(_ config_json: UnsafePointer<CChar>?) -> UnsafeMutablePointer<RuntimeHandle>?
@_silgen_name("mai_runtime_destroy")
func mai_runtime_destroy(_ handle: UnsafeMutablePointer<RuntimeHandle>?)
@_silgen_name("mai_load_model")
func mai_load_model(_ handle: UnsafeMutablePointer<RuntimeHandle>?, _ model_id: UnsafePointer<CChar>?) -> Int32
@_silgen_name("mai_unload_model")
func mai_unload_model(_ handle: UnsafeMutablePointer<RuntimeHandle>?) -> Int32
@_silgen_name("mai_chat_completion")
func mai_chat_completion(
    _ handle: UnsafeMutablePointer<RuntimeHandle>?,
    _ prompt: UnsafePointer<CChar>?,
    _ callback: TokenCallback?,
    _ user_data: UnsafeMutableRawPointer?,
    _ completion_id: UnsafeMutablePointer<UInt64>?
) -> Int32
@_silgen_name("mai_cancel_completion")
func mai_cancel_completion(_ handle: UnsafeMutablePointer<RuntimeHandle>?, _ completion_id: UInt64) -> Int32
@_silgen_name("mai_download_start")
func mai_download_start(
    _ handle: UnsafeMutablePointer<RuntimeHandle>?,
    _ request_json: UnsafePointer<CChar>?,
    _ out_job_id: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32
@_silgen_name("mai_download_status_json")
func mai_download_status_json(
    _ handle: UnsafeMutablePointer<RuntimeHandle>?,
    _ job_id: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>?
@_silgen_name("mai_download_list_json")
func mai_download_list_json(_ handle: UnsafeMutablePointer<RuntimeHandle>?) -> UnsafeMutablePointer<CChar>?
@_silgen_name("mai_download_retry")
func mai_download_retry(
    _ handle: UnsafeMutablePointer<RuntimeHandle>?,
    _ job_id: UnsafePointer<CChar>?,
    _ out_new_job_id: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32
@_silgen_name("mai_metrics_json")
func mai_metrics_json(_ handle: UnsafeMutablePointer<RuntimeHandle>?) -> UnsafeMutablePointer<CChar>?
@_silgen_name("mai_device_profile_json")
func mai_device_profile_json(_ handle: UnsafeMutablePointer<RuntimeHandle>?) -> UnsafeMutablePointer<CChar>?
@_silgen_name("mai_free_string")
func mai_free_string(_ s: UnsafeMutablePointer<CChar>?)
#endif

public struct MAIRuntimeConfig: Codable {
    public var modelsDir: String?
    public var cacheDir: String?
    public var logsDir: String?
    public var maxStorageBytes: UInt64?
    public var maxContextTokens: Int?
    public var memorySafetyMarginPct: Float?
    public var inferenceTimeoutSecs: UInt64?
    public var africaMode: Bool?
    public var autoSelectQuantization: Bool?

    enum CodingKeys: String, CodingKey {
        case modelsDir = "models_dir"
        case cacheDir = "cache_dir"
        case logsDir = "logs_dir"
        case maxStorageBytes = "max_storage_bytes"
        case maxContextTokens = "max_context_tokens"
        case memorySafetyMarginPct = "memory_safety_margin_pct"
        case inferenceTimeoutSecs = "inference_timeout_secs"
        case africaMode = "africa_mode"
        case autoSelectQuantization = "auto_select_quantization"
    }

    public init(
        modelsDir: String? = nil,
        cacheDir: String? = nil,
        logsDir: String? = nil,
        maxStorageBytes: UInt64? = nil,
        maxContextTokens: Int? = nil,
        memorySafetyMarginPct: Float? = nil,
        inferenceTimeoutSecs: UInt64? = nil,
        africaMode: Bool? = nil,
        autoSelectQuantization: Bool? = nil
    ) {
        self.modelsDir = modelsDir
        self.cacheDir = cacheDir
        self.logsDir = logsDir
        self.maxStorageBytes = maxStorageBytes
        self.maxContextTokens = maxContextTokens
        self.memorySafetyMarginPct = memorySafetyMarginPct
        self.inferenceTimeoutSecs = inferenceTimeoutSecs
        self.africaMode = africaMode
        self.autoSelectQuantization = autoSelectQuantization
    }
}

public struct MAIDeviceProfile: Decodable {
    public let total_ram_bytes: UInt64
    public let free_ram_bytes: UInt64
    public let cpu_cores: UInt32
    public let cpu_arch: String
    public let has_gpu: Bool
    public let gpu_type: String
    public let platform: String
    public let battery_level: Float?
    public let is_charging: Bool
    public let available_storage_bytes: UInt64
    public let benchmark_tokens_per_sec: Float?
}

public struct MAIDownloadRequest: Encodable {
    public let source_path: String?
    public let source_url: String?
    public let destination_path: String
    public let id: String
    public let name: String
    public let quant: String

    public init(
        sourcePath: String? = nil,
        sourceURL: String? = nil,
        destinationPath: String,
        id: String,
        name: String,
        quant: String
    ) {
        self.source_path = sourcePath
        self.source_url = sourceURL
        self.destination_path = destinationPath
        self.id = id
        self.name = name
        self.quant = quant
    }
}

public struct MAIDownloadJob: Decodable {
    public let job_id: String
    public let model_id: String
    public let model_name: String
    public let quant: String
    public let source: String
    public let destination_path: String
    public let status: String
    public let resumed_from_bytes: UInt64
    public let downloaded_bytes: UInt64
    public let total_bytes: UInt64?
    public let progress_pct: Double?
    public let retries: Int
    public let created_at: String
    public let updated_at: String
    public let completed_at: String?
    public let error: String?
}

public struct MAIRuntimeMetrics: Decodable {
    public let inference_total: UInt64
    public let inference_errors_total: UInt64
    public let active_streams: UInt64
    public let downloads_started_total: UInt64
    public let downloads_completed_total: UInt64
    public let downloads_failed_total: UInt64
    public let downloads_active: UInt64
    public let download_bytes_total: UInt64
    public let ram_total_bytes: UInt64
    public let ram_free_bytes: UInt64
}

public enum MAIRuntimeError: Error {
    case initFailed
    case invalidUTF8
    case runtimeFailure
    case completionNotFound
    case unknown(code: Int32)
    case invalidHandle
    case invalidJSONString

    static func from(code: Int32) -> MAIRuntimeError? {
        switch code {
        case 0: return nil
        case -1: return .invalidHandle
        case -2: return .invalidUTF8
        case -3: return .runtimeFailure
        case -4: return .completionNotFound
        default: return .unknown(code: code)
        }
    }
}

private final class TokenCallbackBox {
    let onToken: (String) -> Void
    let onComplete: (() -> Void)?

    init(onToken: @escaping (String) -> Void, onComplete: (() -> Void)?) {
        self.onToken = onToken
        self.onComplete = onComplete
    }
}

public final class MobileAIRuntime {
    private var handle: UnsafeMutablePointer<RuntimeHandle>?

    private static let tokenTrampoline: @convention(c) (
        UnsafePointer<CChar>?,
        UnsafeMutableRawPointer?
    ) -> Void = { tokenPtr, userData in
        guard let userData else { return }

        if let tokenPtr {
            let box = Unmanaged<TokenCallbackBox>
                .fromOpaque(userData)
                .takeUnretainedValue()
            let token = String(cString: tokenPtr)
            DispatchQueue.main.async {
                box.onToken(token)
            }
        } else {
            // Final sentinel callback: consume retained callback box and signal completion.
            let box = Unmanaged<TokenCallbackBox>
                .fromOpaque(userData)
                .takeRetainedValue()
            DispatchQueue.main.async {
                box.onComplete?()
            }
        }
    }

    public init(config: MAIRuntimeConfig? = nil) throws {
        let configJSON: String
        if let config {
            let data = try JSONEncoder().encode(config)
            guard let json = String(data: data, encoding: .utf8) else {
                throw MAIRuntimeError.invalidUTF8
            }
            configJSON = json
        } else {
            configJSON = "{}"
        }

        let created: UnsafeMutablePointer<RuntimeHandle>? = configJSON.withCString { cStr in
            mai_runtime_init(cStr)
        }

        guard let created else {
            throw MAIRuntimeError.initFailed
        }
        self.handle = created
    }

    deinit {
        if let handle {
            mai_runtime_destroy(handle)
        }
    }

    public func loadModel(id: String) async throws {
        guard let handle else { throw MAIRuntimeError.invalidHandle }

        let result = try await Task.detached(priority: .userInitiated) {
            id.withCString { cStr in
                mai_load_model(handle, cStr)
            }
        }.value

        if let error = MAIRuntimeError.from(code: result) {
            throw error
        }
    }

    public func unloadModel() async throws {
        guard let handle else { throw MAIRuntimeError.invalidHandle }

        let result = try await Task.detached(priority: .userInitiated) {
            mai_unload_model(handle)
        }.value

        if let error = MAIRuntimeError.from(code: result) {
            throw error
        }
    }

    @discardableResult
    public func streamCompletion(
        prompt: String,
        onToken: @escaping (String) -> Void,
        onComplete: (() -> Void)? = nil
    ) async throws -> UInt64 {
        guard let handle else { throw MAIRuntimeError.invalidHandle }

        var completionId: UInt64 = 0
        let callbackBox = Unmanaged.passRetained(
            TokenCallbackBox(onToken: onToken, onComplete: onComplete)
        )

        let result: Int32 = prompt.withCString { cPrompt in
            mai_chat_completion(
                handle,
                cPrompt,
                MobileAIRuntime.tokenTrampoline,
                callbackBox.toOpaque(),
                &completionId
            )
        }

        if let error = MAIRuntimeError.from(code: result) {
            callbackBox.release()
            throw error
        }

        return completionId
    }

    public func cancelCompletion(id: UInt64) throws {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        let result = mai_cancel_completion(handle, id)
        if let error = MAIRuntimeError.from(code: result) {
            throw error
        }
    }

    public func deviceProfile() throws -> MAIDeviceProfile {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        guard let raw = mai_device_profile_json(handle) else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(raw)
        }

        let json = String(cString: raw)
        guard let data = json.data(using: .utf8) else {
            throw MAIRuntimeError.invalidUTF8
        }

        return try JSONDecoder().decode(MAIDeviceProfile.self, from: data)
    }

    public func startDownload(_ request: MAIDownloadRequest) throws -> String {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        let payload = try JSONEncoder().encode(request)
        guard let payloadString = String(data: payload, encoding: .utf8) else {
            throw MAIRuntimeError.invalidUTF8
        }

        var out: UnsafeMutablePointer<CChar>?
        let code: Int32 = payloadString.withCString { cStr in
            mai_download_start(handle, cStr, &out)
        }
        if let error = MAIRuntimeError.from(code: code) {
            throw error
        }
        guard let out else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(out)
        }
        return String(cString: out)
    }

    public func downloadStatus(jobId: String) throws -> MAIDownloadJob {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        guard let raw = jobId.withCString({ mai_download_status_json(handle, $0) }) else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(raw)
        }
        let data = String(cString: raw).data(using: .utf8) ?? Data()
        return try JSONDecoder().decode(MAIDownloadJob.self, from: data)
    }

    public func listDownloads() throws -> [MAIDownloadJob] {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        guard let raw = mai_download_list_json(handle) else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(raw)
        }
        let data = String(cString: raw).data(using: .utf8) ?? Data()
        struct Wrapper: Decodable {
            let data: [MAIDownloadJob]
        }
        return try JSONDecoder().decode(Wrapper.self, from: data).data
    }

    public func retryDownload(jobId: String) throws -> String {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        var out: UnsafeMutablePointer<CChar>?
        let code: Int32 = jobId.withCString { cStr in
            mai_download_retry(handle, cStr, &out)
        }
        if let error = MAIRuntimeError.from(code: code) {
            throw error
        }
        guard let out else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(out)
        }
        return String(cString: out)
    }

    public func runtimeMetrics() throws -> MAIRuntimeMetrics {
        guard let handle else { throw MAIRuntimeError.invalidHandle }
        guard let raw = mai_metrics_json(handle) else {
            throw MAIRuntimeError.runtimeFailure
        }
        defer {
            mai_free_string(raw)
        }
        let data = String(cString: raw).data(using: .utf8) ?? Data()
        return try JSONDecoder().decode(MAIRuntimeMetrics.self, from: data)
    }
}
