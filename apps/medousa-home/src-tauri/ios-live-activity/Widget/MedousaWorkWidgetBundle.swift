import SwiftUI
import WidgetKit

@main
struct MedousaWorkWidgetBundle: WidgetBundle {
    var body: some Widget {
        MedousaHomeGlanceWidget()
        if #available(iOS 16.2, *) {
            MedousaWorkLiveActivity()
        }
    }
}
