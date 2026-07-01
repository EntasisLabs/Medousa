import ActivityKit
import SwiftUI
import WidgetKit

@available(iOS 16.1, *)
struct MedousaWorkLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MedousaWorkAttributes.self) { context in
            MedousaWorkLockScreenView(context: context)
                .activityBackgroundTint(Color.black.opacity(0.82))
                .widgetURL(deepLink(for: context.state.primaryCardId))
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: iconName(for: context.state.mood))
                        .foregroundStyle(accentColor(for: context.state.mood))
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if context.state.blockedCount > 0 {
                        Text("\(context.state.blockedCount)")
                            .font(.caption.bold())
                            .foregroundStyle(.orange)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(context.state.headline)
                        .font(.headline)
                        .lineLimit(1)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    Text(context.state.motionSummary ?? context.state.eyebrow)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            } compactLeading: {
                Image(systemName: iconName(for: context.state.mood))
                    .foregroundStyle(accentColor(for: context.state.mood))
            } compactTrailing: {
                Text(compactTrailing(for: context.state))
                    .font(.caption2.bold())
                    .foregroundStyle(.secondary)
            } minimal: {
                Image(systemName: iconName(for: context.state.mood))
                    .foregroundStyle(accentColor(for: context.state.mood))
            }
            .widgetURL(deepLink(for: context.state.primaryCardId))
        }
    }

    private func deepLink(for cardId: String?) -> URL? {
        guard let cardId, !cardId.isEmpty else { return URL(string: "medousa://work") }
        return URL(string: "medousa://work/\(cardId)")
    }

    private func iconName(for mood: String) -> String {
        switch mood {
        case "waiting": return "exclamationmark.circle.fill"
        case "offline": return "wifi.slash"
        case "quiet": return "moon.fill"
        default: return "bolt.fill"
        }
    }

    private func accentColor(for mood: String) -> Color {
        switch mood {
        case "waiting": return .orange
        case "offline": return .gray
        case "quiet": return .secondary
        default: return .green
        }
    }

    private func compactTrailing(for state: MedousaWorkAttributes.ContentState) -> String {
        if state.blockedCount > 0 { return "!" }
        if let summary = state.motionSummary, !summary.isEmpty {
            return String(summary.prefix(8))
        }
        return state.eyebrow.prefix(6).description
    }
}

@available(iOS 16.1, *)
private struct MedousaWorkLockScreenView: View {
    let context: ActivityViewContext<MedousaWorkAttributes>

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(context.attributes.workshopName)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Spacer()
                Text(context.state.eyebrow)
                    .font(.caption2.bold())
                    .foregroundStyle(accentColor(for: context.state.mood))
            }
            Text(context.state.headline)
                .font(.headline)
                .lineLimit(2)
            if let subline = context.state.subline, !subline.isEmpty {
                Text(subline)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            } else if let summary = context.state.motionSummary, !summary.isEmpty {
                Text(summary)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 4)
    }

    private func accentColor(for mood: String) -> Color {
        switch mood {
        case "waiting": return .orange
        case "offline": return .gray
        default: return .green
        }
    }
}
