import XCTest
@testable import AIUsageMeter

final class CredentialsTests: XCTestCase {
    private func blob(_ json: String) -> Data { Data(json.utf8) }

    func testReadsTheClaudeAccessToken() {
        let d = blob(#"{"claudeAiOauth":{"accessToken":"tok-1","expiresAt":1}}"#)
        XCTAssertEqual(Credentials.claudeToken(in: d), "tok-1")
    }

    /// A second keychain item can hold other logins only (MCP servers, for example).
    func testAnItemWithoutAClaudeLoginGivesNoToken() {
        XCTAssertNil(Credentials.claudeToken(in: blob(#"{"mcpOAuth":{"x":{}}}"#)))
        XCTAssertNil(Credentials.claudeToken(in: blob(#"{"claudeAiOauth":{"accessToken":""}}"#)))
        XCTAssertNil(Credentials.claudeToken(in: blob("not json")))
    }

    func testTheFirstItemWithATokenWinsAndNewestComesFirst() {
        let items: [Credentials.KeychainItem] = [
            .init(account: "unknown", modified: Date(timeIntervalSince1970: 100)),
            .init(account: "me", modified: Date(timeIntervalSince1970: 200)),
            .init(account: "old", modified: Date(timeIntervalSince1970: 50)),
        ]
        XCTAssertEqual(Credentials.newestFirst(items).map(\.account), ["me", "unknown", "old"])
    }

    func testPickSkipsItemsWithoutATokenAndKeepsTheNewestValidOne() {
        let items: [Credentials.KeychainItem] = [
            .init(account: "unknown", modified: Date(timeIntervalSince1970: 300)),
            .init(account: "me", modified: Date(timeIntervalSince1970: 200)),
        ]
        let data: [String: Data] = [
            "unknown": blob(#"{"mcpOAuth":{}}"#),
            "me": blob(#"{"claudeAiOauth":{"accessToken":"good"}}"#),
        ]
        XCTAssertEqual(Credentials.pickClaudeToken(items) { data[$0.account] }, "good")
    }

    /// Every item read can raise a macOS prompt. Once an account has held the login,
    /// read it first and leave the other items alone.
    func testTheAccountThatWorkedIsReadFirstAndTheOthersAreNotTouched() {
        let items: [Credentials.KeychainItem] = [
            .init(account: "unknown", modified: Date(timeIntervalSince1970: 300)),
            .init(account: "me", modified: Date(timeIntervalSince1970: 200)),
        ]
        var reads: [String] = []
        let found = Credentials.pickClaudeLogin(items, preferred: "me") { item in
            reads.append(item.account)
            return self.blob(#"{"claudeAiOauth":{"accessToken":"good"}}"#)
        }
        XCTAssertEqual(found?.token, "good")
        XCTAssertEqual(found?.account, "me")
        XCTAssertEqual(reads, ["me"])
    }

    func testAMissingPreferredAccountFallsBackToNewestFirst() {
        let items: [Credentials.KeychainItem] = [
            .init(account: "a", modified: Date(timeIntervalSince1970: 100)),
            .init(account: "b", modified: Date(timeIntervalSince1970: 200)),
        ]
        var reads: [String] = []
        _ = Credentials.pickClaudeLogin(items, preferred: "gone") { item in
            reads.append(item.account)
            return nil
        }
        XCTAssertEqual(reads, ["b", "a"])
    }

    /// Claude Code refreshes its login only while it runs. An expired token stays in the
    /// Keychain, so reading it again every poll only repeats the prompt.
    func testAnExpiredTokenIsNotReadFromTheKeychainAgainForAWhile() {
        var cache = Credentials.TokenCache()
        let t0 = Date(timeIntervalSince1970: 1_000_000)
        cache.store("old", now: t0)
        cache.markExpired(now: t0)
        XCTAssertEqual(cache.usable(now: t0.addingTimeInterval(5 * 60)), "old")
        XCTAssertNil(cache.usable(now: t0.addingTimeInterval(31 * 60)))
    }

    func testANewTokenReplacesTheExpiredOne() {
        var cache = Credentials.TokenCache()
        let t0 = Date(timeIntervalSince1970: 1_000_000)
        cache.store("old", now: t0)
        cache.markExpired(now: t0)
        cache.store("new", now: t0.addingTimeInterval(40 * 60))
        XCTAssertEqual(cache.usable(now: t0.addingTimeInterval(41 * 60)), "new")
    }

    /// Reading the Keychain gave the same expired token: wait again, do not ask every poll.
    func testTheSameExpiredTokenStartsANewWait() {
        var cache = Credentials.TokenCache()
        let t0 = Date(timeIntervalSince1970: 1_000_000)
        cache.store("old", now: t0)
        cache.markExpired(now: t0)
        let t1 = t0.addingTimeInterval(40 * 60)
        cache.store("old", now: t1)
        XCTAssertEqual(cache.usable(now: t1.addingTimeInterval(5 * 60)), "old")
        XCTAssertNil(cache.usable(now: t1.addingTimeInterval(31 * 60)))
    }
}
