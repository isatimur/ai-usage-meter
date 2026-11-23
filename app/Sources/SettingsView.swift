import ServiceManagement
import SwiftUI

/// What each tool's telemetry and sharing switches are set to, from local settings.
struct PrivacyRows: View {
    let findings: [PrivacyFinding]

    var body: some View {
        if findings.isEmpty {
            Text("Could not read the tool settings.").font(.caption).foregroundStyle(.secondary)
        }
        ForEach(findings) { f in
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Image(systemName: f.sending ? "arrow.up.circle.fill" : "checkmark.circle")
                        .foregroundStyle(f.sending ? .orange : .green)
                    Text("\(f.tool): \(f.controls)")
                        .fixedSize(horizontal: false, vertical: true)
                }
                Text(f.sending ? "On. To turn off: \(f.howToTurnOff)" : "Off.")
                    .font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        Text("This lists the switches each tool ships, read from your local settings. It does not show what a tool sends.")
            .font(.caption).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

struct SettingsView: View {
    @ObservedObject var model: UsageModel
    @AppStorage("refreshSeconds") private var refreshSeconds = 120
    @AppStorage("showLimits") private var showLimits = false
    @AppStorage("notifyLimits") private var notifyLimits = false
    @AppStorage("showStatus") private var showStatus = false
    @AppStorage("monthlyBudget") private var monthlyBudget = 0.0
    @AppStorage("obsidianFolder") private var obsidianFolder = ""
    @AppStorage("obsidianAuto") private var obsidianAuto = false
    @State private var openRouterKey = Keychain.get(UsageModel.openRouterKeyAccount) ?? ""
    @State private var launchAtLogin = SMAppService.mainApp.status == .enabled
    @State private var loginError: String?
    @State private var privacy: [PrivacyFinding] = (try? CoreBridge.privacy()) ?? []
    @State private var plannedChanges: [PrivacyChange] = []
    @State private var confirmDisable = false
    @State private var fixMessage: String?

    var body: some View {
        Form {
            Section("OpenRouter") {
                SecureField("API key", text: $openRouterKey)
                    .onSubmit(saveKey)
                HStack {
                    Button("Save key", action: saveKey)
                    Button("Delete") {
                        openRouterKey = ""
                        saveKey()
                    }
                }
                Text("The key is stored in the Keychain. Leave the field empty to turn OpenRouter off.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Subscription limits") {
                Toggle("Show Claude and Codex limits", isOn: $showLimits)
                    .onChange(of: showLimits) { _, _ in model.refreshLimitsIfDue(force: true) }
                Toggle("Send notifications (limits at 75% and 90%, fast usage, budget)", isOn: $notifyLimits)
                    .onChange(of: notifyLimits) { _, on in if on { Notifier.requestAccess() } }
                Text("The app reads your Claude Code login (Keychain, macOS asks once) and your Codex login (~/.codex/auth.json). Every 5 minutes it asks the Anthropic and OpenAI servers for your limits. It does not store tokens anywhere. These requests are unofficial and may stop working.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Provider status") {
                Toggle("Show Claude and OpenAI incidents", isOn: $showStatus)
                    .onChange(of: showStatus) { _, _ in model.refreshStatusIfDue(force: true) }
                Text("Every 5 minutes the app reads status.claude.com and status.openai.com. These are public pages: no login, no token, nothing about you is sent. A line shows in the popover only during an incident.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Budget") {
                TextField("Monthly budget, $", value: $monthlyBudget, format: .number)
                    .onChange(of: monthlyBudget) { _, _ in model.updateBudget() }
                Text("0 means off. The app compares the budget with the estimated cost at API prices, not with your bill.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Obsidian") {
                TextField("Notes folder", text: $obsidianFolder,
                          prompt: Text("~/Documents/Obsidian/Vault/AI Usage"))
                Toggle("Update the daily note automatically (every 30 minutes)", isOn: $obsidianAuto)
                Text("One note per day: YYYY-MM-DD.md. Writing stays off until you set a folder. The “Write to Obsidian” button is in the main window.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Privacy") {
                PrivacyRows(findings: privacy)
                if privacy.contains(where: { $0.state == "default" && $0.defaultOn }) {
                    Button("Turn off telemetry and feedback…", action: planDisable)
                }
                if let m = fixMessage {
                    Text(m).font(.caption).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Section("General") {
                Picker("Refresh every", selection: $refreshSeconds) {
                    Text("1 minute").tag(60)
                    Text("2 minutes").tag(120)
                    Text("5 minutes").tag(300)
                    Text("10 minutes").tag(600)
                }
                .onChange(of: refreshSeconds) { _, _ in model.reschedule() }
                Toggle("Launch at login", isOn: $launchAtLogin)
                    .onChange(of: launchAtLogin) { _, on in setLaunchAtLogin(on) }
                if let e = loginError { Text(e).font(.caption).foregroundStyle(.red) }
            }
        }
        .formStyle(.grouped)
        .frame(width: 460, height: 700)
        .alert("Turn off telemetry and feedback?", isPresented: $confirmDisable) {
            Button("Turn off", action: applyDisable)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(plannedChanges.map { "• \($0.summary)\n  \(($0.file as NSString).abbreviatingWithTildeInPath)" }
                .joined(separator: "\n")
                + "\n\nEach file is backed up first. Restart the tools afterwards.")
        }
    }

    private func planDisable() {
        do {
            plannedChanges = try CoreBridge.privacyDisable(apply: false).changes
            if plannedChanges.isEmpty {
                fixMessage = "Nothing to change automatically. Use the instructions above."
            } else {
                fixMessage = nil
                confirmDisable = true
            }
        } catch {
            fixMessage = error.localizedDescription
        }
    }

    private func applyDisable() {
        do {
            _ = try CoreBridge.privacyDisable(apply: true)
            privacy = (try? CoreBridge.privacy()) ?? []
            fixMessage = "Done. A backup was saved next to each changed file. Restart the tools."
        } catch {
            fixMessage = error.localizedDescription
        }
    }

    private func saveKey() {
        Keychain.set(openRouterKey.trimmingCharacters(in: .whitespaces), for: UsageModel.openRouterKeyAccount)
        model.refresh()
    }

    private func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            loginError = nil
        } catch {
            loginError = error.localizedDescription
            launchAtLogin = SMAppService.mainApp.status == .enabled
        }
    }
}
