import Foundation

/// `--demo` fills the app with made-up numbers. Used for README screenshots, so
/// no real project name or amount ever leaves the machine.
enum Demo {
    private static func totals(_ out: Int, _ cost: Double, _ sessions: Int) -> String {
        #"{"input":\#(out * 40),"output":\#(out),"cache_read":\#(out * 300),"cache_write":\#(out * 20),"cost_usd":\#(cost),"sessions":\#(sessions),"cost_incomplete":false}"#
    }

    static func budget() -> BudgetForecast {
        let json = #"{"status":"over","spent":312.0,"budget":500.0,"projected":640.0}"#
        return try! JSONDecoder().decode(BudgetForecast.self, from: Data(json.utf8))
    }

    static func load() -> ([UsageRange: Summary], [UsageRange: [SessionInfo]], LimitsBundle) {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        let min: Int64 = 60_000
        let hour: Int64 = 3_600_000
        let fmt = DateFormatter()
        fmt.locale = Locale(identifier: "en_US_POSIX")
        fmt.dateFormat = "yyyy-MM-dd"
        let costs = [31.0, 52, 18, 0, 0, 44, 71, 63, 27, 38, 90, 58, 41, 84.2]
        let days = costs.enumerated().filter { $0.element > 0 }.map { i, c -> String in
            let d = fmt.string(from: Date().addingTimeInterval(-Double(costs.count - 1 - i) * 86_400))
            return #"["\#(d)",\#(totals(Int(c * 10_000), c, 3))]"#
        }.joined(separator: ",")
        let summary = """
        {"total":\(totals(900_000, 84.2, 9)),
         "by_tool":{"Claude":\(totals(640_000, 61.5, 6)),"Codex":\(totals(260_000, 22.7, 3))},
         "by_model":{"claude-opus-5-5":\(totals(380_000, 38.1, 4)),"claude-sonnet-5-5":\(totals(260_000, 23.4, 3)),"gpt-5-codex":\(totals(260_000, 22.7, 3))},
         "by_project":{"web-app":\(totals(420_000, 40.3, 4)),"mobile-client":\(totals(300_000, 27.9, 3)),"docs-site":\(totals(180_000, 16.0, 2))},
         "by_day":[\(days)],"cost_incomplete":false}
        """
        let sessions = """
        [{"tool":"Claude","session":"a","project":"/dev/web-app","model":"claude-opus-5-5","first_ts_ms":\(now - 90 * min),"last_ts_ms":\(now - 20_000),"active":true,"totals":\(totals(150_000, 21.4, 1))},
         {"tool":"Codex","session":"b","project":"/dev/mobile-client","model":"gpt-5-codex","first_ts_ms":\(now - 3 * hour),"last_ts_ms":\(now - 2 * min),"active":true,"totals":\(totals(90_000, 8.9, 1))},
         {"tool":"Claude","session":"c","project":"/dev/docs-site","model":"claude-sonnet-5-5","first_ts_ms":\(now - 5 * hour),"last_ts_ms":\(now - 70 * min),"active":false,"totals":\(totals(60_000, 5.2, 1))}]
        """
        let limits = """
        {"claude":{"plan":null,"error":null,"windows":[
           {"label":"5 hours","percent":62,"resets_at_ms":\(now + 2 * hour),"pace":{"status":"runs_out","expected_percent":60,"runs_out_at_ms":\(now + 110 * min)}},
           {"label":"Week","percent":31,"resets_at_ms":\(now + 4 * 24 * hour),"pace":{"status":"ok","expected_percent":43,"runs_out_at_ms":null}}]},
         "codex":{"plan":"plus","error":null,"windows":[
           {"label":"5 hours","percent":18,"resets_at_ms":\(now + 210 * min),"pace":{"status":"ok","expected_percent":30,"runs_out_at_ms":null}},
           {"label":"Week","percent":52,"resets_at_ms":\(now + 2 * 24 * hour),"pace":{"status":"ok","expected_percent":71,"runs_out_at_ms":null}}]}}
        """
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        func decode<T: Decodable>(_ s: String) -> T { try! decoder.decode(T.self, from: Data(s.utf8)) }
        let s: Summary = decode(summary)
        let list: [SessionInfo] = decode(sessions)
        var sums: [UsageRange: Summary] = [:]
        var lists: [UsageRange: [SessionInfo]] = [:]
        for r in UsageRange.allCases { sums[r] = s; lists[r] = list }
        return (sums, lists, decode(limits))
    }
}
