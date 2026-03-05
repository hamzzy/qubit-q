import 'dart:ffi';

import 'package:ffi/ffi.dart';

final class RuntimeHandle extends Opaque {}

typedef TokenCallbackNative = Void Function(Pointer<Utf8>, Pointer<Void>);
typedef TokenCallbackDart = void Function(Pointer<Utf8>, Pointer<Void>);

class MaiBindings {
  MaiBindings(DynamicLibrary lib)
      : maiRuntimeInit = lib.lookupFunction<
            Pointer<RuntimeHandle> Function(Pointer<Utf8>),
            Pointer<RuntimeHandle> Function(Pointer<Utf8>)>('mai_runtime_init'),
        maiRuntimeDestroy = lib.lookupFunction<
            Void Function(Pointer<RuntimeHandle>),
            void Function(Pointer<RuntimeHandle>)>('mai_runtime_destroy'),
        maiLoadModel = lib.lookupFunction<
            Int32 Function(Pointer<RuntimeHandle>, Pointer<Utf8>),
            int Function(
                Pointer<RuntimeHandle>, Pointer<Utf8>)>('mai_load_model'),
        maiUnloadModel = lib.lookupFunction<
            Int32 Function(Pointer<RuntimeHandle>),
            int Function(Pointer<RuntimeHandle>)>('mai_unload_model'),
        maiChatCompletion = lib.lookupFunction<
            Int32 Function(
                Pointer<RuntimeHandle>,
                Pointer<Utf8>,
                Pointer<NativeFunction<TokenCallbackNative>>,
                Pointer<Void>,
                Pointer<Uint64>),
            int Function(
                Pointer<RuntimeHandle>,
                Pointer<Utf8>,
                Pointer<NativeFunction<TokenCallbackNative>>,
                Pointer<Void>,
                Pointer<Uint64>)>('mai_chat_completion'),
        maiCancelCompletion = lib.lookupFunction<
            Int32 Function(Pointer<RuntimeHandle>, Uint64),
            int Function(Pointer<RuntimeHandle>, int)>('mai_cancel_completion'),
        maiDeviceProfileJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>),
            Pointer<Utf8> Function(
                Pointer<RuntimeHandle>)>('mai_device_profile_json'),
        maiDownloadStart = lib.lookupFunction<
            Int32 Function(
                Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>),
            int Function(
                Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>)>(
          'mai_download_start',
        ),
        maiDownloadStatusJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>, Pointer<Utf8>),
            Pointer<Utf8> Function(Pointer<RuntimeHandle>,
                Pointer<Utf8>)>('mai_download_status_json'),
        maiDownloadListJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>),
            Pointer<Utf8> Function(
                Pointer<RuntimeHandle>)>('mai_download_list_json'),
        maiDownloadRetry = lib.lookupFunction<
            Int32 Function(
                Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>),
            int Function(
                Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>)>(
          'mai_download_retry',
        ),
        maiDownloadCancel = lib.lookupFunction<
            Int32 Function(Pointer<RuntimeHandle>, Pointer<Utf8>),
            int Function(Pointer<RuntimeHandle>, Pointer<Utf8>)>(
          'mai_download_cancel',
        ),
        maiDownloadDelete = lib.lookupFunction<
            Int32 Function(Pointer<RuntimeHandle>, Pointer<Utf8>, Bool),
            int Function(Pointer<RuntimeHandle>, Pointer<Utf8>, bool)>(
          'mai_download_delete',
        ),
        maiMetricsJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>),
            Pointer<Utf8> Function(Pointer<RuntimeHandle>)>('mai_metrics_json'),
        maiModelCatalogJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>),
            Pointer<Utf8> Function(
                Pointer<RuntimeHandle>)>('mai_model_catalog_json'),
        maiHubSearchModelsJson = lib.lookupFunction<
            Pointer<Utf8> Function(Pointer<RuntimeHandle>, Pointer<Utf8>),
            Pointer<Utf8> Function(Pointer<RuntimeHandle>,
                Pointer<Utf8>)>('mai_hub_search_models_json'),
        maiLastErrorMessage = lib.lookupFunction<Pointer<Utf8> Function(),
            Pointer<Utf8> Function()>('mai_last_error_message'),
        maiFreeString = lib.lookupFunction<Void Function(Pointer<Utf8>),
            void Function(Pointer<Utf8>)>('mai_free_string') {
    try {
      maiChatCompletionWithParams = lib.lookupFunction<
          Int32 Function(
            Pointer<RuntimeHandle>,
            Pointer<Utf8>,
            Pointer<Utf8>,
            Pointer<NativeFunction<TokenCallbackNative>>,
            Pointer<Void>,
            Pointer<Uint64>,
          ),
          int Function(
            Pointer<RuntimeHandle>,
            Pointer<Utf8>,
            Pointer<Utf8>,
            Pointer<NativeFunction<TokenCallbackNative>>,
            Pointer<Void>,
            Pointer<Uint64>,
          )>('mai_chat_completion_with_params');
    } catch (_) {
      maiChatCompletionWithParams = null;
    }
  }

  final Pointer<RuntimeHandle> Function(Pointer<Utf8>) maiRuntimeInit;
  final void Function(Pointer<RuntimeHandle>) maiRuntimeDestroy;
  final int Function(Pointer<RuntimeHandle>, Pointer<Utf8>) maiLoadModel;
  final int Function(Pointer<RuntimeHandle>) maiUnloadModel;
  final int Function(
    Pointer<RuntimeHandle>,
    Pointer<Utf8>,
    Pointer<NativeFunction<TokenCallbackNative>>,
    Pointer<Void>,
    Pointer<Uint64>,
  ) maiChatCompletion;
  late final int Function(
    Pointer<RuntimeHandle>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<NativeFunction<TokenCallbackNative>>,
    Pointer<Void>,
    Pointer<Uint64>,
  )? maiChatCompletionWithParams;
  final int Function(Pointer<RuntimeHandle>, int) maiCancelCompletion;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>) maiDeviceProfileJson;
  final int Function(
          Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>)
      maiDownloadStart;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>, Pointer<Utf8>)
      maiDownloadStatusJson;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>) maiDownloadListJson;
  final int Function(
          Pointer<RuntimeHandle>, Pointer<Utf8>, Pointer<Pointer<Utf8>>)
      maiDownloadRetry;
  final int Function(Pointer<RuntimeHandle>, Pointer<Utf8>) maiDownloadCancel;
  final int Function(Pointer<RuntimeHandle>, Pointer<Utf8>, bool)
      maiDownloadDelete;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>) maiMetricsJson;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>) maiModelCatalogJson;
  final Pointer<Utf8> Function(Pointer<RuntimeHandle>, Pointer<Utf8>)
      maiHubSearchModelsJson;
  final Pointer<Utf8> Function() maiLastErrorMessage;
  final void Function(Pointer<Utf8>) maiFreeString;
}
