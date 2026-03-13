; =============================================================================
; AiOS - C API Wrappers
; Bridges cdecl C calling convention to register-based asm functions
; =============================================================================

section .text

; Import the actual asm implementations
extern vga_print
extern vga_print_hex
extern vga_print_dec
extern vga_set_color
extern vga_newline

; Export C-callable versions (prefixed with c_)
global c_vga_print
global c_vga_print_hex
global c_vga_print_dec
global c_vga_set_color
global c_vga_newline

; =============================================================================
; c_vga_print(const char *str)
; cdecl: arg on stack at [esp+4]
; asm expects: esi = string pointer
; =============================================================================
c_vga_print:
    push esi
    mov esi, [esp+8]           ; arg1 (account for pushed esi)
    call vga_print
    pop esi
    ret

; =============================================================================
; c_vga_print_hex(uint32_t value)
; cdecl: arg on stack at [esp+4]
; asm expects: eax = value
; =============================================================================
c_vga_print_hex:
    push eax
    mov eax, [esp+8]
    call vga_print_hex
    pop eax
    ret

; =============================================================================
; c_vga_print_dec(uint32_t value)
; cdecl: arg on stack at [esp+4]
; asm expects: eax = value
; =============================================================================
c_vga_print_dec:
    push eax
    mov eax, [esp+8]
    call vga_print_dec
    pop eax
    ret

; =============================================================================
; c_vga_set_color(uint8_t color)
; cdecl: arg on stack at [esp+4]
; asm expects: al = color
; =============================================================================
c_vga_set_color:
    push eax
    mov eax, [esp+8]
    call vga_set_color
    pop eax
    ret

; =============================================================================
; c_vga_newline(void)
; No args
; =============================================================================
c_vga_newline:
    call vga_newline
    ret
