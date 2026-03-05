import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../runtime/runtime_provider.dart';
import '../../shared/theme.dart';

/// A single chat message.
class _ChatMessage {
  final String text;
  final bool isUser;
  final DateTime timestamp;

  _ChatMessage({required this.text, required this.isUser, DateTime? timestamp})
      : timestamp = timestamp ?? DateTime.now();
}

class ChatPage extends ConsumerStatefulWidget {
  const ChatPage({super.key});

  @override
  ConsumerState<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends ConsumerState<ChatPage> {
  final TextEditingController _promptController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final List<_ChatMessage> _messages = [];
  String _streamingResponse = '';
  bool _wasGenerating = false;

  @override
  void dispose() {
    _promptController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });
  }

  void _sendMessage(String text) {
    if (text.trim().isEmpty) return;
    final runtime = ref.read(runtimeControllerProvider);
    if (!runtime.initialized || runtime.isGenerating) return;

    setState(() {
      _messages.add(_ChatMessage(text: text.trim(), isUser: true));
      _streamingResponse = '';
    });
    _promptController.clear();
    _scrollToBottom();
    runtime.startCompletion(text.trim());
  }

  @override
  Widget build(BuildContext context) {
    final runtime = ref.watch(runtimeControllerProvider);
    final tps = runtime.estimatedTokensPerSec;

    // Track streaming response changes
    if (runtime.isGenerating && runtime.lastResponse.isNotEmpty) {
      _streamingResponse = runtime.lastResponse;
    }

    // When generation finishes, add the AI response to history
    if (_wasGenerating &&
        !runtime.isGenerating &&
        _streamingResponse.isNotEmpty) {
      _messages.add(_ChatMessage(text: _streamingResponse, isUser: false));
      _streamingResponse = '';
    } else if (_wasGenerating &&
        !runtime.isGenerating &&
        _streamingResponse.isEmpty &&
        runtime.lastError != null &&
        runtime.lastError!.isNotEmpty) {
      _messages.add(
          _ChatMessage(text: 'Error: ${runtime.lastError!}', isUser: false));
    }
    _wasGenerating = runtime.isGenerating;

    _scrollToBottom();

    return Column(
      children: [
        // Model header bar
        _ModelHeader(
          selectedModelId: runtime.selectedModelId,
          loadedModelId: runtime.loadedModelId,
          modelIds: runtime.availableModelIds,
          tps: tps,
          isGenerating: runtime.isGenerating,
          onSelectModel: runtime.selectModel,
          onOpenEngineRoom: () => _openEngineRoom(context),
        ),

        // Chat messages
        Expanded(
          child: _messages.isEmpty && !runtime.isGenerating
              ? _WelcomeView()
              : ListView.builder(
                  controller: _scrollController,
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
                  itemCount: _messages.length + (runtime.isGenerating ? 1 : 0),
                  itemBuilder: (context, index) {
                    if (index < _messages.length) {
                      final msg = _messages[index];
                      return Padding(
                        padding: const EdgeInsets.only(bottom: 12),
                        child: _MessageBubble(
                          text: msg.text,
                          isUser: msg.isUser,
                          onLongPress: msg.isUser
                              ? null
                              : () => _showResponseMeta(
                                    modelId: runtime.loadedModelId ??
                                        runtime.selectedModelId,
                                    tps: tps,
                                    responseLength: msg.text.length,
                                  ),
                        ),
                      );
                    }
                    // Streaming AI response
                    return Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: _MessageBubble(
                        text: _streamingResponse.isEmpty
                            ? null
                            : _streamingResponse,
                        isUser: false,
                        isStreaming: true,
                      ),
                    );
                  },
                ),
        ),

        // Input area
        _InputArea(
          controller: _promptController,
          canSend: runtime.initialized && !runtime.isGenerating,
          isGenerating: runtime.isGenerating,
          onSend: _sendMessage,
          onRegenerate: runtime.lastPrompt.isNotEmpty && !runtime.isGenerating
              ? () => _sendMessage(runtime.lastPrompt)
              : null,
          onStop: runtime.isGenerating ? runtime.cancelCompletion : null,
        ),
      ],
    );
  }

  void _openEngineRoom(BuildContext ctx) {
    final runtime = ref.read(runtimeControllerProvider);
    final promptCtrl = TextEditingController(text: runtime.systemPrompt);
    double temperature = runtime.temperature;
    double topP = runtime.topP;
    double topK = runtime.topK.toDouble();
    double repeatPenalty = runtime.repeatPenalty;
    double maxOutputTokens = runtime.maxOutputTokens.toDouble();
    double contextTokens = runtime.contextWindow.toDouble();
    bool preferAccelerator = runtime.preferAccelerator;
    bool thermalGuard = runtime.thermalGuardEnabled;
    bool backgroundRun = runtime.backgroundProcessingEnabled;

    showModalBottomSheet<void>(
      context: ctx,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(24)),
      ),
      builder: (sheetCtx) {
        return StatefulBuilder(
          builder: (sheetCtx, setSheetState) {
            return SafeArea(
              child: Container(
                margin: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                decoration: BoxDecoration(
                  gradient: LuminaGradients.card,
                  borderRadius: BorderRadius.circular(24),
                  border:
                      Border.all(color: Colors.white.withValues(alpha: 0.10)),
                ),
                child: Padding(
                  padding: EdgeInsets.fromLTRB(
                    20,
                    16,
                    20,
                    16 + MediaQuery.of(sheetCtx).viewInsets.bottom,
                  ),
                  child: SingleChildScrollView(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        // Drag handle
                        Center(
                          child: Container(
                            width: 40,
                            height: 4,
                            decoration: BoxDecoration(
                              color: Colors.white.withValues(alpha: 0.2),
                              borderRadius: BorderRadius.circular(2),
                            ),
                          ),
                        ),
                        const SizedBox(height: 16),
                        Row(
                          children: [
                            const Icon(Icons.tune_rounded,
                                size: 22, color: LuminaColors.accent),
                            const SizedBox(width: 10),
                            Text('Engine Room',
                                style: Theme.of(sheetCtx).textTheme.titleLarge),
                          ],
                        ),
                        const SizedBox(height: 20),
                        TextField(
                          controller: promptCtrl,
                          maxLines: 3,
                          decoration: const InputDecoration(
                            labelText: 'System Prompt',
                            hintText: 'Define AI persona...',
                          ),
                        ),
                        const SizedBox(height: 20),
                        _SliderRow(
                          label: 'Temperature',
                          value: temperature,
                          min: 0.1,
                          max: 2.0,
                          leftHint: 'Precise',
                          rightHint: 'Creative',
                          format: (v) => v.toStringAsFixed(1),
                          onChanged: (v) =>
                              setSheetState(() => temperature = v),
                        ),
                        const SizedBox(height: 16),
                        _SliderRow(
                          label: 'Top P',
                          value: topP,
                          min: 0.1,
                          max: 1.0,
                          leftHint: 'Focused',
                          rightHint: 'Diverse',
                          format: (v) => v.toStringAsFixed(2),
                          onChanged: (v) => setSheetState(() => topP = v),
                        ),
                        const SizedBox(height: 16),
                        _SliderRow(
                          label: 'Top K',
                          value: topK,
                          min: 1,
                          max: 200,
                          divisions: 199,
                          leftHint: '1',
                          rightHint: '200',
                          format: (v) => '${v.toInt()}',
                          onChanged: (v) => setSheetState(() => topK = v),
                        ),
                        const SizedBox(height: 16),
                        _SliderRow(
                          label: 'Repeat Penalty',
                          value: repeatPenalty,
                          min: 1.0,
                          max: 2.0,
                          leftHint: 'Loose',
                          rightHint: 'Strict',
                          format: (v) => v.toStringAsFixed(2),
                          onChanged: (v) =>
                              setSheetState(() => repeatPenalty = v),
                        ),
                        const SizedBox(height: 16),
                        _SliderRow(
                          label: 'Max Output Tokens',
                          value: maxOutputTokens,
                          min: 32,
                          max: 2048,
                          divisions: 63,
                          leftHint: '32',
                          rightHint: '2048',
                          format: (v) => '${v.toInt()}',
                          onChanged: (v) =>
                              setSheetState(() => maxOutputTokens = v),
                        ),
                        const SizedBox(height: 16),
                        _SliderRow(
                          label: 'Context Window',
                          value: contextTokens,
                          min: 512,
                          max: 32768,
                          divisions: 63,
                          leftHint: '512',
                          rightHint: '32k',
                          format: (v) => '${v.toInt()}',
                          onChanged: (v) =>
                              setSheetState(() => contextTokens = v),
                        ),
                        const SizedBox(height: 12),
                        _ToggleTile(
                          icon: Icons.developer_board_rounded,
                          title: 'Prefer GPU / NPU',
                          value: preferAccelerator,
                          onChanged: (v) =>
                              setSheetState(() => preferAccelerator = v),
                        ),
                        _ToggleTile(
                          icon: Icons.thermostat_rounded,
                          title: 'Thermal Guard',
                          subtitle: 'Throttle inference when device gets hot',
                          value: thermalGuard,
                          onChanged: (v) =>
                              setSheetState(() => thermalGuard = v),
                        ),
                        _ToggleTile(
                          icon: Icons.sync_rounded,
                          title: 'Background Processing',
                          subtitle: 'Keep long prompts alive in background',
                          value: backgroundRun,
                          onChanged: (v) =>
                              setSheetState(() => backgroundRun = v),
                        ),
                        const SizedBox(height: 16),
                        SizedBox(
                          width: double.infinity,
                          height: 48,
                          child: FilledButton(
                            style: FilledButton.styleFrom(
                              backgroundColor: LuminaColors.accent,
                              foregroundColor: Colors.black,
                            ),
                            onPressed: () {
                              runtime.setSystemPrompt(promptCtrl.text);
                              runtime.setTemperature(temperature);
                              runtime.setTopP(topP);
                              runtime.setTopK(topK.toInt());
                              runtime.setRepeatPenalty(repeatPenalty);
                              runtime
                                  .setMaxOutputTokens(maxOutputTokens.toInt());
                              runtime.setContextWindow(contextTokens.toInt());
                              runtime.setPreferAccelerator(preferAccelerator);
                              runtime.setThermalGuardEnabled(thermalGuard);
                              runtime.setBackgroundProcessingEnabled(
                                  backgroundRun);
                              Navigator.of(sheetCtx).pop();
                            },
                            child: const Text('Apply Settings'),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            );
          },
        );
      },
    );
  }

  void _showResponseMeta({
    required String modelId,
    required double? tps,
    required int responseLength,
  }) {
    showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: LuminaColors.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
        title: const Row(
          children: [
            Icon(Icons.info_outline_rounded,
                size: 20, color: LuminaColors.accent),
            SizedBox(width: 8),
            Text('Response Metadata'),
          ],
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _MetaRow('Model', modelId),
            _MetaRow('Estimated T/s', tps?.toStringAsFixed(1) ?? '--'),
            _MetaRow('Output chars', '$responseLength'),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Sub-widgets
// ---------------------------------------------------------------------------

class _ModelHeader extends StatelessWidget {
  const _ModelHeader({
    required this.selectedModelId,
    required this.loadedModelId,
    required this.modelIds,
    required this.tps,
    required this.isGenerating,
    required this.onSelectModel,
    required this.onOpenEngineRoom,
  });

  final String selectedModelId;
  final String? loadedModelId;
  final List<String> modelIds;
  final double? tps;
  final bool isGenerating;
  final ValueChanged<String> onSelectModel;
  final VoidCallback onOpenEngineRoom;

  @override
  Widget build(BuildContext context) {
    final usingModelId = loadedModelId ?? selectedModelId;
    final bool isReady = loadedModelId != null;
    final bool isSelectedLoaded = loadedModelId == selectedModelId;
    final Color lightColor = !isReady
        ? LuminaColors.amber
        : (isSelectedLoaded ? LuminaColors.emerald : LuminaColors.amber);
    final String statusText = !isReady
        ? 'Loading/Not loaded'
        : (isSelectedLoaded ? 'Ready' : 'Running different model');

    return Container(
      margin: const EdgeInsets.fromLTRB(16, 8, 16, 4),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
      decoration: BoxDecoration(
        gradient: LuminaGradients.card,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: Colors.white.withValues(alpha: 0.12)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 10,
                height: 10,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: lightColor,
                  boxShadow: [
                    BoxShadow(
                      color: lightColor.withValues(
                          alpha: isGenerating ? 0.75 : 0.45),
                      blurRadius: isGenerating ? 10 : 6,
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  'Using: $usingModelId',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontWeight: FontWeight.w700,
                    fontSize: 13,
                    letterSpacing: 0.2,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
                decoration: BoxDecoration(
                  color: Colors.black.withValues(alpha: 0.26),
                  borderRadius: BorderRadius.circular(999),
                  border: Border.all(
                      color: LuminaColors.emerald.withValues(alpha: 0.4)),
                ),
                child: Text(
                  tps == null ? '-- T/s' : '${tps!.toStringAsFixed(1)} T/s',
                  style: const TextStyle(
                    color: LuminaColors.emerald,
                    fontWeight: FontWeight.w700,
                    fontSize: 13,
                  ),
                ),
              ),
              const SizedBox(width: 6),
              IconButton(
                tooltip: 'Engine Room',
                onPressed: onOpenEngineRoom,
                icon: const Icon(Icons.tune_rounded, size: 20),
                visualDensity: VisualDensity.compact,
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            statusText,
            style: TextStyle(
              fontSize: 11,
              color: lightColor,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: 8),
          _ModelSelector(
            modelId: selectedModelId,
            modelIds: modelIds,
            onSelectModel: onSelectModel,
          ),
        ],
      ),
    );
  }
}

class _WelcomeView extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(20),
              decoration: BoxDecoration(
                gradient: LuminaGradients.card,
                borderRadius: BorderRadius.circular(18),
                border: Border.all(color: Colors.white.withValues(alpha: 0.10)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 42,
                    height: 42,
                    decoration: BoxDecoration(
                      gradient: LuminaGradients.accent,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: const Icon(Icons.auto_awesome_rounded,
                        size: 22, color: Colors.white),
                  ),
                  const SizedBox(height: 16),
                  const Text(
                    'Your personal\nAI runtime',
                    style: TextStyle(
                      fontSize: 27,
                      height: 1.1,
                      fontWeight: FontWeight.w800,
                      color: Colors.white,
                    ),
                  ),
                  const SizedBox(height: 8),
                  const Text(
                    'Runs locally with your selected model.\nPrivate by default, fast by design.',
                    style: TextStyle(color: LuminaColors.white60, height: 1.5),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 14),
            const Row(
              children: [
                Expanded(
                  child: _WelcomeCard(
                    title: 'Ask MAI',
                    subtitle: 'Get instant answers.',
                    badge: 'NORMAL',
                  ),
                ),
                SizedBox(width: 10),
                Expanded(
                  child: _WelcomeCard(
                    title: 'Creative',
                    subtitle: 'Brainstorm ideas.',
                    badge: 'PREMIUM',
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),
            const Row(
              children: [
                Expanded(
                  child: _WelcomeCard(
                    title: 'Story',
                    subtitle: 'Narrative drafts.',
                    badge: 'NORMAL',
                  ),
                ),
                SizedBox(width: 10),
                Expanded(
                  child: _WelcomeCard(
                    title: 'Mentor',
                    subtitle: 'Business prompts.',
                    badge: 'PREMIUM',
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _WelcomeCard extends StatelessWidget {
  const _WelcomeCard({
    required this.title,
    required this.subtitle,
    required this.badge,
  });

  final String title;
  final String subtitle;
  final String badge;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(14),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            LuminaColors.accentLight.withValues(alpha: 0.35),
            LuminaColors.accent.withValues(alpha: 0.22),
            Colors.black.withValues(alpha: 0.30),
          ],
        ),
        border: Border.all(color: Colors.white.withValues(alpha: 0.1)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
            decoration: BoxDecoration(
              color: Colors.black.withValues(alpha: 0.26),
              borderRadius: BorderRadius.circular(999),
            ),
            child: Text(
              badge,
              style: const TextStyle(
                fontSize: 9,
                fontWeight: FontWeight.w700,
                color: Colors.white,
                letterSpacing: 0.3,
              ),
            ),
          ),
          const SizedBox(height: 10),
          Text(title,
              style:
                  const TextStyle(fontWeight: FontWeight.w700, fontSize: 15)),
          const SizedBox(height: 6),
          Text(
            subtitle,
            style: const TextStyle(color: LuminaColors.white60, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

class _ModelSelector extends StatelessWidget {
  const _ModelSelector({
    required this.modelId,
    required this.modelIds,
    required this.onSelectModel,
  });

  final String modelId;
  final List<String> modelIds;
  final ValueChanged<String> onSelectModel;

  @override
  Widget build(BuildContext context) {
    if (modelIds.isEmpty) {
      return Text(
        modelId,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 14),
      );
    }

    final current = modelIds.contains(modelId) ? modelId : modelIds.first;

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.24),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: Colors.white.withValues(alpha: 0.1)),
      ),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<String>(
          value: current,
          isExpanded: true,
          iconSize: 16,
          dropdownColor: LuminaColors.surface,
          style: const TextStyle(
            color: LuminaColors.white87,
            fontWeight: FontWeight.w700,
            fontSize: 13,
          ),
          items: modelIds
              .map(
                (id) => DropdownMenuItem<String>(
                  value: id,
                  child: Text(
                    id,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              )
              .toList(growable: false),
          onChanged: (value) {
            if (value != null) {
              onSelectModel(value);
            }
          },
        ),
      ),
    );
  }
}

class _MessageBubble extends StatelessWidget {
  const _MessageBubble({
    this.text,
    required this.isUser,
    this.isStreaming = false,
    this.onLongPress,
  });

  final String? text;
  final bool isUser;
  final bool isStreaming;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    final align = isUser ? CrossAxisAlignment.end : CrossAxisAlignment.start;
    final bubbleColor = isUser
        ? Colors.black.withValues(alpha: 0.25)
        : LuminaColors.surfaceLight;
    final borderColor = isUser
        ? LuminaColors.accent.withValues(alpha: 0.8)
        : Colors.white.withValues(alpha: 0.10);

    final displayText = text ?? '';
    final showCursor = isStreaming && (text == null || text!.isEmpty);

    return Column(
      crossAxisAlignment: align,
      children: [
        GestureDetector(
          onLongPress: onLongPress,
          onDoubleTap: isUser || displayText.isEmpty
              ? null
              : () {
                  Clipboard.setData(ClipboardData(text: displayText));
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(
                      content: Text('Copied to clipboard'),
                      duration: Duration(seconds: 1),
                    ),
                  );
                },
          child: Container(
            constraints: BoxConstraints(
              maxWidth: MediaQuery.of(context).size.width * 0.82,
            ),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              gradient: isUser ? LuminaGradients.accent : null,
              color: isUser ? null : bubbleColor,
              borderRadius: BorderRadius.only(
                topLeft: const Radius.circular(16),
                topRight: const Radius.circular(16),
                bottomLeft: Radius.circular(isUser ? 16 : 4),
                bottomRight: Radius.circular(isUser ? 4 : 16),
              ),
              border: Border.all(color: borderColor),
            ),
            child: showCursor
                ? const _TypingIndicator()
                : Text(
                    displayText,
                    style: const TextStyle(
                      color: Colors.white,
                      height: 1.4,
                      fontSize: 14.5,
                    ),
                  ),
          ),
        ),
      ],
    );
  }
}

class _TypingIndicator extends StatefulWidget {
  const _TypingIndicator();

  @override
  State<_TypingIndicator> createState() => _TypingIndicatorState();
}

class _TypingIndicatorState extends State<_TypingIndicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl;

  @override
  void initState() {
    super.initState();
    _ctrl = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    )..repeat();
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _ctrl,
      builder: (context, _) {
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: List.generate(3, (i) {
            final delay = i * 0.2;
            final t = ((_ctrl.value - delay) % 1.0).clamp(0.0, 1.0);
            final opacity = 0.3 + 0.7 * (t < 0.5 ? t * 2 : 2 - t * 2);
            return Padding(
              padding: const EdgeInsets.symmetric(horizontal: 2),
              child: Opacity(
                opacity: opacity,
                child: Container(
                  width: 8,
                  height: 8,
                  decoration: const BoxDecoration(
                    color: LuminaColors.accentLight,
                    shape: BoxShape.circle,
                  ),
                ),
              ),
            );
          }),
        );
      },
    );
  }
}

class _InputArea extends StatelessWidget {
  const _InputArea({
    required this.controller,
    required this.canSend,
    required this.isGenerating,
    required this.onSend,
    required this.onRegenerate,
    required this.onStop,
  });

  final TextEditingController controller;
  final bool canSend;
  final bool isGenerating;
  final ValueChanged<String> onSend;
  final VoidCallback? onRegenerate;
  final VoidCallback? onStop;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.36),
        border: Border(
            top: BorderSide(color: Colors.white.withValues(alpha: 0.06))),
      ),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: TextField(
                    controller: controller,
                    minLines: 1,
                    maxLines: 5,
                    textInputAction: TextInputAction.newline,
                    decoration: InputDecoration(
                      hintText: 'Type a message...',
                      hintStyle: const TextStyle(color: LuminaColors.white60),
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(24),
                        borderSide: BorderSide(
                          color: LuminaColors.accent.withValues(alpha: 0.8),
                        ),
                      ),
                      enabledBorder: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(24),
                        borderSide: BorderSide(
                          color: LuminaColors.accent.withValues(alpha: 0.8),
                        ),
                      ),
                      focusedBorder: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(24),
                        borderSide: const BorderSide(
                            color: LuminaColors.accent, width: 1.4),
                      ),
                      filled: true,
                      fillColor: Colors.black.withValues(alpha: 0.24),
                      contentPadding: const EdgeInsets.symmetric(
                          horizontal: 18, vertical: 12),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                if (isGenerating)
                  _CircleButton(
                    icon: Icons.stop_rounded,
                    color: LuminaColors.red,
                    onTap: onStop,
                  )
                else
                  _CircleButton(
                    icon: Icons.send_rounded,
                    color: LuminaColors.accent,
                    onTap: canSend ? () => onSend(controller.text) : null,
                  ),
              ],
            ),
            if (onRegenerate != null && !isGenerating) ...[
              const SizedBox(height: 6),
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton.icon(
                  onPressed: onRegenerate,
                  icon: const Icon(Icons.refresh_rounded, size: 16),
                  label:
                      const Text('Regenerate', style: TextStyle(fontSize: 12)),
                  style: TextButton.styleFrom(
                    foregroundColor: LuminaColors.white60,
                    visualDensity: VisualDensity.compact,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _CircleButton extends StatelessWidget {
  const _CircleButton({required this.icon, required this.color, this.onTap});

  final IconData icon;
  final Color color;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        width: 44,
        height: 44,
        decoration: BoxDecoration(
          gradient: onTap != null
              ? LinearGradient(colors: [color.withValues(alpha: 0.9), color])
              : null,
          color: onTap != null ? null : color.withValues(alpha: 0.3),
          shape: BoxShape.circle,
        ),
        child: Icon(icon, color: Colors.black, size: 22),
      ),
    );
  }
}

class _SliderRow extends StatelessWidget {
  const _SliderRow({
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    this.divisions,
    required this.leftHint,
    required this.rightHint,
    required this.format,
    required this.onChanged,
  });

  final String label;
  final double value;
  final double min;
  final double max;
  final int? divisions;
  final String leftHint;
  final String rightHint;
  final String Function(double) format;
  final ValueChanged<double> onChanged;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(label, style: const TextStyle(fontWeight: FontWeight.w600)),
            const Spacer(),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
              decoration: BoxDecoration(
                color: LuminaColors.accent.withValues(alpha: 0.15),
                borderRadius: BorderRadius.circular(6),
              ),
              child: Text(
                format(value),
                style: const TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                    color: LuminaColors.accentLight),
              ),
            ),
          ],
        ),
        Slider(
          min: min,
          max: max,
          divisions: divisions,
          value: value,
          onChanged: onChanged,
        ),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(leftHint,
                style:
                    const TextStyle(fontSize: 11, color: LuminaColors.white60)),
            Text(rightHint,
                style:
                    const TextStyle(fontSize: 11, color: LuminaColors.white60)),
          ],
        ),
      ],
    );
  }
}

class _ToggleTile extends StatelessWidget {
  const _ToggleTile({
    required this.icon,
    required this.title,
    this.subtitle,
    required this.value,
    required this.onChanged,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final bool value;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    return SwitchListTile(
      secondary: Icon(icon, size: 20, color: LuminaColors.white60),
      title: Text(title, style: const TextStyle(fontSize: 14)),
      subtitle: subtitle != null
          ? Text(subtitle!,
              style: const TextStyle(fontSize: 12, color: LuminaColors.white60))
          : null,
      value: value,
      onChanged: onChanged,
      contentPadding: EdgeInsets.zero,
      dense: true,
    );
  }
}

class _MetaRow extends StatelessWidget {
  const _MetaRow(this.label, this.value);
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        children: [
          Text('$label: ',
              style:
                  const TextStyle(color: LuminaColors.white60, fontSize: 13)),
          Expanded(
              child: Text(value,
                  style: const TextStyle(
                      fontWeight: FontWeight.w600, fontSize: 13))),
        ],
      ),
    );
  }
}
