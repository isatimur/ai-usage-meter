import SwiftUI

@main
struct AIUsageMeterApp: App {
    @StateObject private var model = UsageModel()
    /// Unit tests host this app. They must not read or write the user's real cache.
    private static let underTest = ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil

    var body: some Scene {
        MenuBarExtra {
            PopoverView(model: model)
                .onAppear { if !Self.underTest && model.lastRefresh == nil && !model.isRefreshing { model.start() } }
        } label: {
            Text(model.menuTitle)
                .help("“Claude N%” is the share of your 5-hour subscription limit already used. The amount is the estimated cost today at API prices (Claude Code, Codex, OpenCode).")
                .task { if !Self.underTest && model.lastRefresh == nil && !model.isRefreshing { model.start() } }
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(model: model)
        }
    }
}
