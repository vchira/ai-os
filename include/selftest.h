/* AiOS — Self-Test Framework
   Simulates AI conversations end-to-end without using API tokens.
   Hooks at the HTTPS layer so the full pipeline is tested:
   system prompt → JSON body → HTTP request → response parse →
   tool call detect → tool execute → follow-up → assertions. */
#ifndef AIOS_SELFTEST_H
#define AIOS_SELFTEST_H

/* Run the full self-test suite. Called by /selftest command. */
void selftest_run(void);

/* Selftest mode flag — checked by https_post to intercept requests */
extern int selftest_active;

/* Called by https_post_bin when selftest_active is set.
   Returns simulated response length, or -1 if queue exhausted.
   host is used to distinguish LLM API calls from tool HTTP requests. */
int selftest_get_response(const char *host, char *buf, int max);

#endif
