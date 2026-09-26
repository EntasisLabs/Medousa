import AppIntents
import Foundation

@available(iOS 18.0, *)
struct WorkshopEntity: AppEntity, Codable, Hashable, Identifiable {
    static let typeDisplayRepresentation = TypeDisplayRepresentation(name: "Workshop")
    static let defaultQuery = WorkshopEntityQuery()

    let id: String
    let name: String
    let isActive: Bool

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)")
    }
}

@available(iOS 18.0, *)
enum WorkshopEntitySnapshot {
    static let appGroupId = "group.com.entasislabs.medousa-home"
    static let key = "siri.workshops.v1"

    static func load() -> [WorkshopEntity] {
        guard let encoded = UserDefaults(suiteName: appGroupId)?.string(forKey: key),
              let data = encoded.data(using: .utf8),
              let entities = try? JSONDecoder().decode([WorkshopEntity].self, from: data)
        else {
            return []
        }
        return entities
    }
}

@available(iOS 18.0, *)
struct WorkshopEntityQuery: EntityQuery {
    func entities(for identifiers: [WorkshopEntity.ID]) async throws -> [WorkshopEntity] {
        let requested = Set(identifiers)
        return WorkshopEntitySnapshot.load().filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [WorkshopEntity] {
        WorkshopEntitySnapshot.load()
    }

    func defaultResult() async -> WorkshopEntity? {
        WorkshopEntitySnapshot.load().first(where: \.isActive)
    }
}
