import SwiftUI
import WidgetKit

@main
struct MedousaWorkWidgetBundle: WidgetBundle {
    var body: some Widget {
        MedousaQuickActionsWidget()
        MedousaLiveLauncherWidget()
        MedousaHomeGlanceWidget()
        if #available(iOS 16.2, *) {
            MedousaWorkLiveActivity()
        }
        if #available(iOS 18.0, *) {
            MedousaLiveControlWidget()
        }
    }
}
