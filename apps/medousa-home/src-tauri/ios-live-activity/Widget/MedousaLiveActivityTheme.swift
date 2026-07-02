import SwiftUI

/// Visual tokens aligned with `medousa-theme.ts` (Medousa Home).
enum MedousaPalette {
    static let canvas = Color(red: 16 / 255, green: 16 / 255, blue: 24 / 255)
    static let ink = Color(red: 232 / 255, green: 232 / 255, blue: 244 / 255)
    static let muted = Color(red: 118 / 255, green: 118 / 255, blue: 138 / 255)
    static let subtle = Color(red: 82 / 255, green: 82 / 255, blue: 98 / 255)
    static let primary = Color(red: 131 / 255, green: 68 / 255, blue: 245 / 255)
    static let primarySoft = Color(red: 153 / 255, green: 104 / 255, blue: 255 / 255)
    static let success = Color(red: 169 / 255, green: 219 / 255, blue: 92 / 255)
    static let warning = Color(red: 251 / 255, green: 191 / 255, blue: 36 / 255)
    static let danger = Color(red: 251 / 255, green: 113 / 255, blue: 133 / 255)

    static func accent(for mood: String) -> Color {
        switch mood {
        case "waiting": return warning
        case "offline": return muted
        case "quiet": return subtle
        default: return success
        }
    }

    static func pillBackground(for mood: String) -> Color {
        accent(for: mood).opacity(0.14)
    }

    static func pillForeground(for mood: String) -> Color {
        switch mood {
        case "waiting": return warning
        case "offline": return muted
        case "quiet": return Color(red: 188 / 255, green: 188 / 255, blue: 204 / 255)
        default: return success
        }
    }

    static func iconName(for mood: String) -> String {
        switch mood {
        case "waiting": return "hand.raised.fill"
        case "offline": return "wifi.slash"
        case "quiet": return "moon.stars.fill"
        default: return "bolt.fill"
        }
    }
}

enum MedousaLiveActivityCopy {
    static let lockScreenInsets = EdgeInsets(top: 14, leading: 18, bottom: 16, trailing: 18)

    static func footerLine(workshopName: String, motionSummary: String?, subline: String?) -> String? {
        var parts: [String] = []
        let workshop = workshopName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !workshop.isEmpty {
            parts.append(workshop)
        }
        if let motion = secondaryLine(motionSummary: motionSummary, subline: subline) {
            parts.append(motion)
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    static func secondaryLine(
        motionSummary: String?,
        subline: String?
    ) -> String? {
        if let summary = motionSummary?.trimmingCharacters(in: .whitespacesAndNewlines),
           !summary.isEmpty
        {
            return summary
        }
        guard let subline = subline?.trimmingCharacters(in: .whitespacesAndNewlines),
              !subline.isEmpty
        else {
            return nil
        }
        let generic = subline.lowercased()
        if generic.contains("pick up where medousa") { return nil }
        if generic == "nothing needs you right now." { return nil }
        return subline
    }

    /// Compact island trailing — single digit or short count, never truncated prose.
    static func compactTrailing(
        blockedCount: Int,
        motionSummary: String?
    ) -> String? {
        if blockedCount > 0 { return "!" }
        if let summary = motionSummary {
            if let count = firstCount(before: " running", in: summary) {
                return count
            }
            if let count = firstCount(before: " queued", in: summary) {
                return count
            }
            if let count = firstCount(before: " finishing", in: summary) {
                return count
            }
        }
        return nil
    }

    private static func firstCount(before needle: String, in haystack: String) -> String? {
        guard let range = haystack.range(of: needle) else { return nil }
        let prefix = haystack[..<range.lowerBound].trimmingCharacters(in: .whitespaces)
        guard let digit = prefix.split(separator: " ").last,
              digit.allSatisfy(\.isNumber)
        else {
            return nil
        }
        return String(digit)
    }
}

struct MedousaMark: View {
    var size: CGFloat = 22

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [MedousaPalette.primarySoft, MedousaPalette.primary],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            Text("M")
                .font(.system(size: size * 0.5, weight: .bold, design: .rounded))
                .foregroundStyle(MedousaPalette.ink)
                .offset(y: -0.5)
        }
        .frame(width: size, height: size)
        .shadow(color: MedousaPalette.primary.opacity(0.35), radius: size * 0.15, y: 1)
    }
}

struct MedousaStatusPill: View {
    let label: String
    let mood: String

    private var compactLabel: String {
        switch label.lowercased() {
        case "finishing up": return "Finishing"
        case "needs you": return "Needs you"
        default: return label
        }
    }

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(MedousaPalette.accent(for: mood))
                .frame(width: 5, height: 5)
            Text(compactLabel)
                .font(.system(size: 9, weight: .semibold, design: .rounded))
                .tracking(0.2)
                .foregroundStyle(MedousaPalette.pillForeground(for: mood))
                .lineLimit(1)
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 4)
        .background(MedousaPalette.pillBackground(for: mood))
        .clipShape(Capsule())
        .overlay(
            Capsule()
                .strokeBorder(MedousaPalette.accent(for: mood).opacity(0.22), lineWidth: 0.5)
        )
        .fixedSize()
    }
}

struct MedousaLivePulseBar: View {
    let mood: String

    var body: some View {
        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
            .fill(
                LinearGradient(
                    colors: [
                        MedousaPalette.primary.opacity(0.05),
                        MedousaPalette.accent(for: mood).opacity(0.85),
                        MedousaPalette.primary.opacity(0.05),
                    ],
                    startPoint: .leading,
                    endPoint: .trailing
                )
            )
            .frame(height: 3)
    }
}
