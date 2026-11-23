import Foundation
import Security

/// Reads the logins that Claude Code and Codex already keep on this Mac.
/// The tokens go to Rust for one request each. They are never written to disk or logged here.
enum Credentials {
    enum ClaudeAccess {
        case token(String)
        /// The user closed or denied the macOS prompt. We stop asking for a while.
        case denied
        case missing
    }

    private static let lock = NSLock()
    private static var cachedClaude: String?        // in memory only, gone when the app quits
    private static var deniedUntil: Date?
    private static let backoff: TimeInterval = 60 * 60

    /// Claude Code keeps its login in the Keychain. macOS asks for the login password.
    /// The token stays in memory, so macOS is asked once per launch, not on every poll.
    /// This call can block on the prompt, so call it off the main thread.
    static func claudeAccess() -> ClaudeAccess {
        lock.lock()
        defer { lock.unlock() }
        if let t = cachedClaude { return .token(t) }
        if let until = deniedUntil, until > Date() { return .denied }

        // Listing attributes never asks for a password.
        let list: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitAll,
        ]
        var listed: CFTypeRef?
        let listStatus = SecItemCopyMatching(list as CFDictionary, &listed)
        if listStatus == errSecItemNotFound { return .missing }
        guard listStatus == errSecSuccess, let rows = listed as? [[String: Any]] else {
            deniedUntil = Date().addingTimeInterval(backoff)
            return .denied
        }
        let items = rows.compactMap { r -> KeychainItem? in
            guard let account = r[kSecAttrAccount as String] as? String else { return nil }
            return KeychainItem(account: account, modified: r[kSecAttrModificationDate as String] as? Date ?? .distantPast)
        }
        var denied = false
        let token = pickClaudeToken(items) { item in
            let q: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: item.account,
                kSecReturnData as String: true,
                kSecMatchLimit as String: kSecMatchLimitOne,
            ]
            var out: CFTypeRef?
            let status = SecItemCopyMatching(q as CFDictionary, &out)
            if status == errSecSuccess { return out as? Data }
            // Denied, cancelled, or the keychain is locked.
            if status != errSecItemNotFound { denied = true }
            return nil
        }
        if let token {
            cachedClaude = token
            return .token(token)
        }
        if denied {
            // Do not ask again right away.
            deniedUntil = Date().addingTimeInterval(backoff)
            return .denied
        }
        return .missing
    }

    private static let service = "Claude Code-credentials"

    /// One keychain item: its account name and when it last changed.
    struct KeychainItem: Equatable {
        let account: String
        let modified: Date
    }

    /// The access token inside one item's data, or nil when the item holds no Claude login
    /// (it can hold other logins only).
    static func claudeToken(in data: Data) -> String? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let oauth = obj["claudeAiOauth"] as? [String: Any],
              let token = oauth["accessToken"] as? String, !token.isEmpty
        else { return nil }
        return token
    }

    static func newestFirst(_ items: [KeychainItem]) -> [KeychainItem] {
        items.sorted { $0.modified > $1.modified }
    }

    /// Claude Code can leave more than one item under the same service name. Try the newest
    /// first and use the first one that holds a Claude login.
    static func pickClaudeToken(_ items: [KeychainItem], read: (KeychainItem) -> Data?) -> String? {
        for item in newestFirst(items) {
            if let data = read(item), let token = claudeToken(in: data) { return token }
        }
        return nil
    }

    /// Drops the cached token (it expired) so the next call reads the Keychain again.
    static func forgetClaudeToken() {
        lock.lock()
        cachedClaude = nil
        lock.unlock()
    }

    /// The user asked again (switch toggled, Refresh pressed): allow a new prompt.
    static func resetBackoff() {
        lock.lock()
        deniedUntil = nil
        lock.unlock()
    }

    /// Codex keeps its login in `~/.codex/auth.json`.
    static func codexAuth() -> (token: String, account: String)? {
        let url = URL(fileURLWithPath: NSHomeDirectory() + "/.codex/auth.json")
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tokens = obj["tokens"] as? [String: Any],
              let token = tokens["access_token"] as? String, !token.isEmpty
        else { return nil }
        return (token, tokens["account_id"] as? String ?? "")
    }
}
