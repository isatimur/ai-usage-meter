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

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "Claude Code-credentials",
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard let data = item as? Data,
                  let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let oauth = obj["claudeAiOauth"] as? [String: Any],
                  let token = oauth["accessToken"] as? String, !token.isEmpty
            else { return .missing }
            cachedClaude = token
            return .token(token)
        case errSecItemNotFound:
            return .missing
        default:
            // Denied, cancelled, or the keychain is locked: do not ask again right away.
            deniedUntil = Date().addingTimeInterval(backoff)
            return .denied
        }
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
