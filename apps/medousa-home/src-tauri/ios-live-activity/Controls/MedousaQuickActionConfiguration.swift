import AppIntents
import Foundation

enum MedousaQuickAction: String, AppEnum {
    case ask, live, camera, notes, photos, calendar, projects

    static let typeDisplayRepresentation = TypeDisplayRepresentation(name: "Medousa action")
    static let caseDisplayRepresentations: [Self: DisplayRepresentation] = [
        .ask: "Ask",
        .live: "Live",
        .camera: "Camera",
        .notes: "Notes",
        .photos: "Photos",
        .calendar: "Calendar",
        .projects: "Projects",
    ]

    var label: String {
        switch self {
        case .ask: return "Ask"
        case .live: return "Live"
        case .camera: return "Camera"
        case .notes: return "Notes"
        case .photos: return "Photos"
        case .calendar: return "Calendar"
        case .projects: return "Projects"
        }
    }

    var symbol: String {
        switch self {
        case .ask: return "message"
        case .live: return "waveform"
        case .camera: return "camera"
        case .notes: return "note.text"
        case .photos: return "photo.on.rectangle"
        case .calendar: return "calendar"
        case .projects: return "square.grid.2x2"
        }
    }

    func url() -> URL {
        if self == .live {
            return medousaLaunchURL(host: "live", query: [URLQueryItem(name: "mode", value: "new")])
        }
        return medousaLaunchURL(host: "compose", query: [URLQueryItem(name: "action", value: rawValue)])
    }
}

struct MedousaQuickActionsConfiguration: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Medousa Quick Actions"
    static let description = IntentDescription("Choose the actions shown in your Medousa widget.")

    @Parameter(title: "Primary action", default: .ask)
    var primary: MedousaQuickAction

    @Parameter(title: "First action", default: .live)
    var first: MedousaQuickAction

    @Parameter(title: "Second action", default: .camera)
    var second: MedousaQuickAction

    @Parameter(title: "Third action", default: .notes)
    var third: MedousaQuickAction

    @Parameter(title: "Fourth action", default: .calendar)
    var fourth: MedousaQuickAction

    @Parameter(title: "Fifth action", default: .projects)
    var fifth: MedousaQuickAction
}
