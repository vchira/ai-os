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
banner_info:  db 'Type "help" for available commands.', 10, 10, 0

boot_gdt_msg:   db '[OK] GDT initialized', 10, 0
boot_idt_msg:   db '[OK] IDT initialized', 10, 0
boot_pit_msg:   db '[OK] PIT timer initialized (100 Hz)', 10, 0
boot_kbd_msg:   db '[OK] Keyboard driver initialized', 10, 0
boot_vga_msg:   db '[OK] VGA text mode initialized', 10, 0
boot_mem_msg:   db '[OK] Memory manager initialized', 10, 0
boot_pg_msg:    db '[OK] Paging enabled (identity-mapped 16 MB)', 10, 0
boot_net_msg:   db 'Initializing network...', 10, 0
boot_done_msg:  db 10, 0

section .text
global kernel_main

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
extern shell_run

; =============================================================================
; kernel_main - Kernel entry point
; Called from boot.asm with multiboot magic and info on stack
; =============================================================================
kernel_main:
    ; Initialize VGA
    call vga_init
    mov esi, boot_vga_msg
    call vga_print

    ; Initialize GDT
    call gdt_init
    mov esi, boot_gdt_msg
    call vga_print

    ; Initialize memory manager
    call memory_init
    mov esi, boot_mem_msg
    call vga_print

    ; Initialize IDT (sets up interrupts)
    call idt_init
    mov esi, boot_idt_msg
    call vga_print

    ; Initialize paging (identity-map first 16MB, enable CR0.PG)
    call paging_init
    mov esi, boot_pg_msg
    call vga_print

    ; Initialize PIT timer
    call timer_init
    mov esi, boot_pit_msg
    call vga_print

    ; Initialize keyboard
    call keyboard_init
    mov esi, boot_kbd_msg
    call vga_print

    ; Initialize C runtime (heap)
    call crt_init

    ; Initialize network stack (RTL8139 + lwIP + DHCP)
    mov esi, boot_net_msg
    call vga_print
    call net_init

    ; Print boot complete separator
    mov esi, boot_done_msg
    call vga_print

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

    ; Launch the shell
    call shell_run

    ; Should never reach here
.idle:
    hlt
    jmp .idle
