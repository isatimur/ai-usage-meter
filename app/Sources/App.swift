import SwiftUI

@main
struct AIUsageMeterApp: App {
    @StateObject private var model = UsageModel()

    var body: some Scene {
        MenuBarExtra {
            PopoverView(model: model)
                .onAppear { if model.lastRefresh == nil && !model.isRefreshing { model.start() } }
        } label: {
            Text(model.menuTitle)
                .help("“Claude N%” is the share of your 5-hour subscription limit already used. The amount is the estimated cost today at API prices (Claude Code, Codex, OpenCode).")
                .task { if model.lastRefresh == nil && !model.isRefreshing { model.start() } }
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(model: model)
        }
    }
}
