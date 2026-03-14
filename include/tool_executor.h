/* AiOS — Tool Executor: dispatch LLM tool calls to OS primitives */
#ifndef AIOS_TOOL_EXECUTOR_H
#define AIOS_TOOL_EXECUTOR_H

#include "types.h"

void tool_executor_init(void);

/* Execute a tool by name. input_json is the raw JSON args from the LLM.
   Writes result string to out. Returns length, or negative on error. */
int tool_execute(const char *name, const char *input_json, char *out, int max);

/* Dump all stored memory entries (for /memory debug command). */
int tool_memory_dump(char *buf, int max);

/* Set LLM callback for dynamic tool execution. */
void tool_set_llm_callback(int (*cb)(const char *prompt, char *response, int max_len));

/* Test accessors — used by selftest */
int tool_memory_has_key(const char *key, char *value_out, int max);
int tool_memory_count(void);
int tool_dyn_tool_exists(const char *name);
void tool_memory_clear(void);
void tool_dyn_tools_clear(void);

/* Persistence */
void tool_force_save(void);
void tool_force_load(void);

#endif
