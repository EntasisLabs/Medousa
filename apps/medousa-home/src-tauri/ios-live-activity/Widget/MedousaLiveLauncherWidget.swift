import SwiftUI
import WidgetKit

struct MedousaLiveLauncherWidget: Widget {
    let kind = "MedousaLiveLauncher"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: MedousaLiveLauncherProvider()) { entry in
            MedousaLiveLauncherView()
                .medousaWidgetBackground()
        }
        .configurationDisplayName("Live")
        .description("Start talking with Medousa.")
        .supportedFamilies([.systemSmall])
    }
}

private struct MedousaLiveLauncherEntry: TimelineEntry { let date: Date }

private struct MedousaLiveLauncherProvider: TimelineProvider {
    func placeholder(in context: Context) -> MedousaLiveLauncherEntry { .init(date: Date()) }
    func getSnapshot(in context: Context, completion: @escaping (MedousaLiveLauncherEntry) -> Void) { completion(.init(date: Date())) }
    func getTimeline(in context: Context, completion: @escaping (Timeline<MedousaLiveLauncherEntry>) -> Void) {
        completion(Timeline(entries: [.init(date: Date())], policy: .never))
    }
}

private struct MedousaLiveLauncherView: View {
    var body: some View {
        Group {
            if #available(iOS 18.0, *) {
                Button(intent: OpenMedousaLiveIntent()) { label }
                    .buttonStyle(.plain)
            } else {
                Link(destination: medousaLaunchURL(
                    host: "live",
                    query: [URLQueryItem(name: "mode", value: "new")]
                )) { label }
            }
        }
    }

    private var label: some View {
        VStack(spacing: 11) {
            MedousaMark(size: 30)
            Image(systemName: "waveform")
                .font(.system(size: 30, weight: .medium))
                .foregroundStyle(MedousaPalette.primarySoft)
            Text("Talk Live")
                .font(.system(size: 15, weight: .semibold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
