import XCTest
@testable import AIUsageMeter

/// These tests guard the contract between the Rust core and the Swift models:
/// when a JSON field changes name or type, decoding fails here, not on a user's Mac.
final class CoreBridgeTests: XCTestCase {
    private func decode<T: Decodable>(_ json: String) throws -> T {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return try d.decode(T.self, from: Data(json.utf8))
    }

    // MARK: pure decoding

    func testProviderStatusIncidentOnlyWhenThePageSaysSo() throws {
        let ok: ProviderStatus = try decode(#"{"level":"operational","description":"All Systems Operational"}"#)
        XCTAssertFalse(ok.hasIncident)
        let bad: ProviderStatus = try decode(#"{"level":"major","description":"Elevated errors"}"#)
        XCTAssertTrue(bad.hasIncident)
        // An unreachable page is an error, never an incident.
        let down: ProviderStatus = try decode(#"{"error":"timeout"}"#)
        XCTAssertFalse(down.hasIncident)
    }

    func testLimitWindowDecodesResetAndPace() throws {
        let w: LimitWindow = try decode(
            #"{"label":"5h","percent":42.5,"resets_at_ms":1700000000000,"pace":{"status":"runs_out","expected_percent":30.0,"runs_out_at_ms":1699990000000}}"#)
        XCTAssertEqual(w.percent, 42.5)
        XCTAssertEqual(w.resetsAt, Date(timeIntervalSince1970: 1_700_000_000))
        XCTAssertEqual(w.pace?.status, "runs_out")
        XCTAssertNotNil(w.pace?.runsOutAt)
    }

    func testToolLimitsCarriesAnErrorInsteadOfWindows() throws {
        let l: ToolLimits = try decode(#"{"error":"HTTP 401"}"#)
        XCTAssertNil(l.windows)
        XCTAssertEqual(l.error, "HTTP 401")
    }

    func testDayEntryDecodesFromADayTotalsPair() throws {
        let e: DayEntry = try decode(
            #"["2026-11-10",{"input":1,"output":2,"cache_read":3,"cache_write":4,"cost_usd":0.5,"sessions":1,"cost_incomplete":false}]"#)
        XCTAssertEqual(e.day, "2026-11-10")
        XCTAssertEqual(e.totals.output, 2)
        XCTAssertEqual(e.totals.costUsd, 0.5)
    }

    func testCoreErrorReadsAsItsMessage() {
        XCTAssertEqual(CoreError.message("boom").errorDescription, "boom")
    }

    // MARK: the real Rust core

    /// The core keeps one cache per process, so every test shares one home.
    private static let home: String = {
        let dir = NSTemporaryDirectory() + "aium-swift-tests-\(getpid())"
        let logs = dir + "/.claude/projects/-w-proj"
        try? FileManager.default.createDirectory(atPath: logs, withIntermediateDirectories: true)
        // Timestamps are "now" so the event is inside every range, today or in a year.
        let now = ISO8601DateFormatter().string(from: Date())
        let line = """
        {"type":"assistant","timestamp":"\(now)","sessionId":"swift-s1","cwd":"/work/proj","isSidechain":false,\
        "requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-4-5","usage":{"input_tokens":100,\
        "output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}
        """
        try? (line + "\n").write(toFile: logs + "/a.jsonl", atomically: true, encoding: .utf8)
        return dir
    }()

    func testRefreshThenSummaryDecodesFromTheRealCore() throws {
        try CoreBridge.refresh(home: Self.home)
        let s = try CoreBridge.summary(.today)
        XCTAssertEqual(s.total.output, 50)
        XCTAssertEqual(s.total.input, 100)
        XCTAssertGreaterThan(s.total.costUsd, 0)
        XCTAssertNotNil(s.byTool["Claude"])
    }

    func testSessionsDecodeFromTheRealCore() throws {
        try CoreBridge.refresh(home: Self.home)
        let list = try CoreBridge.sessions(.today)
        let s = try XCTUnwrap(list.first { $0.session == "swift-s1" })
        XCTAssertEqual(s.tool, "Claude")
        XCTAssertEqual(s.project, "/work/proj")
        XCTAssertEqual(s.model, "claude-opus-4-5")
    }

    func testBudgetForecastDecodesFromTheRealCore() throws {
        let f = try XCTUnwrap(CoreBridge.budget(spent: 10, budget: 100))
        XCTAssertEqual(f.budget, 100)
        XCTAssertNil(try CoreBridge.budget(spent: 10, budget: 0))
    }

    func testPrivacyFindingsDecodeFromTheRealCore() throws {
        XCTAssertNoThrow(try CoreBridge.privacy())
    }
}
