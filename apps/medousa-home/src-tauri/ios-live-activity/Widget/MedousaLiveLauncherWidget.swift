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
        .supportedFamilies([.systemSmall, .accessoryCircular, .accessoryRectangular, .accessoryInline])
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
    @Environment(\.widgetFamily) private var family

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
        Group {
            switch family {
            case .accessoryCircular:
                ZStack(alignment: .bottomTrailing) {
                    MedousaMark(size: 38)
                    Image(systemName: "waveform")
                        .font(.system(size: 11, weight: .bold))
                        .padding(4)
                        .background(.background, in: Circle())
                }
                .widgetAccentable()
            case .accessoryRectangular:
                HStack(spacing: 8) {
                    MedousaMark(size: 30)
                        .widgetAccentable()
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Talk Live")
                            .font(.system(size: 14, weight: .semibold, design: .rounded))
                        Text("Start Medousa")
                            .font(.system(size: 11, weight: .medium, design: .rounded))
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }
            case .accessoryInline:
                Label("Talk Live with Medousa", systemImage: "waveform")
            default:
                VStack(spacing: 11) {
                    MedousaMark(size: 30)
                    Image(systemName: "waveform")
                        .font(.system(size: 30, weight: .medium))
                        .foregroundStyle(MedousaPalette.primarySoft)
                    Text("Talk Live")
                        .font(.system(size: 15, weight: .semibold, design: .rounded))
                        .foregroundStyle(MedousaPalette.ink)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
