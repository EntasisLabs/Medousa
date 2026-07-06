import SwiftUI
import WidgetKit

private extension View {
    @ViewBuilder
    func medousaWidgetBackground() -> some View {
        if #available(iOS 17.0, *) {
            containerBackground(for: .widget) {
                MedousaPalette.canvas
            }
        } else {
            background(MedousaPalette.canvas)
        }
    }
}

struct MedousaHomeGlanceWidget: Widget {
    let kind = MedousaWidgetSnapshot.widgetKind

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: MedousaGlanceProvider()) { entry in
            MedousaGlanceWidgetView(entry: entry)
                .medousaWidgetBackground()
        }
        .configurationDisplayName("Pulse")
        .description("Glance at what's running in your workshop.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

struct MedousaGlanceEntry: TimelineEntry {
    let date: Date
    let snapshot: MedousaWidgetSnapshot
}

struct MedousaGlanceProvider: TimelineProvider {
    func placeholder(in context: Context) -> MedousaGlanceEntry {
        MedousaGlanceEntry(date: Date(), snapshot: .placeholder)
    }

    func getSnapshot(in context: Context, completion: @escaping (MedousaGlanceEntry) -> Void) {
        completion(currentEntry())
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<MedousaGlanceEntry>) -> Void) {
        let entry = currentEntry()
        let refresh = Date().addingTimeInterval(15 * 60)
        completion(Timeline(entries: [entry], policy: .after(refresh)))
    }

    private func currentEntry() -> MedousaGlanceEntry {
        MedousaGlanceEntry(
            date: Date(),
            snapshot: MedousaWidgetSnapshot.load() ?? .placeholder
        )
    }
}

private struct MedousaGlanceWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: MedousaGlanceEntry

    var body: some View {
        Group {
            switch family {
            case .systemMedium:
                MedousaGlanceMediumView(snapshot: entry.snapshot)
            default:
                MedousaGlanceSmallView(snapshot: entry.snapshot)
            }
        }
        .widgetURL(deepLink(for: entry.snapshot.primaryCardId))
    }

    private func deepLink(for cardId: String?) -> URL? {
        guard let cardId, !cardId.isEmpty else { return URL(string: "medousa://work") }
        return URL(string: "medousa://work/\(cardId)")
    }
}

private struct MedousaGlanceSmallView: View {
    let snapshot: MedousaWidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 6) {
                MedousaMark(size: 18)
                Text("Medousa")
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .tracking(0.6)
                    .foregroundStyle(MedousaPalette.muted)
                Spacer(minLength: 0)
                if snapshot.blockedCount > 0 {
                    Text("\(snapshot.blockedCount)")
                        .font(.system(size: 11, weight: .bold, design: .rounded))
                        .foregroundStyle(MedousaPalette.warning)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 3)
                        .background(MedousaPalette.warning.opacity(0.14))
                        .clipShape(Capsule())
                }
            }

            MedousaStatusPill(label: snapshot.eyebrow, mood: snapshot.mood)

            Text(snapshot.headline)
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
                .lineLimit(3)
                .minimumScaleFactor(0.85)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(14)
    }
}

private struct MedousaGlanceMediumView: View {
    let snapshot: MedousaWidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 8) {
                MedousaMark(size: 20)
                Text("Medousa")
                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                    .tracking(0.8)
                    .foregroundStyle(MedousaPalette.muted)
                Spacer(minLength: 6)
                MedousaStatusPill(label: snapshot.eyebrow, mood: snapshot.mood)
            }

            MedousaLivePulseBar(mood: snapshot.mood)

            Text(snapshot.headline)
                .font(.system(size: 15, weight: .semibold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
                .lineLimit(2)
                .minimumScaleFactor(0.9)
                .frame(maxWidth: .infinity, alignment: .leading)

            if let footer = MedousaLiveActivityCopy.footerLine(
                workshopName: snapshot.workshopName,
                motionSummary: snapshot.motionSummary,
                subline: snapshot.subline
            ) {
                Text(footer)
                    .font(.system(size: 11, weight: .medium, design: .rounded))
                    .foregroundStyle(MedousaPalette.subtle)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(16)
    }
}
