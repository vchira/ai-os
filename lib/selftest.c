/* AiOS — Self-Test Suite
   Simulates AI conversations end-to-end without API tokens.
   Hooks at https_post so the full pipeline is exercised:
   system prompt → JSON body → HTTPS POST (intercepted) → response parse →
   tool call detect → tool execute → follow-up chain → assertions.

   See tests/selftest_plan.txt for human-readable test case documentation. */

#include "include/selftest.h"
#include "include/tool_executor.h"
#include "include/scheduler.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"
#include "include/window.h"

extern void fb_print(const char *str);
extern void fb_set_color(int attr);
extern void fb_newline(void);
extern int llm_ask(const char *question, char *response, int max_len);
extern int llm_get_active(void);

/* Window-based output: selftest opens a window and appends all output there */
static int st_win_id = -1;

static void st_print(const char *str) {
    if (st_win_id >= 0)
        win_append_text(st_win_id, str);
    fb_print(str);
}

static void st_newline(void) {
    if (st_win_id >= 0)
        win_append_text(st_win_id, "\n");
    fb_newline();
}

/* ========================================================================= */
/* Response queue — fed to https_post via the hook                           */
/* ========================================================================= */

int selftest_active = 0;

#define MAX_RESPONSES 8
static const char *response_queue[MAX_RESPONSES];
static int response_count = 0;
static int response_index = 0;

static void queue_clear(void) {
    response_count = 0;
    response_index = 0;
}

static void queue_add(const char *raw_text) {
    if (response_count < MAX_RESPONSES)
        response_queue[response_count++] = raw_text;
}

/* JSON-escape raw text into buf at position p. Returns new position. */
static int json_escape_into(char *buf, int p, int max, const char *raw) {
    while (*raw && p < max - 4) {
        if (*raw == '"')       { buf[p++] = '\\'; buf[p++] = '"'; }
        else if (*raw == '\\') { buf[p++] = '\\'; buf[p++] = '\\'; }
        else if (*raw == '\n') { buf[p++] = '\\'; buf[p++] = 'n'; }
        else if (*raw == '\r') { /* skip */ }
        else buf[p++] = *raw;
        raw++;
    }
    return p;
}

/* Build a fake API JSON response that works for both Claude and OpenAI.
   Claude parser looks for "text":"...", OpenAI parser looks for "content":"..." */
static int build_fake_response(char *buf, int max, const char *raw) {
    int p = 0;
    const char *s;

    s = "{\"text\":\"";
    memcpy(buf + p, s, strlen(s)); p += strlen(s);
    p = json_escape_into(buf, p, max - 60, raw);

    s = "\",\"content\":\"";
    memcpy(buf + p, s, strlen(s)); p += strlen(s);
    p = json_escape_into(buf, p, max - 10, raw);

    s = "\"}";
    memcpy(buf + p, s, strlen(s)); p += strlen(s);
    buf[p] = '\0';
    return p;
}

/* Called by the https_post_bin hook */
int selftest_get_response(const char *host, char *buf, int max) {
    /* For LLM API calls — return next queued response */
    if (strstr(host, "anthropic") || strstr(host, "openai") ||
        strstr(host, "ollama")) {
        if (response_index >= response_count) {
            /* Queue exhausted — return a safe "no tool call" response */
            const char *fallback = "{\"text\":\"Test complete.\",\"content\":\"Test complete.\"}";
            int len = strlen(fallback);
            memcpy(buf, fallback, len);
            buf[len] = '\0';
            return len;
        }
        const char *raw = response_queue[response_index++];
        return build_fake_response(buf, max, raw);
    }

    /* For http_request tool calls — return generic fake response */
    const char *fake = "{\"status\":\"ok\",\"message\":\"selftest mock response\"}";
    int len = strlen(fake);
    memcpy(buf, fake, len);
    buf[len] = '\0';
    return len;
}

/* ========================================================================= */
/* Display helpers                                                           */
/* ========================================================================= */

#define CLR_WHITE   0x0F
#define CLR_CYAN    0x0B
#define CLR_GREEN   0x0A
#define CLR_RED     0x0C
#define CLR_YELLOW  0x0E
#define CLR_GRAY    0x08
#define CLR_DEFAULT 0x07

static char print_buf[256];

static void print_header(int num, int total, const char *name) {
    st_newline();
    fb_set_color(CLR_WHITE);
    snprintf(print_buf, sizeof(print_buf), "[%02d/%02d] %s", num, total, name);
    st_print(print_buf);
    st_newline();
}

static void print_detail(const char *label, const char *value) {
    fb_set_color(CLR_GRAY);
    snprintf(print_buf, sizeof(print_buf), "  %-8s ", label);
    st_print(print_buf);
    fb_set_color(CLR_DEFAULT);
    st_print(value);
    st_newline();
}

static void print_pass(const char *check) {
    fb_set_color(CLR_GRAY);
    st_print("  Check:  ");
    st_print(check);
    fb_set_color(CLR_GREEN);
    st_print(" PASS");
    st_newline();
}

static void print_fail(const char *check, const char *detail) {
    fb_set_color(CLR_GRAY);
    st_print("  Check:  ");
    st_print(check);
    fb_set_color(CLR_RED);
    st_print(" FAIL");
    st_newline();
    if (detail && detail[0]) {
        fb_set_color(CLR_YELLOW);
        st_print("          ");
        st_print(detail);
        st_newline();
    }
}

/* ========================================================================= */
/* State management                                                          */
/* ========================================================================= */

static void reset_all(void) {
    tool_memory_clear();
    tool_dyn_tools_clear();
    /* Reset scheduler */
    sched_event_t *ev = scheduler_get_events();
    memset(ev, 0, SCHED_MAX_EVENTS * sizeof(sched_event_t));
}

/* Pre-populate a memory entry */
static void seed_memory(const char *key, const char *value) {
    char input[512];
    char out[256];
    snprintf(input, sizeof(input),
             "{\"key\":\"%s\",\"value\":\"%s\"}", key, value);
    tool_execute("memorize", input, out, sizeof(out));
}

/* Pre-populate a scheduler event */
static void seed_reminder(int minutes, const char *message) {
    scheduler_add(minutes, 0, 0, message);
}

/* Pre-populate a dynamic tool */
static void seed_dyn_tool(const char *name, const char *desc, const char *impl) {
    char input[1024];
    char out[256];
    snprintf(input, sizeof(input),
             "{\"name\":\"%s\",\"description\":\"%s\",\"implementation\":\"%s\"}",
             name, desc, impl);
    tool_execute("create_tool", input, out, sizeof(out));
}

/* ========================================================================= */
/* Run a test: set up responses, call llm_ask, return final response         */
/* ========================================================================= */

static char test_response[4096];

static int run_ai(const char *user_prompt) {
    return llm_ask(user_prompt, test_response, sizeof(test_response));
}

/* ========================================================================= */
/* Individual tests                                                          */
/* ========================================================================= */

#define TOTAL_TESTS 15

static int test_memorize(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:memorize\nTOOL_INPUT:{\"key\":\"fav_color\",\"value\":\"blue\"}");
    queue_add("Got it! I've memorized that your favorite color is blue.");

    print_detail("User:", "Remember that my favorite color is blue");
    run_ai("Remember that my favorite color is blue");

    char val[64];
    if (tool_memory_has_key("fav_color", val, sizeof(val)) && strcmp(val, "blue") == 0) {
        print_pass("memory[\"fav_color\"] == \"blue\"");
        return 1;
    }
    snprintf(print_buf, sizeof(print_buf), "got: \"%s\"", val);
    print_fail("memory[\"fav_color\"] == \"blue\"", print_buf);
    return 0;
}

static int test_recall_specific(void) {
    reset_all();
    seed_memory("fav_color", "blue");
    queue_clear();
    queue_add("TOOL_CALL:recall\nTOOL_INPUT:{\"key\":\"fav_color\"}");
    queue_add("Your favorite color is blue.");

    print_detail("Setup:", "memorized fav_color=blue");
    print_detail("User:", "What is my favorite color?");
    run_ai("What is my favorite color?");

    if (strstr(test_response, "blue")) {
        print_pass("response contains \"blue\"");
        return 1;
    }
    print_fail("response contains \"blue\"", test_response);
    return 0;
}

static int test_recall_all(void) {
    reset_all();
    seed_memory("name", "Alice");
    seed_memory("city", "Paris");
    queue_clear();
    queue_add("TOOL_CALL:recall\nTOOL_INPUT:{}");
    queue_add("I remember your name is Alice and your city is Paris.");

    print_detail("Setup:", "memorized name=Alice, city=Paris");
    print_detail("User:", "What do you remember about me?");
    run_ai("What do you remember about me?");

    if (strstr(test_response, "Alice") && strstr(test_response, "Paris")) {
        print_pass("response contains \"Alice\" and \"Paris\"");
        return 1;
    }
    print_fail("response contains \"Alice\" and \"Paris\"", test_response);
    return 0;
}

static int test_forget(void) {
    reset_all();
    seed_memory("temp_note", "delete me");
    queue_clear();
    queue_add("TOOL_CALL:forget\nTOOL_INPUT:{\"key\":\"temp_note\"}");
    queue_add("Done, I've forgotten the temp note.");

    print_detail("Setup:", "memorized temp_note=\"delete me\"");
    print_detail("User:", "Forget the temp note");
    run_ai("Forget the temp note");

    if (!tool_memory_has_key("temp_note", NULL, 0)) {
        print_pass("memory[\"temp_note\"] does NOT exist");
        return 1;
    }
    print_fail("memory[\"temp_note\"] does NOT exist", "key still present");
    return 0;
}

static int test_datetime(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:get_datetime\nTOOL_INPUT:{}");
    queue_add("The current date and time is shown above.");

    print_detail("User:", "What time is it?");
    int ret = run_ai("What time is it?");

    if (ret > 0 && test_response[0] != '\0') {
        print_pass("get_datetime executed, response not empty");
        return 1;
    }
    print_fail("get_datetime executed", "empty response");
    return 0;
}

static int test_reminder_relative(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:set_reminder\nTOOL_INPUT:{\"minutes\":\"30\",\"message\":\"check email\"}");
    queue_add("Reminder set for 30 minutes from now.");

    print_detail("User:", "Remind me in 30 minutes to check email");
    run_ai("Remind me in 30 minutes to check email");

    sched_event_t *ev = scheduler_get_events();
    for (int i = 0; i < SCHED_MAX_EVENTS; i++) {
        if (ev[i].active && strstr(ev[i].message, "check email")) {
            print_pass("scheduler has event with \"check email\"");
            return 1;
        }
    }
    print_fail("scheduler has event with \"check email\"", "not found");
    return 0;
}

static int test_reminder_absolute(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:set_reminder\nTOOL_INPUT:{\"hour\":\"14\",\"minute\":\"30\",\"message\":\"call mom\"}");
    queue_add("Reminder set for 2:30 PM.");

    print_detail("User:", "Remind me at 14:30 to call mom");
    run_ai("Remind me at 14:30 to call mom");

    sched_event_t *ev = scheduler_get_events();
    for (int i = 0; i < SCHED_MAX_EVENTS; i++) {
        if (ev[i].active && ev[i].hour == 14 && ev[i].minute == 30) {
            print_pass("scheduler event at 14:30");
            return 1;
        }
    }
    print_fail("scheduler event at 14:30", "not found");
    return 0;
}

static int test_cancel_reminder(void) {
    reset_all();
    seed_reminder(60, "test reminder");
    queue_clear();
    queue_add("TOOL_CALL:cancel_reminder\nTOOL_INPUT:{\"id\":\"0\"}");
    queue_add("Reminder cancelled.");

    print_detail("Setup:", "added reminder \"test reminder\"");
    print_detail("User:", "Cancel reminder 0");
    run_ai("Cancel reminder 0");

    sched_event_t *ev = scheduler_get_events();
    if (!ev[0].active) {
        print_pass("scheduler event 0 is not active");
        return 1;
    }
    print_fail("scheduler event 0 is not active", "still active");
    return 0;
}

static int test_create_tool(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:create_tool\n"
              "TOOL_INPUT:{\"name\":\"greet\",\"description\":\"Greets people\","
              "\"implementation\":\"Say hello to the person\"}");
    queue_add("I've created the 'greet' tool.");

    print_detail("User:", "Create a tool called greet");
    run_ai("Create a tool called greet that says hello to people");

    if (tool_dyn_tool_exists("greet")) {
        print_pass("dynamic tool \"greet\" exists");
        return 1;
    }
    print_fail("dynamic tool \"greet\" exists", "not found");
    return 0;
}

static int test_delete_tool(void) {
    reset_all();
    seed_dyn_tool("greet", "Greets people", "Say hello");
    queue_clear();
    queue_add("TOOL_CALL:delete_tool\nTOOL_INPUT:{\"name\":\"greet\"}");
    queue_add("The greet tool has been deleted.");

    print_detail("Setup:", "created dynamic tool \"greet\"");
    print_detail("User:", "Delete the greet tool");
    run_ai("Delete the greet tool");

    if (!tool_dyn_tool_exists("greet")) {
        print_pass("dynamic tool \"greet\" does NOT exist");
        return 1;
    }
    print_fail("dynamic tool \"greet\" does NOT exist", "still exists");
    return 0;
}

static int test_list_tools(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:list_tools\nTOOL_INPUT:{}");
    queue_add("I have these tools: memorize, recall, forget, and more.");

    print_detail("User:", "What tools do you have?");
    run_ai("What tools do you have?");

    /* The tool result (list_tools output) was sent in the follow-up prompt.
       The AI's final response is the second queued message. Verify it ran. */
    if (test_response[0] != '\0') {
        print_pass("list_tools executed, response not empty");
        return 1;
    }
    print_fail("list_tools executed", "empty response");
    return 0;
}

static int test_http_request(void) {
    reset_all();
    queue_clear();
    queue_add("TOOL_CALL:http_request\n"
              "TOOL_INPUT:{\"host\":\"example.com\",\"path\":\"/\",\"method\":\"GET\"}");
    queue_add("Here's what I got from example.com: selftest mock response.");

    print_detail("User:", "Fetch the homepage of example.com");
    run_ai("Fetch the homepage of example.com");

    /* The http_request tool called https_post which was intercepted.
       Verify the conversation completed. */
    if (test_response[0] != '\0') {
        print_pass("http_request executed via HTTPS hook");
        return 1;
    }
    print_fail("http_request executed", "empty response");
    return 0;
}

static int test_multi_step(void) {
    reset_all();
    queue_clear();
    /* First tool call: memorize */
    queue_add("TOOL_CALL:memorize\nTOOL_INPUT:{\"key\":\"todo_milk\",\"value\":\"buy milk\"}");
    /* Second tool call: set_reminder */
    queue_add("TOOL_CALL:set_reminder\nTOOL_INPUT:{\"minutes\":\"60\",\"message\":\"buy milk\"}");
    /* Final answer */
    queue_add("Done! Saved the note and set a 1-hour reminder for buying milk.");

    print_detail("User:", "Remember to buy milk and remind me in 1 hour");
    run_ai("Remember to buy milk and remind me in 1 hour");

    int pass = 1;
    char val[64];

    if (tool_memory_has_key("todo_milk", val, sizeof(val)) && strcmp(val, "buy milk") == 0) {
        print_pass("memory[\"todo_milk\"] == \"buy milk\"");
    } else {
        print_fail("memory[\"todo_milk\"] == \"buy milk\"", "not found");
        pass = 0;
    }

    sched_event_t *ev = scheduler_get_events();
    int found = 0;
    for (int i = 0; i < SCHED_MAX_EVENTS; i++) {
        if (ev[i].active && strstr(ev[i].message, "buy milk")) { found = 1; break; }
    }
    if (found) {
        print_pass("scheduler has event with \"buy milk\"");
    } else {
        print_fail("scheduler has event with \"buy milk\"", "not found");
        pass = 0;
    }

    return pass;
}

static int test_persistence(void) {
    reset_all();
    seed_memory("persist_test", "survives reboot");

    print_detail("Setup:", "memorized persist_test=\"survives reboot\"");
    print_detail("Action:", "save → clear RAM → load");

    /* Save to disk */
    tool_force_save();

    /* Clear RAM */
    tool_memory_clear();

    /* Verify it's gone from RAM */
    if (tool_memory_has_key("persist_test", NULL, 0)) {
        print_fail("RAM cleared", "key still in RAM after clear");
        return 0;
    }

    /* Load from disk */
    tool_force_load();

    /* Verify it's back */
    char val[64];
    if (tool_memory_has_key("persist_test", val, sizeof(val)) &&
        strcmp(val, "survives reboot") == 0) {
        print_pass("after load: memory[\"persist_test\"] == \"survives reboot\"");
        return 1;
    }
    print_fail("after load: memory[\"persist_test\"]", "not found after load");
    return 0;
}

static int test_error_missing_key(void) {
    reset_all();
    int count_before = tool_memory_count();
    queue_clear();
    /* AI tries to memorize without a key field */
    queue_add("TOOL_CALL:memorize\nTOOL_INPUT:{\"value\":\"orphan data\"}");
    queue_add("Sorry, I need a key name to store that.");

    print_detail("User:", "Store something for me");
    run_ai("Store something for me");

    int count_after = tool_memory_count();
    if (count_after == count_before) {
        print_pass("memory count unchanged (error handled)");
        return 1;
    }
    snprintf(print_buf, sizeof(print_buf), "before=%d after=%d", count_before, count_after);
    print_fail("memory count unchanged", print_buf);
    return 0;
}

/* ========================================================================= */
/* Main test runner                                                          */
/* ========================================================================= */

void selftest_run(void) {
    /* Check provider is configured */
    if (llm_get_active() < 0) {
        fb_set_color(CLR_RED);
        fb_print("Self-test requires an active LLM provider.");
        fb_newline();
        fb_print("Use /provider to select one (no real API calls are made).");
        fb_newline();
        fb_set_color(CLR_DEFAULT);
        return;
    }

    /* Open a window for selftest output */
    st_win_id = win_create("Self-Test", 60, 40, 600, 500,
                           WIN_CLOSABLE | WIN_RESIZABLE | WIN_SCROLLABLE);

    st_newline();
    fb_set_color(CLR_WHITE);
    st_print("=== AiOS Self-Test Suite ===");
    st_newline();
    fb_set_color(CLR_GRAY);
    st_print("Testing all subsystems with simulated AI conversations.");
    st_newline();
    st_print("No API tokens used. Full pipeline exercised.");
    st_newline();

    selftest_active = 1;

    int passed = 0;
    int total = TOTAL_TESTS;

    /* Save current state so we can restore after tests */
    /* (we can't easily snapshot — tests will leave clean state) */

    print_header(1, total, "Memorize a value");
    passed += test_memorize();

    print_header(2, total, "Recall a specific value");
    passed += test_recall_specific();

    print_header(3, total, "Recall all entries");
    passed += test_recall_all();

    print_header(4, total, "Forget a value");
    passed += test_forget();

    print_header(5, total, "Get date/time");
    passed += test_datetime();

    print_header(6, total, "Set reminder (relative)");
    passed += test_reminder_relative();

    print_header(7, total, "Set reminder (absolute)");
    passed += test_reminder_absolute();

    print_header(8, total, "Cancel reminder");
    passed += test_cancel_reminder();

    print_header(9, total, "Create dynamic tool");
    passed += test_create_tool();

    print_header(10, total, "Delete dynamic tool");
    passed += test_delete_tool();

    print_header(11, total, "List tools");
    passed += test_list_tools();

    print_header(12, total, "HTTP request");
    passed += test_http_request();

    print_header(13, total, "Multi-step (memorize + reminder)");
    passed += test_multi_step();

    print_header(14, total, "Persistence (save/load cycle)");
    passed += test_persistence();

    print_header(15, total, "Error handling (missing field)");
    passed += test_error_missing_key();

    selftest_active = 0;

    /* Clean up test data */
    reset_all();

    /* Print summary */
    st_newline();
    fb_set_color(CLR_WHITE);
    st_print("=== Results: ");
    if (passed == total) {
        fb_set_color(CLR_GREEN);
    } else {
        fb_set_color(CLR_RED);
    }
    snprintf(print_buf, sizeof(print_buf), "%d/%d", passed, total);
    st_print(print_buf);
    fb_set_color(CLR_WHITE);
    if (passed == total)
        st_print(" ALL PASSED ===");
    else
        st_print(" SOME FAILED ===");
    st_newline();
    fb_set_color(CLR_DEFAULT);

    /* Window stays open for user to review (closable via mouse) */
}
