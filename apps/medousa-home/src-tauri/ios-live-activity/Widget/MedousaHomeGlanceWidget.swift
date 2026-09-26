import SwiftUI
import WidgetKit

extension View {
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
        .configurationDisplayName("Continuity")
        .description("See what Medousa is handling and what needs you.")
        .supportedFamilies([.systemMedium, .systemLarge])
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
            case .systemLarge:
                MedousaContinuityLargeView(snapshot: entry.snapshot)
            default:
                MedousaGlanceMediumView(snapshot: entry.snapshot)
            }
        }
        .widgetURL(deepLink(for: entry.snapshot.primaryCardId))
    }

    private func deepLink(for cardId: String?) -> URL? {
        guard let cardId, !cardId.isEmpty else { return URL(string: "medousa://work") }
        return URL(string: "medousa://work/\(cardId)")
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

            Spacer(minLength: 0)

            HStack(spacing: 8) {
                if let summary = MedousaLiveActivityCopy.footerLine(
                    workshopName: snapshot.workshopName,
                    motionSummary: snapshot.motionSummary,
                    subline: snapshot.subline
                ) {
                    Text(summary)
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundStyle(MedousaPalette.muted)
                        .lineLimit(1)
                }
                Spacer(minLength: 4)
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(MedousaPalette.primarySoft)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(16)
    }
}

private struct MedousaContinuityLargeView: View {
    let snapshot: MedousaWidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 9) {
                MedousaMark(size: 24)
                Text("Medousa")
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                Spacer()
                MedousaStatusPill(label: snapshot.eyebrow, mood: snapshot.mood)
            }
            MedousaLivePulseBar(mood: snapshot.mood)
            Text(snapshot.headline)
                .font(.system(size: 22, weight: .semibold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
                .lineLimit(3)
            if let subline = snapshot.subline, !subline.isEmpty {
                Text(subline)
                    .font(.system(size: 14, weight: .regular, design: .rounded))
                    .foregroundStyle(MedousaPalette.muted)
                    .lineLimit(3)
            }
            Spacer()
            HStack {
                Text(snapshot.workshopName)
                    .font(.system(size: 12, weight: .medium, design: .rounded))
                    .foregroundStyle(MedousaPalette.muted)
                Spacer()
                Label("Open", systemImage: "arrow.up.right")
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(MedousaPalette.primarySoft)
            }
        }
        .padding(18)
    }
}
