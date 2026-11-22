import Foundation
import UserNotifications

/// Alerts when a limit window crosses 75% or 90%, or when the pace forecast says
/// the limit runs out before the reset. Each alert fires once per window and level.
enum Notifier {
    private static let sentKey = "sentAlerts"

    static func requestAccess() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    static func check(_ bundle: LimitsBundle) {
        guard UserDefaults.standard.bool(forKey: "notifyLimits") else { return }
        var sent = Set(UserDefaults.standard.stringArray(forKey: sentKey) ?? [])
        for (tool, limits) in [("Claude", bundle.claude), ("Codex", bundle.codex)] {
            for w in limits?.windows ?? [] {
                let window = "\(tool)|\(w.label)|\(w.resetsAtMs ?? 0)"
                var alerts: [(String, String)] = []
                if w.percent >= 90 { alerts.append(("90", "\(Int(w.percent.rounded()))% used")) }
                else if w.percent >= 75 { alerts.append(("75", "\(Int(w.percent.rounded()))% used")) }
                if let p = w.pace, p.status == "runs_out", w.percent < 90, let at = p.runsOutAt {
                    alerts.append(("pace", "at this pace the limit runs out in \(Format.duration(max(0, at.timeIntervalSinceNow)))"))
                }
                for (level, text) in alerts where !sent.contains("\(window)|\(level)") {
                    sent.insert("\(window)|\(level)")
                    post(title: "\(tool): “\(w.label)” limit", body: text)
                }
            }
        }
        // Keep the list short: old windows never come back.
        UserDefaults.standard.set(Array(sent.suffix(200)), forKey: sentKey)
    }

    /// One budget alert per month and level.
    static func checkBudget(_ b: BudgetForecast) {
        guard UserDefaults.standard.bool(forKey: "notifyLimits"), b.status != "ok" else { return }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM"
        let month = f.string(from: Date())
        let key = "budget|\(month)|\(b.status)"
        var sent = Set(UserDefaults.standard.stringArray(forKey: sentKey) ?? [])
        guard !sent.contains(key) else { return }
        sent.insert(key)
        UserDefaults.standard.set(Array(sent.suffix(200)), forKey: sentKey)
        post(title: "Monthly budget",
             body: b.status == "exceeded"
                ? "Estimated spend \(Format.cost(b.spent)) is already over your budget of \(Format.cost(b.budget))."
                : "At this pace the month ends at \(Format.cost(b.projected ?? b.spent)) against a budget of \(Format.cost(b.budget)).")
    }

    private static func post(title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
