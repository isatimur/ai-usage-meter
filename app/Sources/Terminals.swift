import AppKit
import Foundation

/// Jumps from a session row to the terminal that runs it.
///
/// A session is matched to a running `claude` or `codex` process by tool and
/// working directory. The tab is selected by tty in Terminal.app. For any other
/// terminal the app that owns the process is brought to the front.
enum Terminals {
    private struct Proc {
        let pid: Int32
        let ppid: Int32
        let tty: String
        let age: TimeInterval      // seconds since the process started
        let name: String
    }

    private static func run(_ path: String, _ args: [String]) -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = args
        let out = Pipe()
        p.standardOutput = out
        p.standardError = Pipe()
        do { try p.run() } catch { return "" }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        return String(decoding: data, as: UTF8.self)
    }

    private static func processes() -> [Proc] {
        run("/bin/ps", ["-axo", "pid=,ppid=,tty=,etime=,comm="]).split(separator: "\n").compactMap { line in
            let f = line.split(separator: " ", maxSplits: 4, omittingEmptySubsequences: true)
            guard f.count == 5, let pid = Int32(f[0]), let ppid = Int32(f[1]) else { return nil }
            return Proc(pid: pid, ppid: ppid, tty: String(f[2]), age: seconds(String(f[3])),
                        name: (String(f[4]) as NSString).lastPathComponent)
        }
    }

    /// `etime` looks like `[[dd-]hh:]mm:ss`.
    private static func seconds(_ etime: String) -> TimeInterval {
        var days = 0.0
        var rest = etime
        if let dash = etime.firstIndex(of: "-") {
            days = Double(etime[..<dash]) ?? 0
            rest = String(etime[etime.index(after: dash)...])
        }
        let parts = rest.split(separator: ":").compactMap { Double($0) }
        return parts.reduce(0) { $0 * 60 + $1 } + days * 86_400
    }

    private static func cwds(of pids: [Int32]) -> [Int32: String] {
        guard !pids.isEmpty else { return [:] }
        let out = run("/usr/sbin/lsof", ["-a", "-d", "cwd", "-Fpn", "-p", pids.map(String.init).joined(separator: ",")])
        var result: [Int32: String] = [:]
        var current: Int32?
        for line in out.split(separator: "\n") {
            if line.hasPrefix("p") { current = Int32(line.dropFirst()) }
            else if line.hasPrefix("n"), let pid = current { result[pid] = String(line.dropFirst()) }
        }
        return result
    }

    /// Returns a message for the user when the jump failed, or nil on success.
    @MainActor static func focus(_ s: SessionInfo) -> String? {
        let all = processes()
        let wanted = s.tool.lowercased()
        let candidates = all.filter { $0.name == wanted && $0.tty != "??" }
        let dirs = cwds(of: candidates.map(\.pid))
        let here = candidates.filter { dirs[$0.pid] == s.project }
        guard !here.isEmpty else {
            return "No open terminal found for this session. It may already be closed."
        }
        // Several terminals in one folder: take the one started closest to the session start.
        let started = s.firstDate.timeIntervalSinceNow * -1
        let target = here.min { abs($0.age - started) < abs($1.age - started) }!

        if selectTerminalTab(tty: "/dev/" + target.tty) { return nil }
        if activateOwner(of: target, in: all) { return nil }
        return "Could not find the terminal window."
    }

    private static func isRunning(_ bundleID: String) -> Bool {
        !NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).isEmpty
    }

    /// Terminal.app only. Checks that the app is running first, so the script never launches it.
    private static func selectTerminalTab(tty: String) -> Bool {
        guard isRunning("com.apple.Terminal") else { return false }
        let source = """
        tell application "Terminal"
            repeat with w in windows
                repeat with t in tabs of w
                    if tty of t is "\(tty)" then
                        set selected of t to true
                        set index of w to 1
                        activate
                        return "ok"
                    end if
                end repeat
            end repeat
        end tell
        return "no"
        """
        var error: NSDictionary?
        let result = NSAppleScript(source: source)?.executeAndReturnError(&error)
        return error == nil && result?.stringValue == "ok"
    }

    /// Walks up the parent chain to the first GUI app (Ghostty, iTerm, VS Code, Warp...) and brings it forward.
    private static func activateOwner(of proc: Proc, in all: [Proc]) -> Bool {
        let byPid = Dictionary(all.map { ($0.pid, $0) }, uniquingKeysWith: { a, _ in a })
        var pid = proc.ppid
        for _ in 0..<12 {
            if let app = NSRunningApplication(processIdentifier: pid), app.activationPolicy == .regular {
                return app.activate()
            }
            guard let parent = byPid[pid], parent.ppid > 1 else { return false }
            pid = parent.ppid
        }
        return false
    }
}
