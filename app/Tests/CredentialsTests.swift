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
}
