import SwiftUI
import WidgetKit

@available(iOS 18.0, *)
struct MedousaLiveControlWidget: ControlWidget {
    var body: some ControlWidgetConfiguration {
        StaticControlConfiguration(kind: "com.entasislabs.medousa-home.control.live") {
            ControlWidgetButton(action: OpenMedousaLiveIntent()) {
                Label("Medousa Live", systemImage: "waveform")
            }
        }
        .displayName("Medousa Live")
        .description("Start a new voice session with Medousa.")
    }
}
