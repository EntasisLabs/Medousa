import CarPlay
import Foundation
import UIKit

struct CarPlayLiveSnapshot: Codable {
    let owner: String
    let active: Bool
    let muted: Bool
    let phase: String
    let canControl: Bool
}

struct CarPlayLiveAction: Codable {
    let owner: String
    let action: String
}

private struct CarPlayLiveExchange: Encodable {
    let enabled: Bool
    let action: CarPlayLiveAction?
}

/// Companion controls only: never creates a second voice runtime or displays
/// transcripts, tool output, credentials, or arbitrary model text in the car.
@available(iOS 26.4, *)
@MainActor
final class MedousaCarPlayLiveController {
    static let shared = MedousaCarPlayLiveController()
    private var snapshot: CarPlayLiveSnapshot?
    private var updatedAt: TimeInterval = -.infinity
    private var pending: CarPlayLiveAction?
    private weak var interfaceController: CPInterfaceController?
    private var template: CPVoiceControlTemplate?
    private var timer: Timer?

    func exchange(json: String) -> String {
        guard let manifest = Bundle.main.object(forInfoDictionaryKey: "UIApplicationSceneManifest") as? [String: Any],
              let configurations = manifest["UISceneConfigurations"] as? [String: Any],
              configurations["CPTemplateApplicationSceneSessionRoleApplication"] != nil
        else { return "{\"enabled\":false,\"action\":null}" }
        guard let data = json.data(using: .utf8),
              let next = try? JSONDecoder().decode(CarPlayLiveSnapshot.self, from: data),
              !next.owner.isEmpty, next.owner.count <= 512
        else { return "{\"enabled\":true,\"action\":null}" }
        // Never apply a delayed tap to a different workshop/chat or a busy UI.
        let action = isFresh && next.canControl && pending?.owner == next.owner ? pending : nil
        pending = nil
        snapshot = next
        updatedAt = ProcessInfo.processInfo.systemUptime
        render()
        let reply = CarPlayLiveExchange(enabled: true, action: action)
        guard let encoded = try? JSONEncoder().encode(reply),
              let result = String(data: encoded, encoding: .utf8)
        else { return "{\"enabled\":true,\"action\":null}" }
        return result
    }

    private var isFresh: Bool { ProcessInfo.processInfo.systemUptime - updatedAt < 3 }

    func connect(_ controller: CPInterfaceController) {
        pending = nil
        interfaceController = controller
        let states = [
            state("unavailable", "Finish setup on iPhone while parked", "iphone", []),
            state("ready", "Medousa Live", "waveform", [button("mic", "Start Live", "start")]),
            state("listening", "Listening", "mic", controls()),
            state("working", "Working in Medousa", "waveform", controls()),
            state("muted", "Microphone paused", "mic.slash", controls(muted: true)),
        ]
        let template = CPVoiceControlTemplate(voiceControlStates: states)
        self.template = template
        controller.setRootTemplate(template, animated: false) { [weak self] success, _ in
            if success { self?.render() }
        }
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.render() }
        }
        MedousaLiveVoiceSessionManager.shared.setCarPlayConnected(true)
    }

    func disconnect(_ controller: CPInterfaceController) {
        guard interfaceController === controller else { return }
        timer?.invalidate()
        timer = nil
        pending = nil
        interfaceController = nil
        template = nil
        // Detaching the car does not end the user's phone voice conversation.
        MedousaLiveVoiceSessionManager.shared.setCarPlayConnected(false)
    }

    private func state(_ id: String, _ title: String, _ symbol: String, _ buttons: [CPButton]) -> CPVoiceControlState {
        let state = CPVoiceControlState(identifier: id, titleVariants: [title], image: UIImage(systemName: symbol), repeats: false)
        state.actionButtons = buttons
        return state
    }

    private func button(_ symbol: String, _ title: String, _ action: String) -> CPButton {
        let button = CPButton(image: UIImage(systemName: symbol)!) { [weak self] _ in
            guard let self, self.isFresh, let snapshot = self.snapshot,
                  snapshot.canControl, self.pending == nil else { return }
            let operation = action == "mute" && snapshot.muted ? "unmute" : action
            self.pending = CarPlayLiveAction(owner: snapshot.owner, action: operation)
            self.render()
        }
        button.title = title
        return button
    }

    private func controls(muted: Bool = false) -> [CPButton] {
        [button(muted ? "mic" : "mic.slash", muted ? "Unmute" : "Mute", "mute"),
         button("phone.down", "End Live", "stop")]
    }

    private func render() {
        guard let template else { return }
        let ready = isFresh && snapshot?.canControl == true
        let state: String
        if !isFresh || snapshot == nil || snapshot?.phase == "failed" {
            state = "unavailable"
        } else if snapshot?.active != true {
            state = snapshot?.phase == "connecting" ? "working" : "ready"
        } else if snapshot?.muted == true {
            state = "muted"
        } else if snapshot?.phase == "listening" {
            state = "listening"
        } else {
            state = "working"
        }
        for voiceState in template.voiceControlStates {
            for button in voiceState.actionButtons { button.isEnabled = ready && pending == nil }
        }
        if template.activeStateIdentifier != state {
            template.activateVoiceControlState(withIdentifier: state)
        }
    }
}

@available(iOS 26.4, *)
@objc(MedousaCarPlaySceneDelegate)
final class MedousaCarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {
    func templateApplicationScene(_ scene: CPTemplateApplicationScene, didConnect interfaceController: CPInterfaceController) {
        MedousaCarPlayLiveController.shared.connect(interfaceController)
    }

    func templateApplicationScene(_ scene: CPTemplateApplicationScene, didDisconnectInterfaceController interfaceController: CPInterfaceController) {
        MedousaCarPlayLiveController.shared.disconnect(interfaceController)
    }
}
