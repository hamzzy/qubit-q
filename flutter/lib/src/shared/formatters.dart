String formatBytes(num bytes) {
  if (bytes < 1024) {
    return '${bytes.toStringAsFixed(0)} B';
  }

  const units = <String>['KB', 'MB', 'GB', 'TB'];
  var value = bytes / 1024.0;
  var index = 0;

  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }

  return '${value.toStringAsFixed(1)} ${units[index]}';
}

String formatPercent(double? value) {
  if (value == null) {
    return '--';
  }
  return '${value.toStringAsFixed(1)}%';
}

String formatTimestamp(String? iso8601) {
  if (iso8601 == null || iso8601.isEmpty) {
    return '--';
  }

  final dt = DateTime.tryParse(iso8601);
  if (dt == null) {
    return iso8601;
  }

  final local = dt.toLocal();
  final hh = local.hour.toString().padLeft(2, '0');
  final mm = local.minute.toString().padLeft(2, '0');
  final ss = local.second.toString().padLeft(2, '0');
  return '${local.year}-${local.month.toString().padLeft(2, '0')}-${local.day.toString().padLeft(2, '0')} $hh:$mm:$ss';
}
