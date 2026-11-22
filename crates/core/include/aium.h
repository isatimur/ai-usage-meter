#ifndef AIUM_H
#define AIUM_H

#ifdef __cplusplus
extern "C" {
#endif

/* Every function returns a heap-allocated JSON string, or {"error": "..."} on
   failure. Release it with aium_free. */

/* Scan `home` for new usage. Returns {"ok":true,"bytes_read":N,"events_added":N}. */
char *aium_refresh(const char *home);

/* Summary JSON for "today", "week" or "month". Call aium_refresh first. */
char *aium_summary(const char *range);

/* JSON array of the newest `limit` sessions in "today", "week" or "month",
   newest first. Call aium_refresh first. */
char *aium_sessions(const char *range, unsigned int limit);

/* {"date":"YYYY-MM-DD","markdown":"..."}: today's note for Obsidian.
   Call aium_refresh first. */
char *aium_daily_note(void);
char *aium_budget(double spent, double budget);

/* Telemetry and feedback switches of the supported tools, read from local
   config and the environment. Returns {"findings": [...]}. */
char *aium_privacy(void);

/* Plans (apply = 0) or applies (apply != 0) turning off the telemetry and
   feedback switches in each tool's own config. Applying saves a backup of each
   file first. Returns {"changes": [{file, summary, diff}], "backups": [...]}. */
char *aium_privacy_disable(int apply);

/* Plan limits. NULL or blank tokens skip that tool. Returns
   {"claude": <limits>|{"error":..}|null, "codex": ...}. Tokens are used for this
   call only. */
char *aium_limits(const char *claude_token, const char *codex_token, const char *codex_account);

/* OpenRouter key usage. The key is used for this call only. */
char *aium_openrouter_key_usage(const char *key);

/* Release a string returned by any function above. NULL is a no-op. */
void aium_free(char *s);

#ifdef __cplusplus
}
#endif

#endif
