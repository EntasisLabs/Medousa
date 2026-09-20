import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit

@available(iOS 16.2, *)
private func isMedousaLive(_ state: MedousaWorkAttributes.ContentState) -> Bool {
    ["listening", "muted", "connecting"].contains(state.motionSummary?.lowercased() ?? "")
}

@available(iOS 16.2, *)
struct MedousaWorkLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MedousaWorkAttributes.self) { context in
            MedousaWorkLockScreenView(context: context)
                .activityBackgroundTint(MedousaPalette.canvas.opacity(0.94))
                .activitySystemActionForegroundColor(MedousaPalette.ink)
                .widgetURL(deepLink(for: context.state.primaryCardId))
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 8) {
                        MedousaMark(size: 28)
                        Image(systemName: MedousaPalette.iconName(for: context.state.mood))
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(MedousaPalette.accent(for: context.state.mood))
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    MedousaStatusPill(label: context.state.eyebrow, mood: context.state.mood)
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(context.state.headline)
                        .font(.system(.subheadline, design: .rounded, weight: .semibold))
                        .foregroundStyle(MedousaPalette.ink)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(spacing: 10) {
                        Text(context.attributes.workshopName)
                            .font(.system(.caption, design: .rounded))
                            .foregroundStyle(MedousaPalette.muted)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        if isMedousaLive(context.state) {
                            MedousaLiveControls(state: context.state, compact: true)
                        }
                    }
                }
            } compactLeading: {
                ZStack {
                    MedousaMark(size: 18)
                    Circle()
                        .strokeBorder(MedousaPalette.accent(for: context.state.mood), lineWidth: 1.5)
                        .frame(width: 22, height: 22)
                }
            } compactTrailing: {
                if let trailing = MedousaLiveActivityCopy.compactTrailing(
                    blockedCount: context.state.blockedCount,
                    motionSummary: context.state.motionSummary
                ) {
                    Text(trailing)
                        .font(.system(size: 12, weight: .bold, design: .rounded))
                        .foregroundStyle(
                            context.state.blockedCount > 0
                                ? MedousaPalette.warning
                                : MedousaPalette.success
                        )
                        .monospacedDigit()
                }
            } minimal: {
                MedousaMark(size: 16)
            }
            .widgetURL(deepLink(for: context.state.primaryCardId))
        }
    }

    private func deepLink(for cardId: String?) -> URL? {
        guard let cardId, !cardId.isEmpty else { return URL(string: "medousa://work") }
        return URL(string: "medousa://work/\(cardId)")
    }
}

@available(iOS 16.2, *)
private struct MedousaWorkLockScreenView: View {
    let context: ActivityViewContext<MedousaWorkAttributes>

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 8) {
                MedousaMark(size: 20)
                Text("Medousa")
                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                    .tracking(0.8)
                    .foregroundStyle(MedousaPalette.muted)
                    .lineLimit(1)
                Spacer(minLength: 6)
                MedousaStatusPill(label: context.state.eyebrow, mood: context.state.mood)
            }

            MedousaLivePulseBar(mood: context.state.mood)
                .padding(.top, 1)

            Text(context.state.headline)
                .font(.system(size: 15, weight: .semibold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
                .lineLimit(2)
                .minimumScaleFactor(0.9)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)

            if let footer = MedousaLiveActivityCopy.footerLine(
                workshopName: context.attributes.workshopName,
                motionSummary: context.state.motionSummary,
                subline: context.state.subline
            ) {
                Text(footer)
                    .font(.system(size: 11, weight: .medium, design: .rounded))
                    .foregroundStyle(MedousaPalette.subtle)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if isMedousaLive(context.state) {
                MedousaLiveControls(state: context.state, compact: false)
                    .padding(.top, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(MedousaLiveActivityCopy.lockScreenInsets)
    }

}

@available(iOS 17.0, *)
private struct MedousaLiveControls: View {
    let state: MedousaWorkAttributes.ContentState
    let compact: Bool

    private var isMuted: Bool { state.motionSummary?.lowercased() == "muted" }

    var body: some View {
        HStack(spacing: compact ? 8 : 12) {
            Button(intent: SetMedousaLiveMutedIntent(muted: !isMuted)) {
                Group {
                    if compact {
                        Image(systemName: isMuted ? "mic.fill" : "mic.slash.fill")
                    } else {
                        Label(isMuted ? "Unmute" : "Mute", systemImage: isMuted ? "mic.fill" : "mic.slash.fill")
                    }
                }
                    .font(.system(size: compact ? 13 : 12, weight: .semibold, design: .rounded))
                    .frame(minWidth: compact ? 32 : 88, minHeight: compact ? 32 : 36)
                    .background(MedousaPalette.ink.opacity(0.08))
                    .clipShape(Capsule())
            }
            .buttonStyle(.plain)
            .tint(MedousaPalette.ink)

            Button(intent: StopMedousaLiveIntent()) {
                Group {
                    if compact {
                        Image(systemName: "xmark")
                    } else {
                        Label("End", systemImage: "xmark")
                    }
                }
                    .font(.system(size: compact ? 13 : 12, weight: .semibold, design: .rounded))
                    .frame(minWidth: compact ? 32 : 72, minHeight: compact ? 32 : 36)
                    .background(MedousaPalette.danger.opacity(0.18))
                    .clipShape(Capsule())
            }
            .buttonStyle(.plain)
            .tint(MedousaPalette.danger)
        }
    }
}
