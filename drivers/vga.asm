; =============================================================================
; AiOS — VGA API Shims
; Preserves the existing vga_print/vga_putchar/etc. calling convention
; (register-based) and forwards to the C framebuffer driver (cdecl).
;
; All existing asm code (kernel.asm, ai_prompt.asm, shell.asm, paging.asm)
; continues to work unchanged — same function names, same registers.
; =============================================================================

%include "include/constants.inc"

section .data
global vga_color
vga_color: db DEFAULT_COLOR

section .text
global vga_init
global vga_clear
global vga_putchar
global vga_print
global vga_print_hex
global vga_print_dec
global vga_newline
global vga_set_color
global vga_get_cursor_row
global vga_get_cursor_col

; C framebuffer functions (cdecl)
extern fb_init
extern fb_clear
extern fb_putchar
extern fb_print
extern fb_newline
extern fb_set_color
extern fb_get_cursor_row
extern fb_get_cursor_col

; Saved multiboot info pointer (set by kernel_main)
extern saved_mbi

; =============================================================================
; vga_init — Initialize framebuffer using saved multiboot info
; =============================================================================
vga_init:
    push eax
    push ecx
    push edx
    push dword [saved_mbi]
    call fb_init
    add esp, 4
    pop edx
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_clear — Clear the framebuffer screen
; =============================================================================
vga_clear:
    push eax
    push ecx
    push edx
    call fb_clear
    pop edx
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_putchar — Print one character
; Input: al = character (register-based convention)
; =============================================================================
vga_putchar:
    push ebx
    push ecx
    push edx
    push edi
    movzx eax, al
    push eax
    call fb_putchar
    add esp, 4
    pop edi
    pop edx
    pop ecx
    pop ebx
    ret

; =============================================================================
; vga_print — Print null-terminated string
; Input: esi = pointer to string (register-based convention)
; =============================================================================
vga_print:
    push eax
    push ecx
    push edx
    push esi            ; preserve esi for caller
    push esi            ; argument to fb_print
    call fb_print
    add esp, 4
    pop esi
    pop edx
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_newline — Print a newline character
; =============================================================================
vga_newline:
    push eax
    push ecx
    push edx
    call fb_newline
    pop edx
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_set_color — Set VGA-style color attribute
; Input: al = color (bg << 4 | fg)
; =============================================================================
vga_set_color:
    mov [vga_color], al         ; keep local copy for backward compat
    push eax
    push ecx
    push edx
    movzx eax, al
    push eax
    call fb_set_color
    add esp, 4
    pop edx
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_get_cursor_row / vga_get_cursor_col
; Returns: eax = row or column
; =============================================================================
vga_get_cursor_row:
    push ecx
    push edx
    call fb_get_cursor_row
    pop edx
    pop ecx
    ret

vga_get_cursor_col:
    push ecx
    push edx
    call fb_get_cursor_col
    pop edx
    pop ecx
    ret

; =============================================================================
; vga_print_hex — Print 32-bit value in hex (keeps pure asm, calls vga_putchar)
; Input: eax = value
; =============================================================================
vga_print_hex:
    push eax
    push ebx
    push ecx
    push edx

    mov ebx, eax
    push eax
    mov al, '0'
    call vga_putchar
    mov al, 'x'
    call vga_putchar
    pop eax

    mov ecx, 8
.hex_loop:
    rol ebx, 4
    mov eax, ebx
    and eax, 0x0F
    cmp eax, 10
    jl .hex_digit
    add eax, 'A' - 10
    jmp .hex_print
.hex_digit:
    add eax, '0'
.hex_print:
    call vga_putchar
    dec ecx
    jnz .hex_loop

    pop edx
    pop ecx
    pop ebx
    pop eax
    ret

; =============================================================================
; vga_print_dec — Print 32-bit unsigned decimal (keeps pure asm, calls vga_putchar)
; Input: eax = value
; =============================================================================
vga_print_dec:
    push eax
    push ebx
    push ecx
    push edx

    mov ebx, eax
    mov ecx, 0

    test ebx, ebx
    jnz .push_digits
    mov al, '0'
    call vga_putchar
    jmp .dec_done

.push_digits:
    test ebx, ebx
    jz .pop_digits
    mov eax, ebx
    xor edx, edx
    mov ebx, 10
    div ebx
    mov ebx, eax
    push edx
    inc ecx
    jmp .push_digits

.pop_digits:
    test ecx, ecx
    jz .dec_done
    pop eax
    add eax, '0'
    call vga_putchar
    dec ecx
    jmp .pop_digits

.dec_done:
    pop edx
    pop ecx
    pop ebx
    pop eax
    ret
