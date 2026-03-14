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

#endif
