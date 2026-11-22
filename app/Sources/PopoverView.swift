import SwiftUI

struct PopoverView: View {
    @ObservedObject var model: UsageModel
    @State private var showAllSessions = false
    @State private var focusStatus: String?

    private var summary: Summary? { model.summaries[model.selected] }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("Range", selection: $model.selected) {
                ForEach(UsageRange.allCases) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            if let s = summary {
                header(s)
                budgetLine
                limitsSection
                if let n = model.keychainNote {
                    Text(n).font(.caption).foregroundStyle(.orange)
                }
                dailyChart(s)
                breakdown("By tool", s.byTool)
                breakdown("By model", s.byModel)
                breakdown("By project", s.byProject, nameTransform: lastPathComponent)
                sessionsSection
            } else {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Reading your logs. The first time can take a minute.")
                        .font(.callout).foregroundStyle(.secondary)
                }
            }

            if let key = model.openRouter { openRouterRow(key) }
            if let e = model.openRouterError {
                Text("OpenRouter error:\(e)").font(.caption).foregroundStyle(.red)
            }
            if let e = model.lastError {
                Text(e).font(.caption).foregroundStyle(.red)
            }

            HStack {
                Button { model.exportToObsidian() } label: {
                    Label("Write to Obsidian", systemImage: "square.and.arrow.down")
                }
                .buttonStyle(.borderless)
                .disabled(model.summaries.isEmpty)
                if let s = model.obsidianStatus {
                    Text(s).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }

            Divider()
            footer
        }
        .padding(14)
        .frame(width: 380)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func header(_ s: Summary) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(Format.cost(s.total.costUsd, incomplete: s.total.costIncomplete))
                .font(.system(size: 32, weight: .semibold, design: .rounded))
            Text("Estimated cost \(model.selected.periodPhrase) at API prices. This is not your bill: on a subscription you pay less.")
                .fixedSize(horizontal: false, vertical: true)
                .font(.caption).foregroundStyle(.secondary)
            Text("Tokens: \(Format.tokens(s.total.input + s.total.cacheRead + s.total.cacheWrite)) in · \(Format.tokens(s.total.output)) out · sessions: \(s.total.sessions)")
                .font(.callout).foregroundStyle(.secondary)
            if s.costIncomplete {
                Text("A “+” means some models have no price, so the real amount is higher.")
                    .font(.caption).foregroundStyle(.orange)
            }
        }
    }

    private func breakdown(_ title: String, _ rows: [String: Totals],
                           nameTransform: @escaping (String) -> String = { $0 }) -> some View {
        let top = rows.sorted { $0.value.costUsd > $1.value.costUsd }.prefix(3)
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title)
                Spacer()
                Text("output tokens").frame(alignment: .trailing)
                Text("cost").frame(width: 64, alignment: .trailing)
            }
            .font(.caption).foregroundStyle(.secondary)
            ForEach(Array(top), id: \.key) { name, t in
                HStack {
                    Text(nameTransform(name)).lineLimit(1).truncationMode(.middle)
                    Spacer()
                    Text(Format.tokens(t.output)).foregroundStyle(.secondary).monospacedDigit()
                    Text(Format.cost(t.costUsd, incomplete: t.costIncomplete))
                        .monospacedDigit().frame(width: 64, alignment: .trailing)
                }
                .font(.callout)
            }
        }
    }

    /// One bar per day with usage. Days without usage are skipped. Plain shapes, so it also renders in snapshots.
    @ViewBuilder private func dailyChart(_ s: Summary) -> some View {
        if s.byDay.count > 1, let top = s.byDay.max(by: { $0.totals.costUsd < $1.totals.costUsd }) {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Cost by day").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    Text("highest: \(Format.cost(top.totals.costUsd)) (\(top.day.suffix(5)))")
                        .font(.caption).foregroundStyle(.secondary)
                }
                HStack(alignment: .bottom, spacing: 2) {
                    ForEach(Array(s.byDay.enumerated()), id: \.offset) { _, d in
                        let h = top.totals.costUsd > 0 ? d.totals.costUsd / top.totals.costUsd : 0
                        RoundedRectangle(cornerRadius: 2)
                            .fill(d.day == top.day ? Color.orange : Color.accentColor.opacity(0.7))
                            .frame(height: max(2, 44 * h))
                            .help("\(d.day): \(Format.cost(d.totals.costUsd))")
                    }
                }
                .frame(height: 44, alignment: .bottom)
            }
        }
    }

    @ViewBuilder private var budgetLine: some View {
        if let b = model.budget {
            let text: String = switch b.status {
            case "exceeded": "Month: \(Format.cost(b.spent)), over the budget of \(Format.cost(b.budget))."
            case "over": "Month: \(Format.cost(b.spent)). At this pace it reaches \(Format.cost(b.projected ?? b.spent)) against a budget of \(Format.cost(b.budget))."
            default: "Month: \(Format.cost(b.spent)) of \(Format.cost(b.budget))" + (b.projected.map { ", forecast \(Format.cost($0))." } ?? ".")
            }
            Text(text).font(.callout)
                .fixedSize(horizontal: false, vertical: true)
                .foregroundStyle(b.status == "ok" ? Color.secondary : Color.red)
        }
    }

    @ViewBuilder private var limitsSection: some View {
        if let l = model.limits, l.claude != nil || l.codex != nil {
            VStack(alignment: .leading, spacing: 6) {
                Text("Limits (share already used)")
                    .font(.caption).foregroundStyle(.secondary)
                limitBlock("Claude", l.claude)
                limitBlock("Codex", l.codex)
            }
        }
    }

    @ViewBuilder private func limitBlock(_ name: String, _ t: ToolLimits?) -> some View {
        if let t {
            VStack(alignment: .leading, spacing: 4) {
                Text(t.plan.map { "\(name) · plan \($0)" } ?? name).font(.callout.weight(.medium))
                if let e = t.error {
                    Text(e).font(.caption).foregroundStyle(.red)
                }
                ForEach(t.windows ?? []) { limitRow($0) }
            }
        }
    }

    private func limitRow(_ w: LimitWindow) -> some View {
        let tint: Color = w.percent >= 90 ? .red : (w.percent >= 70 ? .orange : .green)
        return VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(w.label)
                Spacer()
                if let d = w.resetsAt {
                    Text(Format.resetIn(d)).font(.caption).foregroundStyle(.secondary)
                }
                Text("\(Int(w.percent.rounded()))%").monospacedDigit().frame(width: 44, alignment: .trailing)
            }
            .font(.callout)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.secondary.opacity(0.2))
                    Capsule().fill(tint).frame(width: geo.size.width * min(max(w.percent, 0), 100) / 100)
                }
            }
            .frame(height: 6)
            if let p = w.pace { paceLine(p, resetsAt: w.resetsAt) }
        }
    }

    @ViewBuilder private func paceLine(_ p: Pace, resetsAt: Date?) -> some View {
        if p.status == "runs_out", let at = p.runsOutAt {
            let left = at.timeIntervalSinceNow
            Text(left <= 60
                 ? "Limit reached."
                 : "At this pace the limit runs out in \(Format.duration(left)), before it resets.")
                .font(.caption).foregroundStyle(.red)
        }
    }

    private var sessionsSection: some View {
        let all = model.sessions[model.selected] ?? []
        let live = all.filter(\.active).count
        let shown = showAllSessions ? all : Array(all.prefix(3))
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("Sessions").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Text(live > 0 ? "running now: \(live)" : "nothing running")
                    .font(.caption).foregroundStyle(live > 0 ? .green : .secondary)
            }
            if all.isEmpty {
                Text("No sessions in this period.").font(.callout).foregroundStyle(.secondary)
            }
            let rows = VStack(alignment: .leading, spacing: 6) {
                ForEach(shown) { sessionRow($0) }
            }
            if showAllSessions {
                ScrollView { rows }.frame(maxHeight: 260)
            } else {
                rows
            }
            if let m = focusStatus {
                Text(m).font(.caption).foregroundStyle(.orange)
            }
            if all.count > 3 {
                Button(showAllSessions ? "Collapse" : "Show all (\(all.count))") {
                    showAllSessions.toggle()
                }
                .buttonStyle(.borderless).font(.caption)
            }
        }
    }

    private func sessionRow(_ s: SessionInfo) -> some View {
        Button { focusStatus = Terminals.focus(s) } label: { sessionRowContent(s) }
            .buttonStyle(.plain)
            .help("Open the terminal for this session")
    }

    private func sessionRowContent(_ s: SessionInfo) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Circle().fill(s.active ? Color.green : Color.secondary.opacity(0.35))
                .frame(width: 8, height: 8).padding(.top, 5)
            VStack(alignment: .leading, spacing: 1) {
                Text(lastPathComponent(s.project)).lineLimit(1).truncationMode(.middle)
                Text("\(s.tool) · \(s.model) · \(s.active ? "running" : Format.ago(s.lastDate))")
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.tail)
            }
            Spacer()
            Text(Format.cost(s.totals.costUsd, incomplete: s.totals.costIncomplete))
                .monospacedDigit().frame(width: 64, alignment: .trailing)
        }
        .font(.callout)
        .contentShape(Rectangle())
    }

    private func openRouterRow(_ k: KeyUsage) -> some View {
        let spent: Double = switch model.selected {
        case .today: k.usageDaily
        case .week: k.usageWeekly
        case .month: k.usageMonthly
        }
        return HStack {
            Text("OpenRouter").font(.callout)
            Spacer()
            if let left = k.limitRemaining {
                Text("\(Format.cost(left)) left").font(.caption).foregroundStyle(.secondary)
            }
            Text(Format.cost(spent)).font(.callout).monospacedDigit()
        }
    }

    private var footer: some View {
        HStack {
            Button { model.refresh(); model.refreshLimitsIfDue(force: model.keychainNote != nil) } label: {
                if model.isRefreshing { ProgressView().controlSize(.small) }
                else { Label("Refresh", systemImage: "arrow.clockwise") }
            }
            .disabled(model.isRefreshing)
            if let t = model.lastRefresh {
                Text(t, style: .time).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            SettingsLink { Text("Settings…") }
            Button("Quit") { NSApp.terminate(nil) }
        }
        .buttonStyle(.borderless)
    }

    private func lastPathComponent(_ path: String) -> String {
        path.isEmpty ? "(unknown)" : (path as NSString).lastPathComponent
    }
}
