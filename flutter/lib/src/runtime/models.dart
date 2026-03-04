class MaiRuntimeException implements Exception {
  final int code;
  final String message;

  MaiRuntimeException(this.code, this.message);

  @override
  String toString() => 'MaiRuntimeException(code: $code, message: $message)';
}

class DeviceProfile {
  final int totalRamBytes;
  final int freeRamBytes;
  final int cpuCores;
  final String cpuArch;
  final bool hasGpu;
  final String gpuType;
  final String platform;
  final double? batteryLevel;
  final bool isCharging;
  final int availableStorageBytes;
  final double? benchmarkTokensPerSec;

  DeviceProfile({
    required this.totalRamBytes,
    required this.freeRamBytes,
    required this.cpuCores,
    required this.cpuArch,
    required this.hasGpu,
    required this.gpuType,
    required this.platform,
    required this.batteryLevel,
    required this.isCharging,
    required this.availableStorageBytes,
    required this.benchmarkTokensPerSec,
  });

  factory DeviceProfile.fromJson(Map<String, dynamic> json) {
    return DeviceProfile(
      totalRamBytes: (json['total_ram_bytes'] as num).toInt(),
      freeRamBytes: (json['free_ram_bytes'] as num).toInt(),
      cpuCores: (json['cpu_cores'] as num).toInt(),
      cpuArch: json['cpu_arch'] as String,
      hasGpu: json['has_gpu'] as bool,
      gpuType: json['gpu_type'] as String,
      platform: json['platform'] as String,
      batteryLevel: (json['battery_level'] as num?)?.toDouble(),
      isCharging: json['is_charging'] as bool,
      availableStorageBytes: (json['available_storage_bytes'] as num).toInt(),
      benchmarkTokensPerSec: (json['benchmark_tokens_per_sec'] as num?)?.toDouble(),
    );
  }
}

class DownloadRequest {
  final String? sourcePath;
  final String? sourceUrl;
  final String? destinationPath;
  final String id;
  final String name;
  final String quant;

  DownloadRequest({
    required this.sourcePath,
    required this.sourceUrl,
    required this.destinationPath,
    required this.id,
    required this.name,
    required this.quant,
  });

  bool get hasExactlyOneSource {
    final hasPath = sourcePath != null && sourcePath!.trim().isNotEmpty;
    final hasUrl = sourceUrl != null && sourceUrl!.trim().isNotEmpty;
    return hasPath != hasUrl;
  }

  bool get hasAtMostOneSource {
    final hasPath = sourcePath != null && sourcePath!.trim().isNotEmpty;
    final hasUrl = sourceUrl != null && sourceUrl!.trim().isNotEmpty;
    return !(hasPath && hasUrl);
  }

  Map<String, dynamic> toJson() {
    final json = <String, dynamic>{
      'source_path': sourcePath,
      'source_url': sourceUrl,
      'id': id,
      'name': name,
      'quant': quant,
    };
    if (destinationPath != null && destinationPath!.trim().isNotEmpty) {
      json['destination_path'] = destinationPath;
    }
    return json;
  }
}

class CatalogModel {
  final String id;
  final String name;
  final String? downloadUrl;
  final String quantization;
  final int? sizeBytes;
  final int? minRamBytes;

  CatalogModel({
    required this.id,
    required this.name,
    required this.downloadUrl,
    required this.quantization,
    required this.sizeBytes,
    required this.minRamBytes,
  });

  factory CatalogModel.fromJson(Map<String, dynamic> json) {
    return CatalogModel(
      id: json['id'] as String,
      name: json['name'] as String,
      downloadUrl: json['download_url'] as String?,
      quantization: (json['quantization'] as String?) ?? 'Q4KM',
      sizeBytes: (json['size_bytes'] as num?)?.toInt(),
      minRamBytes: (json['min_ram_bytes'] as num?)?.toInt(),
    );
  }
}

class HubSearchRequest {
  final String? query;
  final int limit;
  final String? cursor;
  final bool onlyGguf;
  final String? hfToken;

  HubSearchRequest({
    required this.query,
    this.limit = 50,
    this.cursor,
    this.onlyGguf = true,
    this.hfToken,
  });

  Map<String, dynamic> toJson() {
    final json = <String, dynamic>{
      'limit': limit,
      'only_gguf': onlyGguf,
    };
    if (query != null && query!.trim().isNotEmpty) {
      json['query'] = query;
    }
    if (cursor != null && cursor!.trim().isNotEmpty) {
      json['cursor'] = cursor;
    }
    if (hfToken != null && hfToken!.trim().isNotEmpty) {
      json['hf_token'] = hfToken;
    }
    return json;
  }
}

class HubModelFile {
  final String filename;
  final int? sizeBytes;
  final String downloadUrl;
  final String? quantization;

  HubModelFile({
    required this.filename,
    required this.sizeBytes,
    required this.downloadUrl,
    required this.quantization,
  });

  factory HubModelFile.fromJson(Map<String, dynamic> json) {
    return HubModelFile(
      filename: json['filename'] as String,
      sizeBytes: (json['size_bytes'] as num?)?.toInt(),
      downloadUrl: json['download_url'] as String,
      quantization: json['quantization'] as String?,
    );
  }
}

class HubModel {
  final String id;
  final int downloads;
  final int likes;
  final List<String> tags;
  final List<HubModelFile> ggufFiles;

  HubModel({
    required this.id,
    required this.downloads,
    required this.likes,
    required this.tags,
    required this.ggufFiles,
  });

  factory HubModel.fromJson(Map<String, dynamic> json) {
    final filesRaw = (json['gguf_files'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return HubModel(
      id: json['id'] as String,
      downloads: (json['downloads'] as num?)?.toInt() ?? 0,
      likes: (json['likes'] as num?)?.toInt() ?? 0,
      tags: (json['tags'] as List<dynamic>? ?? const <dynamic>[])
          .map((v) => v.toString())
          .toList(growable: false),
      ggufFiles: filesRaw.map(HubModelFile.fromJson).toList(growable: false),
    );
  }
}

class DownloadJob {
  final String jobId;
  final String modelId;
  final String modelName;
  final String quant;
  final String source;
  final String destinationPath;
  final String status;
  final int resumedFromBytes;
  final int downloadedBytes;
  final int? totalBytes;
  final double? progressPct;
  final int retries;
  final String createdAt;
  final String updatedAt;
  final String? completedAt;
  final String? error;

  DownloadJob({
    required this.jobId,
    required this.modelId,
    required this.modelName,
    required this.quant,
    required this.source,
    required this.destinationPath,
    required this.status,
    required this.resumedFromBytes,
    required this.downloadedBytes,
    required this.totalBytes,
    required this.progressPct,
    required this.retries,
    required this.createdAt,
    required this.updatedAt,
    required this.completedAt,
    required this.error,
  });

  factory DownloadJob.fromJson(Map<String, dynamic> json) {
    return DownloadJob(
      jobId: json['job_id'] as String,
      modelId: json['model_id'] as String,
      modelName: json['model_name'] as String,
      quant: json['quant'] as String,
      source: json['source'] as String,
      destinationPath: json['destination_path'] as String,
      status: json['status'] as String,
      resumedFromBytes: (json['resumed_from_bytes'] as num).toInt(),
      downloadedBytes: (json['downloaded_bytes'] as num).toInt(),
      totalBytes: (json['total_bytes'] as num?)?.toInt(),
      progressPct: (json['progress_pct'] as num?)?.toDouble(),
      retries: (json['retries'] as num).toInt(),
      createdAt: json['created_at'] as String,
      updatedAt: json['updated_at'] as String,
      completedAt: json['completed_at'] as String?,
      error: json['error'] as String?,
    );
  }
}

class RuntimeMetrics {
  final int inferenceTotal;
  final int inferenceErrorsTotal;
  final int activeStreams;
  final int downloadsStartedTotal;
  final int downloadsCompletedTotal;
  final int downloadsFailedTotal;
  final int downloadsActive;
  final int downloadBytesTotal;
  final int ramTotalBytes;
  final int ramFreeBytes;

  RuntimeMetrics({
    required this.inferenceTotal,
    required this.inferenceErrorsTotal,
    required this.activeStreams,
    required this.downloadsStartedTotal,
    required this.downloadsCompletedTotal,
    required this.downloadsFailedTotal,
    required this.downloadsActive,
    required this.downloadBytesTotal,
    required this.ramTotalBytes,
    required this.ramFreeBytes,
  });

  factory RuntimeMetrics.fromJson(Map<String, dynamic> json) {
    return RuntimeMetrics(
      inferenceTotal: (json['inference_total'] as num).toInt(),
      inferenceErrorsTotal: (json['inference_errors_total'] as num).toInt(),
      activeStreams: (json['active_streams'] as num).toInt(),
      downloadsStartedTotal: (json['downloads_started_total'] as num).toInt(),
      downloadsCompletedTotal: (json['downloads_completed_total'] as num).toInt(),
      downloadsFailedTotal: (json['downloads_failed_total'] as num).toInt(),
      downloadsActive: (json['downloads_active'] as num).toInt(),
      downloadBytesTotal: (json['download_bytes_total'] as num).toInt(),
      ramTotalBytes: (json['ram_total_bytes'] as num).toInt(),
      ramFreeBytes: (json['ram_free_bytes'] as num).toInt(),
    );
  }
}

class MaiCompletion {
  final int completionId;
  final Stream<String> stream;

  MaiCompletion({required this.completionId, required this.stream});
}
