import CoreImage
import Darwin
import Foundation
import HuggingFace
import MLX
import MLXLLM
import MLXLMCommon
import MLXVLM
import Tauri
import Tokenizers
import WebKit

private enum NativeInferenceError: LocalizedError {
  case invalidModel(String)
  case invalidImage
  case modelBusy
  case modelNotInstalled(String)
  case incompleteDownload(String)
  case downloadNotFound(String)

  var errorDescription: String? {
    switch self {
    case .invalidModel(let model):
      return "Unsupported local model '\(model)'. Choose a catalog model or enter a full Hugging Face MLX repository id."
    case .invalidImage:
      return "An image attachment could not be decoded for the local model."
    case .modelBusy:
      return "The local model is already loading or generating a response."
    case .modelNotInstalled(let model):
      return "\(model) is not downloaded. Open Settings → Connection → Private brain and download it before using it."
    case .incompleteDownload(let model):
      return "\(model) did not finish downloading. Open Settings → Connection → Private brain and resume the download."
    case .downloadNotFound(let jobID):
      return "Local model download '\(jobID)' was not found."
    }
  }
}

private enum JSONValue: Codable, Sendable {
  case string(String)
  case number(Double)
  case bool(Bool)
  case object([String: JSONValue])
  case array([JSONValue])
  case null

  init(from decoder: any Swift.Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode(Double.self) {
      self = .number(value)
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode([String: JSONValue].self) {
      self = .object(value)
    } else {
      self = .array(try container.decode([JSONValue].self))
    }
  }

  func encode(to encoder: any Swift.Encoder) throws {
    var container = encoder.singleValueContainer()
    switch self {
    case .string(let value): try container.encode(value)
    case .number(let value): try container.encode(value)
    case .bool(let value): try container.encode(value)
    case .object(let value): try container.encode(value)
    case .array(let value): try container.encode(value)
    case .null: try container.encodeNil()
    }
  }

  var sendableValue: any Sendable {
    switch self {
    case .string(let value): return value
    case .number(let value): return value
    case .bool(let value): return value
    case .object(let value): return value.mapValues(\.sendableValue)
    case .array(let value): return value.map(\.sendableValue)
    case .null: return NSNull()
    }
  }
}

private struct HuggingFaceDownloader: MLXLMCommon.Downloader {
  private let client: HuggingFace.HubClient

  init(client: HuggingFace.HubClient = HuggingFace.HubClient()) {
    self.client = client
  }

  func download(
    id: String,
    revision: String?,
    matching patterns: [String],
    useLatest _: Bool,
    progressHandler: @Sendable @escaping (Progress) -> Void
  ) async throws -> URL {
    guard let repoID = HuggingFace.Repo.ID(rawValue: id) else {
      throw NativeInferenceError.invalidModel(id)
    }
    return try await client.downloadSnapshot(
      of: repoID,
      revision: revision ?? "main",
      matching: patterns,
      progressHandler: { @MainActor progress in
        progressHandler(progress)
      })
  }
}

private struct HuggingFaceTokenizer: MLXLMCommon.Tokenizer {
  private let upstream: any Tokenizers.Tokenizer

  init(_ upstream: any Tokenizers.Tokenizer) {
    self.upstream = upstream
  }

  func encode(text: String, addSpecialTokens: Bool) -> [Int] {
    upstream.encode(text: text, addSpecialTokens: addSpecialTokens)
  }

  func decode(tokenIds: [Int], skipSpecialTokens: Bool) -> String {
    upstream.decode(tokens: tokenIds, skipSpecialTokens: skipSpecialTokens)
  }

  func convertTokenToId(_ token: String) -> Int? {
    upstream.convertTokenToId(token)
  }

  func convertIdToToken(_ id: Int) -> String? {
    upstream.convertIdToToken(id)
  }

  var bosToken: String? { upstream.bosToken }
  var eosToken: String? { upstream.eosToken }
  var unknownToken: String? { upstream.unknownToken }

  func applyChatTemplate(
    messages: [[String: any Sendable]],
    tools: [[String: any Sendable]]?,
    additionalContext: [String: any Sendable]?
  ) throws -> [Int] {
    do {
      return try upstream.applyChatTemplate(
        messages: messages.map(jinjaCompatibleObject),
        tools: tools?.map(jinjaCompatibleObject),
        additionalContext: additionalContext.map(jinjaCompatibleObject))
    } catch Tokenizers.TokenizerError.missingChatTemplate {
      throw MLXLMCommon.TokenizerError.missingChatTemplate
    }
  }
}

/// swift-jinja represents JSON null as a nil Optional, while Foundation's
/// JSON bridge and MLXLMCommon represent it as NSNull. Normalize recursively
/// at the tokenizer boundary so schemas and historical tool arguments remain
/// valid JSON without leaking an unsupported Foundation object into Jinja.
private func jinjaCompatibleValue(_ value: any Sendable) -> any Sendable {
  switch value {
  case is NSNull:
    return Optional<String>.none as String?
  case let object as [String: any Sendable]:
    return jinjaCompatibleObject(object)
  case let array as [any Sendable]:
    return array.map(jinjaCompatibleValue)
  default:
    return value
  }
}

private func jinjaCompatibleObject(
  _ object: [String: any Sendable]
) -> [String: any Sendable] {
  object.mapValues(jinjaCompatibleValue)
}

private struct HuggingFaceTokenizerLoader: MLXLMCommon.TokenizerLoader {
  func load(from directory: URL) async throws -> any MLXLMCommon.Tokenizer {
    let tokenizer = try await Tokenizers.AutoTokenizer.from(modelFolder: directory)
    return HuggingFaceTokenizer(tokenizer)
  }
}

private struct NativeAttachment: Codable, Sendable {
  let contentType: String
  let name: String?
  let base64: String?
  let url: String?

  func image() throws -> UserInput.Image? {
    guard contentType.lowercased().hasPrefix("image/") else { return nil }
    if let base64, let data = Data(base64Encoded: base64), let image = CIImage(data: data) {
      return .ciImage(image)
    }
    if let url, let resolved = URL(string: url) {
      return .url(resolved)
    }
    throw NativeInferenceError.invalidImage
  }
}

private struct NativeToolCall: Codable, Sendable {
  let id: String
  let name: String
  let arguments: JSONValue

  func mlxCall() -> MLXLMCommon.ToolCall {
    let values: [String: any Sendable]
    if case .object(let object) = arguments {
      values = object.mapValues(\.sendableValue)
    } else {
      values = [:]
    }
    return MLXLMCommon.ToolCall(
      function: .init(name: name, arguments: values),
      id: id)
  }
}

private struct NativeMessage: Codable, Sendable {
  let role: String
  let content: String
  let attachments: [NativeAttachment]
  let toolCalls: [NativeToolCall]
  let toolCallId: String?

  func mlxMessage() throws -> Chat.Message {
    let images = try attachments.compactMap { try $0.image() }
    switch role.lowercased() {
    case "system": return .system(content, images: images)
    case "assistant":
      return .assistant(
        content,
        images: images,
        toolCalls: toolCalls.isEmpty ? nil : toolCalls.map { $0.mlxCall() })
    case "tool": return .tool(content, id: toolCallId)
    default: return .user(content, images: images)
    }
  }
}

private struct NativeToolSpec: Codable, Sendable {
  let name: String
  let description: String?
  let schema: JSONValue?

  var mlxSpec: ToolSpec {
    let parameters: any Sendable = schema?.sendableValue
      ?? (["type": "object", "properties": [String: any Sendable]()] as [String: any Sendable])
    return [
      "type": "function",
      "function": [
        "name": name,
        "description": description ?? "",
        "parameters": parameters,
      ] as [String: any Sendable],
    ]
  }
}

private struct NativeChatOptions: Codable, Sendable {
  let temperature: Double?
  let maxTokens: UInt32?
  let topP: Double?
  let stopSequences: [String]
  let seed: UInt64?

  func generateParameters(using defaults: ModelGenerationDefaults) -> GenerateParameters {
    GenerateParameters(
      maxTokens: maxTokens.map(Int.init),
      temperature: Float(temperature ?? Double(defaults.temperature)),
      topP: Float(topP ?? Double(defaults.topP)),
      topK: defaults.topK,
      repetitionPenalty: defaults.repetitionPenalty,
      seed: seed)
  }
}

private struct ModelGenerationDefaults: Sendable {
  let temperature: Float
  let topP: Float
  let topK: Int
  let repetitionPenalty: Float?

  static let standard = ModelGenerationDefaults(
    temperature: 0.6,
    topP: 1.0,
    topK: 0,
    repetitionPenalty: nil)

  static let lfm25Agent = ModelGenerationDefaults(
    temperature: 0.1,
    topP: 1.0,
    topK: 50,
    repetitionPenalty: 1.1)
}

private struct NativeChatRequest: Codable, Sendable {
  let model: String
  let system: String?
  let messages: [NativeMessage]
  let tools: [NativeToolSpec]
  let options: NativeChatOptions
}

private struct GenerateArguments: Decodable {
  let requestId: String
  let request: NativeChatRequest
  let onEvent: Channel
}

private struct GenerateResponse: Encodable {
  let content: String
  let toolCalls: [NativeToolCall]
  let promptTokens: Int?
  let completionTokens: Int?
  let stopReason: String?
}

private struct GenerationEvent: Encodable {
  let type: String
  let text: String
}

private enum GeneratedTextKind {
  case content
  case reasoning

  var eventType: String {
    switch self {
    case .content: return "content"
    case .reasoning: return "reasoning"
    }
  }
}

private struct GeneratedTextFragment {
  let kind: GeneratedTextKind
  let text: String
}

/// Splits reasoning tags without assuming that tag boundaries match streamed
/// token boundaries. Some reasoning checkpoints put the opening `<think>` tag
/// in the prompt, so callers can start the parser inside the reasoning lane.
private struct TaggedReasoningStream {
  private static let startMarker = "<think>"
  private static let endMarker = "</think>"
  private static let markers = [startMarker, endMarker]

  private var inReasoning: Bool
  private var pending = ""

  init(startsInReasoning: Bool) {
    inReasoning = startsInReasoning
  }

  mutating func consume(_ text: String) -> [GeneratedTextFragment] {
    pending += text
    return drain(flushRemainder: false)
  }

  mutating func finish() -> [GeneratedTextFragment] {
    drain(flushRemainder: true)
  }

  private mutating func drain(flushRemainder: Bool) -> [GeneratedTextFragment] {
    var fragments: [GeneratedTextFragment] = []

    while !pending.isEmpty {
      if let marker = nextMarker() {
        append(String(pending[..<marker.range.lowerBound]), to: &fragments)
        pending = String(pending[marker.range.upperBound...])
        inReasoning = marker.startsReasoning
        continue
      }

      let retainedCharacters = flushRemainder ? 0 : trailingMarkerPrefixLength()
      let flushEnd = pending.index(pending.endIndex, offsetBy: -retainedCharacters)
      append(String(pending[..<flushEnd]), to: &fragments)
      pending = String(pending[flushEnd...])
      break
    }

    return fragments
  }

  private func nextMarker() -> (range: Range<String.Index>, startsReasoning: Bool)? {
    let start = pending.range(of: Self.startMarker).map { ($0, true) }
    let end = pending.range(of: Self.endMarker).map { ($0, false) }
    switch (start, end) {
    case (.some(let start), .some(let end)):
      return start.0.lowerBound < end.0.lowerBound ? start : end
    case (.some(let start), .none):
      return start
    case (.none, .some(let end)):
      return end
    case (.none, .none):
      return nil
    }
  }

  private func trailingMarkerPrefixLength() -> Int {
    let maximum = min(pending.count, Self.markers.map(\.count).max() ?? 0)
    guard maximum > 0 else { return 0 }
    for count in stride(from: maximum, through: 1, by: -1) {
      let suffix = String(pending.suffix(count))
      if Self.markers.contains(where: { $0.hasPrefix(suffix) }) {
        return count
      }
    }
    return 0
  }

  private func append(
    _ text: String,
    to fragments: inout [GeneratedTextFragment]
  ) {
    guard !text.isEmpty else { return }
    fragments.append(
      GeneratedTextFragment(
        kind: inReasoning ? .reasoning : .content,
        text: text))
  }
}

private struct ModelDescriptor: Sendable {
  static let recommendedModelID = "qwen3.5-2b-4bit"

  let id: String
  let displayName: String
  let family: String
  let variant: String
  let repo: String
  let sizeBytes: UInt64
  let ramEstimateMB: UInt64
  let contextLength: UInt64
  let modalities: [String]
  let license: String
  let tags: [String]
  let toolCallFormat: ToolCallFormat?

  var generationDefaults: ModelGenerationDefaults {
    id == "lfm2.5-2.6b-4bit" ? .lfm25Agent : .standard
  }

  var startsGenerationInReasoning: Bool {
    id == "lfm2.5-2.6b-4bit"
  }

  static let catalog: [ModelDescriptor] = [
    .init(
      id: "gemma-4-e2b-it-4bit",
      displayName: "Gemma 4 E2B",
      family: "gemma-4",
      variant: "E2B · 4-bit",
      repo: "mlx-community/gemma-4-e2b-it-4bit",
      sizeBytes: 3_583_086_538,
      ramEstimateMB: 2_600,
      contextLength: 32_768,
      modalities: ["text", "image"],
      license: "gemma",
      tags: ["recommended", "multimodal", "tool-use"],
      toolCallFormat: .gemma),
    .init(
      id: "qwen3.5-2b-4bit",
      displayName: "Qwen 3.5 2B",
      family: "qwen-3.5",
      variant: "2B · 4-bit",
      repo: "mlx-community/Qwen3.5-2B-4bit",
      sizeBytes: 1_749_079_731,
      ramEstimateMB: 2_400,
      contextLength: 32_768,
      modalities: ["text"],
      license: "apache-2.0",
      tags: ["fast", "tool-use"],
      toolCallFormat: .xmlFunction),
    .init(
      id: "lfm2.5-2.6b-4bit",
      displayName: "LFM2.5 2.6B",
      family: "lfm-2.5",
      variant: "2.6B · 4-bit",
      repo: "LiquidAI/LFM2.5-2.6B-MLX-4bit",
      sizeBytes: 1_580_000_000,
      ramEstimateMB: 2_500,
      contextLength: 131_072,
      modalities: ["text"],
      license: "lfm1.0",
      tags: ["reasoning", "tool-use", "agentic"],
      toolCallFormat: .lfm2),
    .init(
      id: "lfm2.5-vl-1.6b-4bit",
      displayName: "LFM2.5 VL 1.6B",
      family: "lfm-2.5",
      variant: "1.6B · 4-bit",
      repo: "mlx-community/LFM2.5-VL-1.6B-4bit",
      sizeBytes: 1_496_381_136,
      ramEstimateMB: 2_300,
      contextLength: 32_768,
      modalities: ["text", "image"],
      license: "lfm1.0",
      tags: ["fast", "multimodal", "tool-use"],
      toolCallFormat: .lfm2),
    .init(
      id: "ministral-3-3b-instruct-4bit",
      displayName: "Ministral 3 3B",
      family: "ministral-3",
      variant: "3B · 4-bit",
      repo: "mlx-community/Ministral-3-3B-Instruct-2512-4bit",
      sizeBytes: 2_050_000_000,
      ramEstimateMB: 3_600,
      contextLength: 32_768,
      modalities: ["text", "image"],
      license: "model-repository",
      tags: ["multimodal", "tool-use"],
      toolCallFormat: .json),
  ]

  static var recommended: ModelDescriptor {
    catalog.first(where: { $0.id == recommendedModelID }) ?? catalog[0]
  }

  static func resolve(_ value: String) throws -> ModelDescriptor {
    let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
    if let known = catalog.first(where: {
      $0.id.caseInsensitiveCompare(normalized) == .orderedSame
        || $0.repo.caseInsensitiveCompare(normalized) == .orderedSame
    }) {
      return known
    }
    guard normalized.contains("/"), normalized.count <= 512 else {
      throw NativeInferenceError.invalidModel(value)
    }
    return .init(
      id: normalized,
      displayName: normalized.split(separator: "/").last.map(String.init) ?? normalized,
      family: "custom-mlx",
      variant: "Custom MLX checkpoint",
      repo: normalized,
      sizeBytes: 0,
      ramEstimateMB: 0,
      contextLength: 0,
      modalities: ["text"],
      license: "model-repository",
      tags: ["custom"],
      toolCallFormat: nil)
  }
}

private struct DownloadProgress: Codable, Sendable {
  let jobId: String
  let modelId: String
  var phase: String
  var bytesDone: UInt64
  var bytesTotal: UInt64
  var percent: Float
  var currentFile: String?
  var message: String
  var error: String?
}

private struct InstalledModel: Codable, Sendable {
  let modelId: String
  let repo: String
  let localPath: String
  let installedAt: String
  let bytesOnDisk: UInt64
  let verified: Bool
  let files: [String]
}

private struct EngineStatus: Encodable, Sendable {
  let featureEnabled: Bool
  let loaded: Bool
  let phase: String
  let baseUrl: String
  let bind: String?
  let modelRepo: String?
  let modelAlias: String?
  let inferenceBackend: String?
  let worker: String?
  let message: String
}

private struct HardwareProbePayload: Encodable, Sendable {
  let totalRamMb: UInt64
  let availableRamMb: UInt64
  let cpuCores: Int
  let cpuArch: String
  let gpuBackend: String
  let freeDiskGb: UInt64
}

private struct HardwareProfilePayload: Encodable, Sendable {
  let probedAt: String
  let tier: String
  let tierLabel: String
  let probe: HardwareProbePayload
  let recommendedModelId: String
  let recommendedDisplayName: String
}

private struct NativeHardwareResponse: Encodable, Sendable {
  let profile: HardwareProfilePayload
  let engineAvailable: Bool
  let compiledBackends: [String]
  let message: String
}

private struct LoadedModel: Sendable {
  let descriptor: ModelDescriptor
  let container: ModelContainer
}

private actor NativeInferenceRuntime {
  static let shared = NativeInferenceRuntime()

  private var loadedModel: LoadedModel?
  private var jobs: [String: DownloadProgress] = [:]
  private var installed: [String: InstalledModel]
  private var phase = "cold"
  private var generationActive = false

  private let manifestURL: URL
  private let hubCacheURL: URL

  init() {
    // MLX defaults its reusable buffer cache to Metal's recommended working
    // set, which can exceed iOS's considerably lower per-process jetsam limit.
    // Keep enough cache for efficient generation without allowing temporary
    // evaluation buffers to crowd out the resident model and app UI.
    MLX.Memory.cacheLimit = 20 * 1024 * 1024
    MLX.Memory.clearCache()

    let support = FileManager.default.urls(
      for: .applicationSupportDirectory,
      in: .userDomainMask).first!
    let caches = FileManager.default.urls(
      for: .cachesDirectory,
      in: .userDomainMask).first!
    let directory = support.appendingPathComponent("Medousa/NativeInference", isDirectory: true)
    try? FileManager.default.createDirectory(
      at: directory,
      withIntermediateDirectories: true)
    manifestURL = directory.appendingPathComponent("installed-models.json")
    hubCacheURL = caches.appendingPathComponent("huggingface/hub", isDirectory: true)
    if let data = try? Data(contentsOf: manifestURL),
      let records = try? JSONDecoder().decode([InstalledModel].self, from: data)
    {
      installed = Dictionary(uniqueKeysWithValues: records.map { ($0.modelId, $0) })
    } else {
      installed = [:]
    }
  }

  func hardware() -> NativeHardwareResponse {
    let totalMB = ProcessInfo.processInfo.physicalMemory / 1_048_576
    let availableMB = UInt64(os_proc_available_memory()) / 1_048_576
    let freeDiskGB = freeDiskBytes() / 1_073_741_824
    let tier: String
    let tierLabel: String
    switch totalMB {
    case ..<6_000: (tier, tierLabel) = ("A", "Minimal")
    case ..<8_000: (tier, tierLabel) = ("B", "Everyday")
    case ..<12_000: (tier, tierLabel) = ("C", "Comfortable")
    case ..<16_000: (tier, tierLabel) = ("D", "Enthusiast")
    default: (tier, tierLabel) = ("E", "Workstation")
    }
    return NativeHardwareResponse(
      profile: HardwareProfilePayload(
        probedAt: iso8601Now(),
        tier: tier,
        tierLabel: tierLabel,
        probe: HardwareProbePayload(
          totalRamMb: totalMB,
          availableRamMb: availableMB,
          cpuCores: ProcessInfo.processInfo.processorCount,
          cpuArch: "arm64",
          gpuBackend: "metal",
          freeDiskGb: freeDiskGB),
        recommendedModelId: ModelDescriptor.recommended.id,
        recommendedDisplayName: ModelDescriptor.recommended.displayName),
      engineAvailable: true,
      compiledBackends: ["mlx-swift", "metal"],
      message: "Private on-device inference is available.")
  }

  func catalog() -> NativeCatalogResponse {
    NativeCatalogResponse(
      tier: hardwareTier(),
      tierLabel: hardwareTierLabel(),
      familyDefault: "gemma-4",
      recommendedModelId: ModelDescriptor.recommended.id,
      models: ModelDescriptor.catalog.map(CatalogEntry.init))
  }

  func models() -> NativeModelsResponse {
    reconcileInstalledModels()
    return NativeModelsResponse(
      installed: installed.values.sorted { $0.modelId < $1.modelId },
      activeDownloads: jobs.values
        .filter { $0.phase != "ready" && $0.phase != "failed" }
        .sorted { $0.modelId < $1.modelId })
  }

  func startDownload(modelID: String) throws -> DownloadProgress {
    let descriptor = try ModelDescriptor.resolve(modelID)
    reconcileInstalledModels()
    if let record = installed[descriptor.id], record.verified,
      isCompleteModelSnapshot(URL(fileURLWithPath: record.localPath))
    {
      return DownloadProgress(
        jobId: "installed",
        modelId: descriptor.id,
        phase: "ready",
        bytesDone: record.bytesOnDisk,
        bytesTotal: record.bytesOnDisk,
        percent: 100,
        currentFile: nil,
        message: "Already installed",
        error: nil)
    }
    if let active = jobs.values.first(where: {
      $0.modelId == descriptor.id && $0.phase != "ready" && $0.phase != "failed"
    }) {
      return active
    }
    guard !generationActive,
      !jobs.values.contains(where: { $0.phase != "ready" && $0.phase != "failed" })
    else {
      throw NativeInferenceError.modelBusy
    }
    let jobID = UUID().uuidString
    let cachedBytes = installed[descriptor.id]?.bytesOnDisk ?? 0
    let initial = DownloadProgress(
      jobId: jobID,
      modelId: descriptor.id,
      phase: "queued",
      bytesDone: cachedBytes,
      bytesTotal: descriptor.sizeBytes,
      percent: 0,
      currentFile: nil,
      message: "Preparing download…",
      error: nil)
    jobs[jobID] = initial
    phase = "loading"
    Task { await self.runDownload(jobID: jobID, descriptor: descriptor) }
    return initial
  }

  func downloadStatus(jobID: String) throws -> DownloadProgress {
    guard let progress = jobs[jobID] else {
      throw NativeInferenceError.downloadNotFound(jobID)
    }
    return progress
  }

  func load(modelID: String?) async throws -> EngineStatus {
    guard !generationActive, phase != "loading" else {
      throw NativeInferenceError.modelBusy
    }
    let descriptor = try ModelDescriptor.resolve(modelID ?? ModelDescriptor.recommended.id)
    _ = try await loadContainer(descriptor: descriptor)
    return status()
  }

  func status() -> EngineStatus {
    EngineStatus(
      featureEnabled: true,
      loaded: loadedModel != nil,
      phase: phase,
      baseUrl: "native://mlx",
      bind: nil,
      modelRepo: loadedModel?.descriptor.repo,
      modelAlias: loadedModel?.descriptor.id,
      inferenceBackend: "mlx-swift",
      worker: nil,
      message: statusMessage())
  }

  func unload() throws -> EngineStatus {
    guard !generationActive, phase != "loading" else { throw NativeInferenceError.modelBusy }
    loadedModel = nil
    MLX.Memory.clearCache()
    phase = "cold"
    return status()
  }

  func remove(modelID: String) throws {
    guard !generationActive, phase != "loading" else { throw NativeInferenceError.modelBusy }
    let descriptor = try ModelDescriptor.resolve(modelID)
    reconcileInstalledModels()
    if loadedModel?.descriptor.id == descriptor.id {
      loadedModel = nil
      MLX.Memory.clearCache()
      phase = "cold"
    }
    let record = installed.removeValue(forKey: descriptor.id)
    let repositoryRoot = hubCacheURL.appendingPathComponent(
      huggingFaceCacheDirectoryName(for: descriptor.repo),
      isDirectory: true)
    if FileManager.default.fileExists(atPath: repositoryRoot.path) {
      try FileManager.default.removeItem(at: repositoryRoot)
    } else if let record {
      let recordedPath = URL(fileURLWithPath: record.localPath)
      let recordedRoot = huggingFaceRepositoryRoot(for: recordedPath) ?? recordedPath
      if FileManager.default.fileExists(atPath: recordedRoot.path) {
        try FileManager.default.removeItem(at: recordedRoot)
      }
    }
    let lockRoot = hubCacheURL.appendingPathComponent(".locks", isDirectory: true)
      .appendingPathComponent(huggingFaceCacheDirectoryName(for: descriptor.repo), isDirectory: true)
    if FileManager.default.fileExists(atPath: lockRoot.path) {
      try? FileManager.default.removeItem(at: lockRoot)
    }
    persistManifest()
  }

  func generate(_ request: NativeChatRequest, onEvent: Channel) async throws -> GenerateResponse {
    guard !generationActive, phase != "loading" else { throw NativeInferenceError.modelBusy }
    generationActive = true
    defer {
      MLX.Memory.clearCache()
      generationActive = false
      phase = loadedModel == nil ? "cold" : "ready"
    }
    let descriptor = try ModelDescriptor.resolve(request.model)
    let model = try await loadContainer(descriptor: descriptor)
    phase = "busy"
    MLX.Memory.clearCache()

    let messages = try request.messages.map { try $0.mlxMessage() }
    let tools = request.tools.map(\.mlxSpec)
    let generateParameters = request.options.generateParameters(
      using: descriptor.generationDefaults)
    let session = ChatSession(
      model,
      instructions: request.system,
      generateParameters: generateParameters,
      tools: tools.isEmpty ? nil : tools)

    var content = ""
    var reasoningStream: TaggedReasoningStream?
    if descriptor.startsGenerationInReasoning {
      reasoningStream = TaggedReasoningStream(startsInReasoning: true)
    }
    var toolCalls: [NativeToolCall] = []
    var promptTokens: Int?
    var completionTokens: Int?
    var stopReason: String?

    func emit(_ fragments: [GeneratedTextFragment]) throws {
      for fragment in fragments {
        switch fragment.kind {
        case .content:
          content += fragment.text
        case .reasoning:
          break
        }
        try onEvent.send(
          GenerationEvent(type: fragment.kind.eventType, text: fragment.text))
      }
    }

    for try await event in session.streamDetails(to: messages) {
      try Task.checkCancellation()
      switch event {
      case .chunk(let text):
        if let fragments = reasoningStream?.consume(text) {
          try emit(fragments)
        } else {
          try emit([GeneratedTextFragment(kind: .content, text: text)])
        }
      case .toolCall(let call):
        let arguments = JSONValue.object(call.function.arguments.mapValues(JSONValue.init))
        toolCalls.append(
          NativeToolCall(
            id: call.id ?? UUID().uuidString,
            name: call.function.name,
            arguments: arguments))
      case .info(let info):
        promptTokens = info.promptTokenCount
        completionTokens = info.generationTokenCount
        switch info.stopReason {
        case .stop: stopReason = toolCalls.isEmpty ? "stop" : "tool_calls"
        case .length: stopReason = "length"
        case .cancelled: stopReason = "cancelled"
        }
      }
    }
    if let fragments = reasoningStream?.finish() {
      try emit(fragments)
    }
    return GenerateResponse(
      content: content,
      toolCalls: toolCalls,
      promptTokens: promptTokens,
      completionTokens: completionTokens,
      stopReason: stopReason)
  }

  private func runDownload(jobID: String, descriptor: ModelDescriptor) async {
    do {
      let directory = try await HuggingFaceDownloader().download(
        id: descriptor.repo,
        revision: nil,
        matching: ["*.safetensors", "*.json", "*.jinja", "*.model", "*.txt"],
        useLatest: true,
        progressHandler: { progress in
          Task {
            await self.updateProgress(jobID: jobID, descriptor: descriptor, progress: progress)
          }
        })
      let snapshot = isCompleteModelSnapshot(directory)
        ? directory
        : bestSnapshotDirectory(
          in: huggingFaceRepositoryRoot(for: directory)
            ?? hubCacheURL.appendingPathComponent(
              huggingFaceCacheDirectoryName(for: descriptor.repo),
              isDirectory: true))
      guard let snapshot, isCompleteModelSnapshot(snapshot) else {
        reconcileInstalledModels()
        throw NativeInferenceError.incompleteDownload(descriptor.displayName)
      }
      let repositoryRoot = huggingFaceRepositoryRoot(for: snapshot)
        ?? hubCacheURL.appendingPathComponent(
          huggingFaceCacheDirectoryName(for: descriptor.repo),
          isDirectory: true)
      let bytes = allocatedBytes(at: repositoryRoot)
      installed[descriptor.id] = InstalledModel(
        modelId: descriptor.id,
        repo: descriptor.repo,
        localPath: snapshot.path,
        installedAt: installed[descriptor.id]?.installedAt ?? iso8601Now(),
        bytesOnDisk: bytes,
        verified: true,
        files: [])
      persistManifest()
      jobs[jobID] = DownloadProgress(
        jobId: jobID,
        modelId: descriptor.id,
        phase: "ready",
        bytesDone: bytes,
        bytesTotal: bytes,
        percent: 100,
        currentFile: nil,
        message: "Download complete",
        error: nil)
      phase = loadedModel == nil ? "cold" : "ready"
    } catch {
      reconcileInstalledModels()
      jobs[jobID] = DownloadProgress(
        jobId: jobID,
        modelId: descriptor.id,
        phase: "failed",
        bytesDone: jobs[jobID]?.bytesDone ?? 0,
        bytesTotal: jobs[jobID]?.bytesTotal ?? descriptor.sizeBytes,
        percent: jobs[jobID]?.percent ?? 0,
        currentFile: nil,
        message: "Download failed",
        error: error.localizedDescription)
      if loadedModel == nil { phase = "failed" }
    }
  }

  private func loadContainer(descriptor: ModelDescriptor) async throws -> ModelContainer {
    if let loadedModel, loadedModel.descriptor.id == descriptor.id {
      return loadedModel.container
    }
    reconcileInstalledModels()
    guard let installedRecord = installed[descriptor.id], installedRecord.verified,
      isCompleteModelSnapshot(URL(fileURLWithPath: installedRecord.localPath))
    else {
      throw NativeInferenceError.modelNotInstalled(descriptor.displayName)
    }
    // Never overlap resident model containers while switching checkpoints.
    // Even two small quantized models can cross an iPhone's jetsam ceiling.
    loadedModel = nil
    MLX.Memory.clearCache()
    phase = "loading"
    // Loading an installed model must stay offline. Resolving the repository
    // ID here can silently resume a multi-gigabyte download while chat looks
    // like it is waiting for the first token.
    let configuration = ModelConfiguration(
      directory: URL(fileURLWithPath: installedRecord.localPath),
      toolCallFormat: descriptor.toolCallFormat)
    let progressHandler: @Sendable (Progress) -> Void = { _ in }
    let downloader = HuggingFaceDownloader()
    let tokenizerLoader = HuggingFaceTokenizerLoader()
    do {
      let container: ModelContainer
      if descriptor.modalities.contains("image") {
        do {
          container = try await VLMModelFactory.shared.loadContainer(
            from: downloader,
            using: tokenizerLoader,
            configuration: configuration,
            progressHandler: progressHandler)
        } catch {
          container = try await LLMModelFactory.shared.loadContainer(
            from: downloader,
            using: tokenizerLoader,
            configuration: configuration,
            progressHandler: progressHandler)
        }
      } else {
        do {
          container = try await LLMModelFactory.shared.loadContainer(
            from: downloader,
            using: tokenizerLoader,
            configuration: configuration,
            progressHandler: progressHandler)
        } catch {
          // Custom repositories may be multimodal even when Medousa has no
          // catalog metadata for them yet.
          container = try await VLMModelFactory.shared.loadContainer(
            from: downloader,
            using: tokenizerLoader,
            configuration: configuration,
            progressHandler: progressHandler)
        }
      }
      loadedModel = LoadedModel(descriptor: descriptor, container: container)
      // Loading and quantizing weights leaves large temporary buffers eligible
      // for reuse. On iOS those buffers need to be returned immediately so the
      // first prompt has headroom beneath the jetsam limit.
      MLX.Memory.clearCache()
      phase = "ready"
      return container
    } catch {
      loadedModel = nil
      MLX.Memory.clearCache()
      phase = "failed"
      throw error
    }
  }

  /// The Hugging Face cache survives an Xcode reinstall, while iOS assigns the
  /// app a new container UUID. Persisted absolute snapshot paths therefore go
  /// stale even though the model blobs are still present. Rebuild the manifest
  /// from the current cache on every boundary where callers depend on it.
  private func reconcileInstalledModels() {
    let previous = installed
    var candidates: [(modelID: String, repo: String, prior: InstalledModel?)] = []
    var seenRepositories = Set<String>()

    for record in previous.values.sorted(by: { $0.modelId < $1.modelId }) {
      candidates.append((record.modelId, record.repo, record))
      seenRepositories.insert(record.repo.lowercased())
    }
    for descriptor in ModelDescriptor.catalog
    where !seenRepositories.contains(descriptor.repo.lowercased()) {
      candidates.append((descriptor.id, descriptor.repo, nil))
      seenRepositories.insert(descriptor.repo.lowercased())
    }

    if let cacheDirectories = try? FileManager.default.contentsOfDirectory(
      at: hubCacheURL,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles])
    {
      for directory in cacheDirectories {
        guard let repo = huggingFaceRepositoryID(fromCacheDirectoryName: directory.lastPathComponent),
          !seenRepositories.contains(repo.lowercased())
        else { continue }
        let descriptor = try? ModelDescriptor.resolve(repo)
        candidates.append((descriptor?.id ?? repo, repo, nil))
        seenRepositories.insert(repo.lowercased())
      }
    }

    var repaired: [String: InstalledModel] = [:]
    for candidate in candidates {
      if let record = cachedRecord(
        modelID: candidate.modelID,
        repo: candidate.repo,
        prior: candidate.prior)
      {
        repaired[candidate.modelID] = record
      }
    }
    installed = repaired
    persistManifest()
  }

  private func cachedRecord(
    modelID: String,
    repo: String,
    prior: InstalledModel?
  ) -> InstalledModel? {
    let currentRepositoryRoot = hubCacheURL.appendingPathComponent(
      huggingFaceCacheDirectoryName(for: repo),
      isDirectory: true)
    let priorPath = prior.map { URL(fileURLWithPath: $0.localPath) }
    let repositoryRoot: URL
    if FileManager.default.fileExists(atPath: currentRepositoryRoot.path) {
      repositoryRoot = currentRepositoryRoot
    } else if let priorPath,
      FileManager.default.fileExists(atPath: priorPath.path)
    {
      repositoryRoot = huggingFaceRepositoryRoot(for: priorPath) ?? priorPath
    } else {
      return nil
    }

    let bytes = allocatedBytes(at: repositoryRoot)
    guard bytes > 0 else { return nil }
    let snapshot: URL?
    if let priorPath, isCompleteModelSnapshot(priorPath),
      huggingFaceRepositoryRoot(for: priorPath)?.standardizedFileURL == repositoryRoot.standardizedFileURL
    {
      snapshot = priorPath
    } else {
      snapshot = bestSnapshotDirectory(in: repositoryRoot)
    }
    let verified = snapshot.map(isCompleteModelSnapshot) ?? false
    return InstalledModel(
      modelId: modelID,
      repo: repo,
      localPath: (snapshot ?? repositoryRoot).path,
      installedAt: prior?.installedAt ?? iso8601Now(),
      bytesOnDisk: bytes,
      verified: verified,
      files: [])
  }

  private func updateProgress(
    jobID: String,
    descriptor: ModelDescriptor,
    progress: Progress
  ) {
    let total = progress.totalUnitCount > 0
      ? UInt64(progress.totalUnitCount)
      : descriptor.sizeBytes
    let completed = progress.completedUnitCount > 0
      ? UInt64(progress.completedUnitCount)
      : UInt64(Double(total) * progress.fractionCompleted)
    jobs[jobID] = DownloadProgress(
      jobId: jobID,
      modelId: descriptor.id,
      phase: "downloading",
      bytesDone: completed,
      bytesTotal: total,
      percent: Float(progress.fractionCompleted * 100),
      currentFile: nil,
      message: "Downloading \(descriptor.displayName)…",
      error: nil)
  }

  private func persistManifest() {
    let records = installed.values.sorted { $0.modelId < $1.modelId }
    if let data = try? JSONEncoder().encode(records) {
      try? data.write(to: manifestURL, options: .atomic)
    }
  }

  private func statusMessage() -> String {
    switch phase {
    case "loading": return "Loading the local model…"
    case "busy": return "Generating privately on this device"
    case "ready": return "Local model ready"
    case "failed": return "The local model could not be loaded"
    default: return "No local model loaded"
    }
  }

  private func hardwareTier() -> String {
    let totalMB = ProcessInfo.processInfo.physicalMemory / 1_048_576
    switch totalMB {
    case ..<6_000: return "A"
    case ..<8_000: return "B"
    case ..<12_000: return "C"
    case ..<16_000: return "D"
    default: return "E"
    }
  }

  private func hardwareTierLabel() -> String {
    switch hardwareTier() {
    case "A": return "Minimal"
    case "B": return "Everyday"
    case "C": return "Comfortable"
    case "D": return "Enthusiast"
    default: return "Workstation"
    }
  }
}

private struct CatalogEntry: Encodable {
  let id: String
  let displayName: String
  let family: String
  let variant: String
  let tierMin = "A"
  let tierMax = "E"
  let tierRecommended: Bool
  let format = "mlx"
  let source = "huggingface"
  let repo: String
  let engine = "mlx-swift"
  let engineArgs: [String: String] = [:]
  let fallback: String? = nil
  let sizeBytes: UInt64
  let contextLength: UInt64
  let ramEstimateMb: UInt64
  let modalities: [String]
  let license: String
  let tags: [String]

  init(_ descriptor: ModelDescriptor) {
    id = descriptor.id
    displayName = descriptor.displayName
    family = descriptor.family
    variant = descriptor.variant
    tierRecommended = descriptor.id == ModelDescriptor.recommended.id
    repo = descriptor.repo
    sizeBytes = descriptor.sizeBytes
    contextLength = descriptor.contextLength
    ramEstimateMb = descriptor.ramEstimateMB
    modalities = descriptor.modalities
    license = descriptor.license
    tags = descriptor.tags
  }
}

private struct NativeCatalogResponse: Encodable {
  let tier: String
  let tierLabel: String
  let familyDefault: String
  let recommendedModelId: String
  let models: [CatalogEntry]
}

private struct NativeModelsResponse: Encodable {
  let installed: [InstalledModel]
  let activeDownloads: [DownloadProgress]
}

private extension JSONValue {
  init(_ value: MLXLMCommon.JSONValue) {
    switch value {
    case .string(let value): self = .string(value)
    case .int(let value): self = .number(Double(value))
    case .double(let value): self = .number(value)
    case .bool(let value): self = .bool(value)
    case .array(let value): self = .array(value.map(JSONValue.init))
    case .object(let value): self = .object(value.mapValues(JSONValue.init))
    case .null: self = .null
    }
  }
}

private func iso8601Now() -> String {
  ISO8601DateFormatter().string(from: Date())
}

private func freeDiskBytes() -> UInt64 {
  let home = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
  let values = try? home.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
  return UInt64(max(0, values?.volumeAvailableCapacityForImportantUsage ?? 0))
}

private func allocatedBytes(at root: URL) -> UInt64 {
  let keys: [URLResourceKey] = [
    .isRegularFileKey,
    .isSymbolicLinkKey,
    .totalFileAllocatedSizeKey,
    .fileAllocatedSizeKey,
  ]
  guard let enumerator = FileManager.default.enumerator(
    at: root,
    includingPropertiesForKeys: keys,
    options: [.skipsHiddenFiles])
  else { return 0 }
  var total: UInt64 = 0
  for case let file as URL in enumerator {
    guard let values = try? file.resourceValues(forKeys: Set(keys)),
      values.isRegularFile == true,
      values.isSymbolicLink != true
    else { continue }
    total += UInt64(values.totalFileAllocatedSize ?? values.fileAllocatedSize ?? 0)
  }
  return total
}

private func huggingFaceCacheDirectoryName(for repo: String) -> String {
  "models--" + repo.replacingOccurrences(of: "/", with: "--")
}

private func huggingFaceRepositoryID(fromCacheDirectoryName name: String) -> String? {
  guard name.hasPrefix("models--") else { return nil }
  let encoded = String(name.dropFirst("models--".count))
  guard let separator = encoded.range(of: "--") else { return nil }
  let owner = encoded[..<separator.lowerBound]
  let model = encoded[separator.upperBound...]
  guard !owner.isEmpty, !model.isEmpty else { return nil }
  return "\(owner)/\(model)"
}

private func bestSnapshotDirectory(in repositoryRoot: URL) -> URL? {
  let snapshots = repositoryRoot.appendingPathComponent("snapshots", isDirectory: true)
  guard let directories = try? FileManager.default.contentsOfDirectory(
    at: snapshots,
    includingPropertiesForKeys: [.contentModificationDateKey, .isDirectoryKey],
    options: [.skipsHiddenFiles]),
    !directories.isEmpty
  else { return nil }

  let sorted = directories.sorted { left, right in
    let leftDate = (try? left.resourceValues(forKeys: [.contentModificationDateKey]))?
      .contentModificationDate ?? .distantPast
    let rightDate = (try? right.resourceValues(forKeys: [.contentModificationDateKey]))?
      .contentModificationDate ?? .distantPast
    return leftDate > rightDate
  }
  let mainReference = repositoryRoot.appendingPathComponent("refs/main")
  let referencedSnapshot: URL? = (try? Data(contentsOf: mainReference))
    .flatMap { String(data: $0, encoding: .utf8) }
    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
    .flatMap { revision in
      guard !revision.isEmpty else { return nil }
      let candidate = snapshots.appendingPathComponent(revision, isDirectory: true)
      return FileManager.default.fileExists(atPath: candidate.path) ? candidate : nil
    }

  if let referencedSnapshot, isCompleteModelSnapshot(referencedSnapshot) {
    return referencedSnapshot
  }
  if let complete = sorted.first(where: isCompleteModelSnapshot) {
    return complete
  }
  return referencedSnapshot ?? sorted.first
}

private func isCompleteModelSnapshot(_ directory: URL) -> Bool {
  let fileManager = FileManager.default
  guard fileManager.fileExists(
    atPath: directory.appendingPathComponent("config.json").path)
  else { return false }
  guard let files = try? fileManager.contentsOfDirectory(
    at: directory,
    includingPropertiesForKeys: nil,
    options: [.skipsHiddenFiles])
  else { return false }
  let hasWeights = files.contains {
    $0.pathExtension.caseInsensitiveCompare("safetensors") == .orderedSame
      && fileManager.fileExists(atPath: $0.path)
  }
  let tokenizerFiles = [
    "tokenizer.json",
    "tokenizer.model",
    "sentencepiece.bpe.model",
    "vocab.json",
  ]
  let hasTokenizer = tokenizerFiles.contains {
    fileManager.fileExists(atPath: directory.appendingPathComponent($0).path)
  }
  return hasWeights && hasTokenizer
}

private func huggingFaceRepositoryRoot(for snapshot: URL) -> URL? {
  var current = snapshot.standardizedFileURL
  while current.pathComponents.count > 2 {
    if current.lastPathComponent == "snapshots" {
      let root = current.deletingLastPathComponent()
      return root.lastPathComponent.hasPrefix("models--") ? root : nil
    }
    current.deleteLastPathComponent()
  }
  return nil
}

@objc(NativeInferencePlugin)
public final class NativeInferencePlugin: Plugin {
  private let taskLock = NSLock()
  private var generationTasks: [String: Task<Void, Never>] = [:]

  @objc public func hardware(_ invoke: Invoke) {
    Task {
      let payload = await NativeInferenceRuntime.shared.hardware()
      invoke.resolve(payload)
    }
  }

  @objc public func catalog(_ invoke: Invoke) {
    Task { invoke.resolve(await NativeInferenceRuntime.shared.catalog()) }
  }

  @objc public func models(_ invoke: Invoke) {
    Task { invoke.resolve(await NativeInferenceRuntime.shared.models()) }
  }

  @objc public func startDownload(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(ModelIDArguments.self)
      Task {
        do { invoke.resolve(try await NativeInferenceRuntime.shared.startDownload(modelID: args.modelId)) }
        catch { invoke.reject(error.localizedDescription) }
      }
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  @objc public func downloadStatus(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(JobIDArguments.self)
      Task {
        do { invoke.resolve(try await NativeInferenceRuntime.shared.downloadStatus(jobID: args.jobId)) }
        catch { invoke.reject(error.localizedDescription) }
      }
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  @objc public func loadModel(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(OptionalModelIDArguments.self)
      Task {
        do { invoke.resolve(try await NativeInferenceRuntime.shared.load(modelID: args.modelId)) }
        catch { invoke.reject(error.localizedDescription) }
      }
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  @objc public func status(_ invoke: Invoke) {
    Task { invoke.resolve(await NativeInferenceRuntime.shared.status()) }
  }

  @objc public func unload(_ invoke: Invoke) {
    Task {
      do { invoke.resolve(try await NativeInferenceRuntime.shared.unload()) }
      catch { invoke.reject(error.localizedDescription) }
    }
  }

  @objc public func removeModel(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(ModelIDArguments.self)
      Task {
        do {
          try await NativeInferenceRuntime.shared.remove(modelID: args.modelId)
          invoke.resolve(["removed": true])
        } catch {
          invoke.reject(error.localizedDescription)
        }
      }
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  @objc public func generate(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(GenerateArguments.self)
      let requestID = args.requestId
      let task = Task { [weak self] in
        defer { self?.removeGenerationTask(requestID) }
        do {
          let response = try await NativeInferenceRuntime.shared.generate(
            args.request,
            onEvent: args.onEvent)
          invoke.resolve(response)
        } catch is CancellationError {
          invoke.reject("Local generation was cancelled")
        } catch {
          invoke.reject(error.localizedDescription)
        }
      }
      taskLock.withLock { generationTasks[requestID] = task }
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  @objc public func cancel(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(RequestIDArguments.self)
      let cancelled = taskLock.withLock { generationTasks[args.requestId]?.cancel() != nil }
      invoke.resolve(["cancelled": cancelled])
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }

  private func removeGenerationTask(_ requestID: String) {
    _ = taskLock.withLock { generationTasks.removeValue(forKey: requestID) }
  }
}

private struct ModelIDArguments: Decodable { let modelId: String }
private struct OptionalModelIDArguments: Decodable { let modelId: String? }
private struct JobIDArguments: Decodable { let jobId: String }
private struct RequestIDArguments: Decodable { let requestId: String }

@_cdecl("init_plugin_native_inference")
public func initPlugin() -> Plugin {
  NativeInferencePlugin()
}
