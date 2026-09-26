import AVFAudio
import Foundation

/// Native microphone capture and playback for the opt-in Live socket path.
/// The surrounding session manager owns AVAudioSession policy; this class only
/// owns the audio graph and converts to/from Live's mono 24 kHz PCM16 contract.
@available(iOS 17.0, *)
@MainActor
final class MedousaLiveNativeAudioEngine {
    enum AudioError: LocalizedError {
        case unavailableFormat
        case converterCreation
        case conversion
        case invalidPlaybackAudio

        var errorDescription: String? {
            switch self {
            case .unavailableFormat: return "The current microphone format is unavailable"
            case .converterCreation: return "Could not create the Live audio converter"
            case .conversion: return "Could not convert microphone audio"
            case .invalidPlaybackAudio: return "Live returned invalid playback audio"
            }
        }
    }

    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let liveFormat: AVAudioFormat
    private var converter: AVAudioConverter?
    private var capture: ((Data) -> Void)?
    private var running = false
    private var muted = false

    init?() {
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 24_000,
            channels: 1,
            interleaved: true
        ) else { return nil }
        liveFormat = format
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: liveFormat)
    }

    func start(capture: @escaping (Data) -> Void) throws {
        guard !running else { return }
        let input = engine.inputNode
        let inputFormat = input.inputFormat(forBus: 0)
        guard inputFormat.sampleRate > 0, inputFormat.channelCount > 0 else {
            throw AudioError.unavailableFormat
        }
        guard let converter = AVAudioConverter(from: inputFormat, to: liveFormat) else {
            throw AudioError.converterCreation
        }
        self.converter = converter
        self.capture = capture

        input.installTap(onBus: 0, bufferSize: 1_024, format: inputFormat) { [weak self] buffer, _ in
            // AVAudioEngine owns and may reuse this buffer after the callback;
            // convert it synchronously instead of crossing an async boundary.
            self?.consumeMicrophone(buffer)
        }
        engine.prepare()
        do {
            try engine.start()
            player.play()
            running = true
        } catch {
            input.removeTap(onBus: 0)
            self.converter = nil
            self.capture = nil
            throw error
        }
    }

    func setMuted(_ value: Bool) {
        muted = value
    }

    func play(_ bytes: Data) throws {
        guard running, MedousaLiveSocketLifecycle.validPCM(bytes),
              let buffer = AVAudioPCMBuffer(
                pcmFormat: liveFormat,
                frameCapacity: AVAudioFrameCount(bytes.count / MemoryLayout<Int16>.size)
              ), let destination = buffer.int16ChannelData?[0]
        else { throw AudioError.invalidPlaybackAudio }

        buffer.frameLength = buffer.frameCapacity
        bytes.withUnsafeBytes { source in
            guard let address = source.baseAddress else { return }
            destination.update(from: address.assumingMemoryBound(to: Int16.self), count: bytes.count / 2)
        }
        player.scheduleBuffer(buffer)
        if !player.isPlaying { player.play() }
    }

    func stop() {
        guard running || converter != nil else { return }
        engine.inputNode.removeTap(onBus: 0)
        player.stop()
        engine.stop()
        converter = nil
        capture = nil
        muted = false
        running = false
    }

    private func consumeMicrophone(_ input: AVAudioPCMBuffer) {
        guard running, !muted, let converter, let capture else { return }
        let ratio = liveFormat.sampleRate / input.format.sampleRate
        let capacity = max(1, AVAudioFrameCount(ceil(Double(input.frameLength) * ratio)) + 1)
        guard let output = AVAudioPCMBuffer(pcmFormat: liveFormat, frameCapacity: capacity) else { return }
        var supplied = false
        var conversionError: NSError?
        let status = converter.convert(to: output, error: &conversionError) { _, state in
            guard !supplied else {
                state.pointee = .noDataNow
                return nil
            }
            supplied = true
            state.pointee = .haveData
            return input
        }
        guard conversionError == nil, status != .error,
              output.frameLength > 0, let samples = output.int16ChannelData?[0] else { return }
        let byteCount = Int(output.frameLength) * MemoryLayout<Int16>.size
        let bytes = Data(bytes: samples, count: byteCount)
        guard MedousaLiveSocketLifecycle.validPCM(bytes) else { return }
        capture(bytes)
    }
}
