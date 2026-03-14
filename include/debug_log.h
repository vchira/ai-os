/* AiOS — Debug Log
   Ring buffer that captures all debug output.
   View with /debug command or ask the AI to show debug log. */

#ifndef AIOS_DEBUG_LOG_H
#define AIOS_DEBUG_LOG_H

/* Append a message to the debug log ring buffer. */
void dbg_log(const char *msg);

/* Show the debug log in a scrollable window. */
void dbg_show(void);

/* Get pointer to the log buffer and its length (for tool use). */
const char *dbg_get_log(int *out_len);

/* Clear the debug log. */
void dbg_clear(void);

#endif
