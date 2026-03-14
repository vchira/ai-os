; =============================================================================
; AiOS - Kernel Main
; Master initialization and entry point
; The Deterministic Substrate for AI-Native Computing
; =============================================================================

%include "include/constants.inc"

section .data

; Boot banner
banner_line1: db '     _    _  ___  ____  ', 10, 0
banner_line2: db '    / \  (_)/ _ \/ ___| ', 10, 0
banner_line3: db '   / _ \ | | | | \___ \ ', 10, 0
banner_line4: db '  / ___ \| | |_| |___) |', 10, 0
banner_line5: db ' /_/   \_\_|\___/|____/ ', 10, 0
banner_line6: db '                         ', 10, 0
banner_ver:   db 'AiOS v0.1 - AI-Native Operating System', 10, 0
banner_sub:   db 'The Deterministic Substrate', 10, 0
banner_info:  db 10, 0

boot_core_msg:  db '[OK] GDT + Memory + IDT + Paging initialized', 10, 0
boot_fb_msg:    db '[OK] Framebuffer initialized', 10, 0
boot_pit_msg:   db '[OK] PIT timer initialized (100 Hz)', 10, 0
boot_kbd_msg:   db '[OK] Keyboard driver initialized', 10, 0
boot_net_msg:   db 'Initializing network...', 10, 0
boot_done_msg:  db 10, 0

section .bss
global saved_mbi
saved_mbi: resd 1               ; multiboot info pointer, used by vga_init shim

section .text
global kernel_main
global debug_print

extern gdt_init
extern idt_init
extern timer_init
extern keyboard_init
extern vga_init
extern vga_print
extern vga_set_color
extern vga_clear
extern vga_newline
extern memory_init
extern paging_init
extern crt_init
extern net_init
extern ai_prompt_run
extern context_init
extern syscall_init
extern llm_init
extern rtc_init
extern tool_executor_init
extern tls_init

; =============================================================================
; kernel_main - Kernel entry point
; Called from boot.asm: stack has [ret_addr][magic][mbi_ptr]
; =============================================================================
kernel_main:
    ; Save multiboot info pointer for framebuffer init
    mov eax, [esp+8]           ; mbi pointer (pushed first by _start)
    mov [saved_mbi], eax

    ; --- Phase 1: Core init (no display — VESA mode, VGA text buffer inactive) ---
    call gdt_init
    call memory_init
    call idt_init
    call paging_init            ; enables paging + PSE (needed for FB mapping)

    ; --- Phase 2: Initialize framebuffer (now we can see output) ---
    call vga_init               ; shim calls fb_init(saved_mbi)

    ; Print retroactive boot status
    mov esi, boot_core_msg
    call debug_print
    mov esi, boot_fb_msg
    call debug_print

    ; --- Phase 3: Rest of initialization ---
    call timer_init
    mov esi, boot_pit_msg
    call debug_print

    call keyboard_init
    mov esi, boot_kbd_msg
    call debug_print

    ; Initialize C runtime (heap)
    call crt_init

    ; Initialize Intent Syscall gate (INT 0x80)
    call syscall_init

    ; Initialize Context Frame
    call context_init

    ; Initialize Real-Time Clock
    call rtc_init

    ; Initialize Tool Executor (in-memory task store)
    call tool_executor_init

    ; Initialize LLM provider system
    call llm_init

    ; Initialize network stack (RTL8139 + lwIP + DHCP)
    mov esi, boot_net_msg
    call debug_print
    call net_init

    ; Initialize TLS subsystem (mbedTLS entropy + RNG)
    call tls_init

    ; Print boot complete separator
    mov esi, boot_done_msg
    call debug_print

    ; Set banner color (light cyan)
    mov al, (COLOR_BLACK << 4) | COLOR_LCYAN
    call vga_set_color

    ; Print banner
    mov esi, banner_line1
    call vga_print
    mov esi, banner_line2
    call vga_print
    mov esi, banner_line3
    call vga_print
    mov esi, banner_line4
    call vga_print
    mov esi, banner_line5
    call vga_print
    mov esi, banner_line6
    call vga_print

    ; Version in yellow
    mov al, (COLOR_BLACK << 4) | COLOR_YELLOW
    call vga_set_color
    mov esi, banner_ver
    call vga_print

    ; Subtitle in light green
    mov al, (COLOR_BLACK << 4) | COLOR_LGREEN
    call vga_set_color
    mov esi, banner_sub
    call vga_print

    ; Info in default color
    mov al, DEFAULT_COLOR
    call vga_set_color
    mov esi, banner_info
    call vga_print

    ; Launch the AI prompt
    call ai_prompt_run

    ; Should never reach here
.idle:
    hlt
    jmp .idle

; =============================================================================
; debug_print — Conditional print gated by AIOS_DEBUG
; Input: esi = pointer to null-terminated string
; When AIOS_DEBUG=1, prints via vga_print. When 0, no-op.
; =============================================================================
debug_print:
%if AIOS_DEBUG
    jmp vga_print       ; tail call — vga_print's ret returns to our caller
%else
    ret
%endif
