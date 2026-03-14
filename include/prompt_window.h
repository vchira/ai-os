/* AiOS — Prompt Window
   Renders the AI prompt inside a movable, resizable window. */

#ifndef AIOS_PROMPT_WINDOW_H
#define AIOS_PROMPT_WINDOW_H

/* Create the prompt window (nearly maximized). Call once during boot. */
void prompt_win_init(void);

/* Print a string to the prompt window (replaces vga_print for prompt). */
void prompt_win_print(const char *str);

/* Print a single character (handles backspace). */
void prompt_win_putchar(int ch);

/* Clear the prompt window content. */
void prompt_win_clear(void);

/* Set the text color attribute (same as vga_set_color). */
void prompt_win_set_color(int attr);

/* Print a newline. */
void prompt_win_newline(void);

/* Print a decimal number. */
void prompt_win_print_dec(int n);

/* Print a hex byte. */
void prompt_win_print_hex(int n);

#endif
