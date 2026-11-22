import Foundation

enum UsageRange: String, CaseIterable, Identifiable {
    case today, week, month
    var id: String { rawValue }
    var title: String {
        switch self {
        case .today: "Today"
        case .week: "7 days"
        case .month: "Month"
        }
    }

    var periodPhrase: String {
        switch self {
        case .today: "today (since 00:00)"
        case .week: "over the last 7 days"
        case .month: "this month (since the 1st)"
        }
    }
}

struct Totals: Decodable, Equatable {
    let input: UInt64
    let output: UInt64
    let cacheRead: UInt64
    let cacheWrite: UInt64
    let costUsd: Double
    let sessions: UInt64
    /// True when some usage has no price, so `costUsd` is a lower bound.
    let costIncomplete: Bool
}

struct DayEntry: Decodable {
    let day: String
    let totals: Totals

    init(from decoder: Decoder) throws {
        var c = try decoder.unkeyedContainer()
        day = try c.decode(String.self)
        totals = try c.decode(Totals.self)
    }
}

struct Summary: Decodable {
    let total: Totals
    let byTool: [String: Totals]
    let byModel: [String: Totals]
    let byProject: [String: Totals]
    let byDay: [DayEntry]
    let costIncomplete: Bool
}

struct SessionInfo: Decodable, Identifiable {
    let tool: String
    let session: String
    let project: String
    let model: String
    let firstTsMs: Int64
    let lastTsMs: Int64
    let active: Bool
    let totals: Totals

    var id: String { tool + "/" + session }
    var firstDate: Date { Date(timeIntervalSince1970: Double(firstTsMs) / 1000) }
    var lastDate: Date { Date(timeIntervalSince1970: Double(lastTsMs) / 1000) }
    var duration: TimeInterval { Double(lastTsMs - firstTsMs) / 1000 }
}

struct Pace: Decodable {
    let status: String          // "ok" or "runs_out"
    let expectedPercent: Double
    let runsOutAtMs: Int64?

    var runsOutAt: Date? { runsOutAtMs.map { Date(timeIntervalSince1970: Double($0) / 1000) } }
}

struct LimitWindow: Decodable, Identifiable {
    let label: String
    let percent: Double
    let resetsAtMs: Int64?
    let pace: Pace?

    var id: String { label }
    var resetsAt: Date? { resetsAtMs.map { Date(timeIntervalSince1970: Double($0) / 1000) } }
}

/// Either the windows, or an error message from the server or the network.
struct ToolLimits: Decodable {
    let plan: String?
    let windows: [LimitWindow]?
    let error: String?
}

struct LimitsBundle: Decodable {
    let claude: ToolLimits?
    let codex: ToolLimits?
}

struct BudgetForecast: Decodable {
    let status: String          // "ok", "over" or "exceeded"
    let spent: Double
    let budget: Double
    let projected: Double?
}

struct BudgetResult: Decodable {
    let forecast: BudgetForecast?
}

struct PrivacyFinding: Decodable, Identifiable {
    let tool: String
    let setting: String
    let controls: String
    let state: String           // "off", "default" or "on"
    let defaultOn: Bool         // what "default" means for this setting
    let howToTurnOff: String
    var id: String { tool + setting }
    /// True when this setting sends data right now.
    var sending: Bool { state == "on" || (state == "default" && defaultOn) }
}

struct PrivacyResult: Decodable {
    let findings: [PrivacyFinding]
}

struct PrivacyChange: Decodable, Identifiable {
    let file: String
    let summary: String
    let diff: [String]
    var id: String { file }
}

struct PrivacyPlan: Decodable {
    let changes: [PrivacyChange]
    let backups: [String]
}

struct DailyNote: Decodable {
    let date: String
    let markdown: String
}

struct KeyUsage: Decodable {
    let label: String?
    let limit: Double?
    let limitRemaining: Double?
    let usage: Double
    let usageDaily: Double
    let usageWeekly: Double
    let usageMonthly: Double
}

enum CoreError: LocalizedError {
    case message(String)
    var errorDescription: String? {
        switch self { case .message(let m): return m }
    }
}

/// Thin wrapper over the Rust C ABI. All calls block, so call them off the main thread.
enum CoreBridge {
    private static func call(_ f: () -> UnsafeMutablePointer<CChar>?) throws -> Data {
        guard let p = f() else { throw CoreError.message("The core returned nothing.") }
        defer { aium_free(p) }
        let data = Data(bytes: p, count: strlen(p))
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let message = obj["error"] as? String {
            throw CoreError.message(message)
        }
        return data
    }

    private static func decode<T: Decodable>(_ data: Data) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: data)
    }

    static func refresh(home: String) throws {
        _ = try call { aium_refresh(home) }
    }

    static func summary(_ range: UsageRange) throws -> Summary {
        try decode(try call { aium_summary(range.rawValue) })
    }

    static func sessions(_ range: UsageRange, limit: UInt32 = 40) throws -> [SessionInfo] {
        try decode(try call { aium_sessions(range.rawValue, limit) })
    }

    static func limits(claudeToken: String?, codexToken: String?, codexAccount: String?) throws -> LimitsBundle {
        try decode(try call { aium_limits(claudeToken, codexToken, codexAccount) })
    }

    static func budget(spent: Double, budget: Double) throws -> BudgetForecast? {
        let r: BudgetResult = try decode(try call { aium_budget(spent, budget) })
        return r.forecast
    }

    static func privacy() throws -> [PrivacyFinding] {
        let r: PrivacyResult = try decode(try call { aium_privacy() })
        return r.findings
    }

    /// `apply: false` only plans. `apply: true` writes, after a backup of each file.
    static func privacyDisable(apply: Bool) throws -> PrivacyPlan {
        try decode(try call { aium_privacy_disable(apply ? 1 : 0) })
    }

    static func dailyNote() throws -> DailyNote {
        try decode(try call { aium_daily_note() })
    }

    static func openRouterKeyUsage(key: String) throws -> KeyUsage {
        try decode(try call { aium_openrouter_key_usage(key) })
    }
}
