import AppKit
import SwiftUI

@MainActor
final class UsageModel: ObservableObject {
    static let openRouterKeyAccount = "openrouter-key"

    @Published var summaries: [UsageRange: Summary] = [:]
    @Published var sessions: [UsageRange: [SessionInfo]] = [:]
    @Published var selected: UsageRange = .today
    @Published var isRefreshing = false
    @Published var hasScanned = false
    @Published var lastError: String?
    @Published var lastRefresh: Date?
    @Published var openRouter: KeyUsage?
    @Published var openRouterError: String?

    @Published var limits: LimitsBundle?
    @Published var budget: BudgetForecast?
    @Published var keychainNote: String?

    /// Recomputes the monthly budget forecast from the month summary. Cheap, so it runs on every refresh.
    func updateBudget() {
        let limit = UserDefaults.standard.double(forKey: "monthlyBudget")
        guard limit > 0, let month = summaries[.month] else { budget = nil; return }
        budget = try? CoreBridge.budget(spent: month.total.costUsd, budget: limit)
        if let b = budget { Notifier.checkBudget(b) }
    }
    @Published var obsidianStatus: String?
    private var lastObsidianWrite: Date?
    private var lastLimitsFetch: Date?
    private var isFetchingLimits = false

    /// The private limit endpoints are polled at most this often.
    private static let limitsEvery: TimeInterval = 5 * 60

    func refreshLimitsIfDue(force: Bool = false) {
        guard UserDefaults.standard.object(forKey: "showLimits") as? Bool ?? false else {
            limits = nil
            return
        }
        guard !isFetchingLimits, !CommandLine.arguments.contains("--snapshot") else { return }
        if !force, let last = lastLimitsFetch, Date().timeIntervalSince(last) < Self.limitsEvery { return }
        isFetchingLimits = true
        lastLimitsFetch = Date()
        if force { Credentials.resetBackoff() }
        Task.detached {
            var claude: String?
            var note: String?
            switch Credentials.claudeAccess() {
            case .token(let t): claude = t
            case .denied:
                note = "No access to the Keychain, so Claude limits are off. Click “Refresh” and choose “Always Allow” in the macOS dialog."
            case .missing: break
            }
            let codex = Credentials.codexAuth()
            let bundle = try? CoreBridge.limits(
                claudeToken: claude, codexToken: codex?.token, codexAccount: codex?.account)
            // The saved token expired: read the Keychain again only after a wait, not every poll.
            if bundle?.claude?.error?.contains("login expired") == true { Credentials.markClaudeTokenExpired() }
            await MainActor.run {
                self.keychainNote = note
                if let bundle {
                    self.limits = bundle
                    Notifier.check(bundle)
                }
                self.isFetchingLimits = false
            }
        }
    }

    @Published var status: StatusBundle?
    private var lastStatusFetch: Date?
    private var isFetchingStatus = false

    /// Public status pages: no login, polled at most this often, and only if the user turned it on.
    func refreshStatusIfDue(force: Bool = false) {
        guard UserDefaults.standard.object(forKey: "showStatus") as? Bool ?? false else {
            status = nil
            return
        }
        guard !isFetchingStatus, !CommandLine.arguments.contains("--snapshot") else { return }
        if !force, let last = lastStatusFetch, Date().timeIntervalSince(last) < Self.limitsEvery { return }
        isFetchingStatus = true
        lastStatusFetch = Date()
        Task.detached {
            let bundle = try? CoreBridge.status()
            await MainActor.run {
                if let bundle { self.status = bundle }
                self.isFetchingStatus = false
            }
        }
    }

    private var timer: Timer?

    /// Writes today's note to the Obsidian folder, one file per day.
    /// The folder has no default: the user picks it in Settings.
    func exportToObsidian() {
        let folder = UserDefaults.standard.string(forKey: "obsidianFolder") ?? ""
        guard !folder.trimmingCharacters(in: .whitespaces).isEmpty else {
            obsidianStatus = "Set a folder in Settings first."
            return
        }
        Task.detached {
            let status: String
            do {
                let note = try CoreBridge.dailyNote()
                let dir = URL(fileURLWithPath: (folder as NSString).expandingTildeInPath)
                try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                let file = dir.appendingPathComponent("\(note.date).md")
                try note.markdown.write(to: file, atomically: true, encoding: .utf8)
                status = "Written: \(file.lastPathComponent)"
            } catch {
                status = "Could not write:\(error.localizedDescription)"
            }
            await MainActor.run {
                self.obsidianStatus = status
                self.lastObsidianWrite = Date()
            }
        }
    }

    private func autoExportIfDue() {
        guard UserDefaults.standard.bool(forKey: "obsidianAuto") else { return }
        if let last = lastObsidianWrite, Date().timeIntervalSince(last) < 30 * 60 { return }
        exportToObsidian()
    }

    var menuTitle: String {
        guard hasScanned, let today = summaries[.today] else { return "…" }
        let cost = Format.cost(today.total.costUsd, incomplete: today.total.costIncomplete)
        if let w = limits?.claude?.windows?.first {
            return "Claude \(Int(w.percent.rounded()))% · \(cost)"
        }
        return cost
    }

    func start() {
        refresh()
        reschedule()
    }

    func reschedule() {
        timer?.invalidate()
        let seconds = UserDefaults.standard.integer(forKey: "refreshSeconds")
        let interval = TimeInterval(seconds > 0 ? seconds : 120)
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    func refresh() {
        guard !isRefreshing else { return }
        if CommandLine.arguments.contains("--demo") {
            (summaries, sessions, limits) = { let d = Demo.load(); return (d.0, d.1, d.2) }()
            budget = Demo.budget()
            hasScanned = true
            lastRefresh = Date()
            writeSnapshotIfRequested()
            return
        }
        isRefreshing = true
        let key = Keychain.get(Self.openRouterKeyAccount)
        Task.detached {
            let result: Result<([UsageRange: Summary], [UsageRange: [SessionInfo]]), Error> = Result {
                try CoreBridge.refresh(home: NSHomeDirectory())
                var all: [UsageRange: Summary] = [:]
                var lists: [UsageRange: [SessionInfo]] = [:]
                for range in UsageRange.allCases {
                    all[range] = try CoreBridge.summary(range)
                    lists[range] = try CoreBridge.sessions(range)
                }
                return (all, lists)
            }
            let (usage, usageError): (KeyUsage?, String?) = {
                guard let key, !key.isEmpty else { return (nil, nil) }
                do { return (try CoreBridge.openRouterKeyUsage(key: key), nil) }
                catch { return (nil, error.localizedDescription) }
            }()
            await MainActor.run {
                switch result {
                case .success(let (all, lists)):
                    self.summaries = all
                    self.sessions = lists
                    self.hasScanned = true
                    self.lastError = nil
                    self.lastRefresh = Date()
                    self.updateBudget()
                    self.autoExportIfDue()
                    self.refreshLimitsIfDue()
                    self.refreshStatusIfDue()
                case .failure(let error):
                    self.lastError = error.localizedDescription
                }
                self.openRouter = usage
                self.openRouterError = usageError
                self.isRefreshing = false
                self.writeSnapshotIfRequested()
            }
        }
    }

    /// `--snapshot PATH` renders the popover to a PNG after the first scan and quits.
    private func writeSnapshotIfRequested() {
        let args = CommandLine.arguments
        guard let i = args.firstIndex(of: "--snapshot"), i + 1 < args.count else { return }
        // Layout check only: AIUM_SNAPSHOT_LIMITS_JSON stands in for the server answer.
        if let raw = ProcessInfo.processInfo.environment["AIUM_SNAPSHOT_LIMITS_JSON"],
           let data = raw.data(using: .utf8) {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            limits = try? decoder.decode(LimitsBundle.self, from: data)
        }
        // A real (off-screen) window, so AppKit controls such as the picker and the buttons are drawn.
        // AIUM_SNAPSHOT_VIEW=privacy draws the Settings privacy rows instead of the popover.
        let host: NSHostingView<AnyView>
        if ProcessInfo.processInfo.environment["AIUM_SNAPSHOT_VIEW"] == "privacy" {
            let rows = VStack(alignment: .leading, spacing: 10) {
                PrivacyRows(findings: (try? CoreBridge.privacy()) ?? [])
            }
            .padding(16).frame(width: 460)
            host = NSHostingView(rootView: AnyView(rows))
        } else {
            host = NSHostingView(rootView: AnyView(PopoverView(model: self)))
        }
        let size = host.fittingSize
        host.frame = NSRect(origin: .zero, size: size)
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: .aqua)
        window.contentView = host
        window.setFrameOrigin(NSPoint(x: -10_000, y: -10_000))
        window.orderFrontRegardless()
        host.layoutSubtreeIfNeeded()
        // Let SwiftUI finish one more layout pass before drawing.
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
            host.cacheDisplay(in: host.bounds, to: rep)
            if let png = rep.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: args[i + 1]))
            }
        }
        NSApp.terminate(nil)
    }
}

enum Format {
    /// A trailing "+" means some usage has no price, so the number is a lower bound.
    static func cost(_ v: Double, incomplete: Bool = false) -> String {
        let base = v >= 100 ? String(format: "$%.0f", v) : String(format: "$%.2f", v)
        return incomplete ? base + "+" : base
    }

    private static let en = Locale(identifier: "en_US_POSIX")

    static func ago(_ d: Date, now: Date = Date()) -> String {
        let s = max(0, Int(now.timeIntervalSince(d)))
        switch s {
        case ..<60: return "just now"
        case ..<3600: return "\(s / 60) min ago"
        case ..<86_400: return "\(s / 3600) h ago"
        default: return "\(s / 86_400) d ago"
        }
    }

    static func resetIn(_ d: Date, now: Date = Date()) -> String {
        let s = max(0, Int(d.timeIntervalSince(now)))
        switch s {
        case ..<60: return "resets now"
        case ..<3600: return "resets in \(s / 60) min"
        case ..<86_400: return "resets in \(s / 3600) h \(s % 3600 / 60) min"
        default: return "resets in \(s / 86_400) d \(s % 86_400 / 3600) h"
        }
    }

    static func duration(_ seconds: TimeInterval) -> String {
        let m = Int(seconds) / 60
        if m < 1 { return "under a minute" }
        if m < 60 { return "\(m) min" }
        return "\(m / 60) h \(m % 60) min"
    }

    static func tokens(_ n: UInt64) -> String {
        let d = Double(n)
        switch d {
        case 1_000_000_000...: return String(format: "%.1fB", locale: en, d / 1_000_000_000)
        case 1_000_000...: return String(format: "%.1fM", locale: en, d / 1_000_000)
        case 1_000...: return String(format: "%.1fK", locale: en, d / 1_000)
        default: return "\(n)"
        }
    }
}
