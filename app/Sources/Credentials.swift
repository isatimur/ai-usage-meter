import Foundation
import os
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

    /// Every Keychain read can raise a macOS prompt, so each one leaves a line here.
    /// No token and no account name is ever logged. Read it with:
    /// log show --last 1h --predicate 'subsystem == "app.aiusagemeter.AIUsageMeter"'
    private static let trace = Logger(subsystem: "app.aiusagemeter.AIUsageMeter", category: "keychain")

    private static let lock = NSLock()
    private static var cache = TokenCache()         // in memory only, gone when the app quits
    private static var deniedUntil: Date?
    private static let backoff: TimeInterval = 60 * 60
    /// The account that held the login last time. The name is not a secret. Reading only
    /// this item means one macOS prompt, not one per item.
    private static let accountKey = "claudeKeychainAccount"

    /// The Claude token, held in memory. When the API says it expired, we keep serving it for
    /// a while instead of reading the Keychain again: Claude Code refreshes the item only
    /// while it runs, so an early re-read finds the same expired token and shows a prompt for nothing.
    struct TokenCache {
        private(set) var token: String?
        private var expiredAt: Date?
        static let holdOff: TimeInterval = 30 * 60

        /// The token to use now, or nil when the Keychain must be read.
        func usable(now: Date) -> String? {
            guard let token else { return nil }
            if let expiredAt, now >= expiredAt.addingTimeInterval(Self.holdOff) { return nil }
            return token
        }

        mutating func store(_ new: String, now: Date) {
            if new == token, expiredAt != nil {
                expiredAt = now           // same expired token again: wait again
            } else {
                token = new
                expiredAt = nil
            }
        }

        mutating func markExpired(now: Date) {
            if token != nil, expiredAt == nil { expiredAt = now }
        }
    }

    /// Claude Code keeps its login in the Keychain. macOS asks for the login password.
    /// The token stays in memory, so macOS is asked once per launch, not on every poll.
    /// This call can block on the prompt, so call it off the main thread.
    static func claudeAccess() -> ClaudeAccess {
        lock.lock()
        defer { lock.unlock() }
        if let t = cache.usable(now: Date()) {
            trace.debug("claude token served from memory, no keychain read")
            return .token(t)
        }
        if let until = deniedUntil, until > Date() {
            trace.info("keychain read skipped: access was denied, waiting")
            return .denied
        }

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
        let preferred = UserDefaults.standard.string(forKey: accountKey)
        trace.info("keychain read starts: \(items.count) matching items, preferred account known: \(preferred != nil)")
        let found = pickClaudeLogin(items, preferred: preferred) { item in
            trace.info("keychain item read (may show a macOS prompt)")
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
        if let found {
            cache.store(found.token, now: Date())
            UserDefaults.standard.set(found.account, forKey: accountKey)
            return .token(found.token)
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
        pickClaudeLogin(items, preferred: nil, read: read)?.token
    }

    /// Same, but reads the item of the `preferred` account first, and returns which account held the login.
    static func pickClaudeLogin(_ items: [KeychainItem], preferred: String?,
                                read: (KeychainItem) -> Data?) -> (token: String, account: String)? {
        var ordered = newestFirst(items)
        if let preferred, let i = ordered.firstIndex(where: { $0.account == preferred }) {
            ordered.insert(ordered.remove(at: i), at: 0)
        }
        for item in ordered {
            if let data = read(item), let token = claudeToken(in: data) { return (token, item.account) }
        }
        return nil
    }

    /// The API said the token expired. Keep it for a while and read the Keychain again later.
    static func markClaudeTokenExpired() {
        trace.info("claude token expired: next keychain read is allowed after the wait")
        lock.lock()
        cache.markExpired(now: Date())
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
