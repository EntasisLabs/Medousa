import AppIntents
import SwiftUI
import WidgetKit

struct MedousaQuickActionsEntry: TimelineEntry {
    let date: Date
    let configuration: MedousaQuickActionsConfiguration
    let snapshot: MedousaWidgetSnapshot?
}

struct MedousaQuickActionsProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> MedousaQuickActionsEntry {
        MedousaQuickActionsEntry(date: Date(), configuration: MedousaQuickActionsConfiguration(), snapshot: .placeholder)
    }

    func snapshot(for configuration: MedousaQuickActionsConfiguration, in context: Context) async -> MedousaQuickActionsEntry {
        MedousaQuickActionsEntry(date: Date(), configuration: configuration, snapshot: MedousaWidgetSnapshot.load())
    }

    func timeline(for configuration: MedousaQuickActionsConfiguration, in context: Context) async -> Timeline<MedousaQuickActionsEntry> {
        Timeline(entries: [MedousaQuickActionsEntry(date: Date(), configuration: configuration, snapshot: MedousaWidgetSnapshot.load())], policy: .never)
    }
}

struct MedousaQuickActionsWidget: Widget {
    let kind = "MedousaQuickActions"

    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: kind, intent: MedousaQuickActionsConfiguration.self, provider: MedousaQuickActionsProvider()) { entry in
            MedousaQuickActionsView(entry: entry)
                .medousaWidgetBackground()
        }
        .configurationDisplayName("Quick Actions")
        .description("Your favorite ways to start with Medousa.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

private struct MedousaQuickActionsView: View {
    @Environment(\.widgetFamily) private var family
    let entry: MedousaQuickActionsEntry

    private var secondary: [MedousaQuickAction] {
        let configuration = entry.configuration
        return [configuration.first, configuration.second, configuration.third, configuration.fourth, configuration.fifth]
    }

    private var livePhase: String? {
        let phase = entry.snapshot?.motionSummary?.lowercased()
        return ["listening", "muted", "connecting"].contains(phase ?? "") ? phase : nil
    }

    var body: some View {
        VStack(spacing: family == .systemLarge ? 18 : 12) {
            if let livePhase {
                activeLiveControls(phase: livePhase)
            } else {
            Group {
                if #available(iOS 18.0, *), entry.configuration.primary == .live {
                    Button(intent: OpenMedousaLiveIntent()) { primaryActionLabel }
                        .buttonStyle(.plain)
                } else {
                    Link(destination: entry.configuration.primary.url()) { primaryActionLabel }
                }
            }

            if family == .systemSmall {
                HStack(spacing: 14) {
                    actionButton(secondary[0], showLabel: false)
                    actionButton(secondary[1], showLabel: false)
                }
            } else if family == .systemLarge {
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 14), count: 3), spacing: 18) {
                    ForEach(secondary, id: \.self) { action in
                        actionButton(action, showLabel: true)
                    }
                }
                Spacer(minLength: 0)
            } else {
                HStack(spacing: 22) {
                    ForEach(secondary.prefix(3), id: \.self) { action in
                        actionButton(action, showLabel: false)
                    }
                }
            }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .padding(family == .systemSmall ? 13 : 16)
    }

    @ViewBuilder
    private func activeLiveControls(phase: String) -> some View {
        let isMuted = phase == "muted"
        VStack(spacing: 12) {
            HStack(spacing: 11) {
                MedousaMark(size: 27)
                VStack(alignment: .leading, spacing: 2) {
                    Text(phase == "connecting" ? "Starting Live…" : isMuted ? "Live paused" : "Medousa Live")
                        .font(.system(size: 15, weight: .semibold, design: .rounded))
                    Text(entry.snapshot?.workshopName ?? "Medousa")
                        .font(.system(size: 10, weight: .medium, design: .rounded))
                        .foregroundStyle(MedousaPalette.muted)
                }
                Spacer(minLength: 0)
            }

            if #available(iOS 17.0, *) {
                HStack(spacing: 12) {
                    Button(intent: SetMedousaLiveMutedIntent(muted: !isMuted)) {
                        Label(isMuted ? "Unmute" : "Mute", systemImage: isMuted ? "mic.fill" : "mic.slash.fill")
                            .frame(maxWidth: .infinity, minHeight: 44)
                            .background(Color.white.opacity(0.07))
                            .clipShape(Capsule())
                    }
                    Button(intent: StopMedousaLiveIntent()) {
                        Label("End", systemImage: "xmark")
                            .frame(maxWidth: .infinity, minHeight: 44)
                            .background(MedousaPalette.danger.opacity(0.18))
                            .clipShape(Capsule())
                    }
                    .tint(MedousaPalette.danger)
                }
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .buttonStyle(.plain)
            }
        }
        .foregroundStyle(MedousaPalette.ink)
    }

    private var primaryActionLabel: some View {
        HStack(spacing: 11) {
            MedousaMark(size: 27)
            Text(entry.configuration.primary.label == "Ask" ? "Ask Medousa" : entry.configuration.primary.label)
                .font(.system(size: 16, weight: .semibold, design: .rounded))
            Spacer(minLength: 0)
        }
        .foregroundStyle(MedousaPalette.ink)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, minHeight: 48)
        .background(Color.white.opacity(0.07))
        .clipShape(Capsule())
        .overlay(Capsule().strokeBorder(Color.white.opacity(0.055), lineWidth: 0.5))
    }

    private func actionButton(_ action: MedousaQuickAction, showLabel: Bool) -> some View {
        Group {
            if #available(iOS 18.0, *), action == .live {
                Button(intent: OpenMedousaLiveIntent()) {
                    actionButtonLabel(action, showLabel: showLabel)
                }
                .buttonStyle(.plain)
            } else {
                Link(destination: action.url()) {
                    actionButtonLabel(action, showLabel: showLabel)
                }
            }
        }
        .accessibilityLabel(action.label)
    }

    private func actionButtonLabel(_ action: MedousaQuickAction, showLabel: Bool) -> some View {
            VStack(spacing: 6) {
                Image(systemName: action.symbol)
                    .font(.system(size: 20, weight: .medium))
                    .frame(width: 48, height: 48)
                    .background(Color.white.opacity(0.055))
                    .clipShape(Circle())
                    .overlay(Circle().strokeBorder(Color.white.opacity(0.045), lineWidth: 0.5))
                if showLabel {
                    Text(action.label)
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .lineLimit(1)
                }
            }
            .foregroundStyle(MedousaPalette.ink)
    }
}
