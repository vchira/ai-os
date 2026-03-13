; =============================================================================
; AsmOS - VGA Text Mode Driver
; 80x25 text mode, color support, scrolling, hardware cursor
; =============================================================================

%include "include/constants.inc"

section .data
global vga_color
vga_row:    dd 0
vga_col:    dd 0
vga_color:  db DEFAULT_COLOR

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

; =============================================================================
; vga_init - Initialize VGA text mode
; =============================================================================
vga_init:
    mov byte [vga_color], DEFAULT_COLOR
    call vga_clear
    ret

; =============================================================================
; vga_clear - Clear the entire screen
; =============================================================================
vga_clear:
    push eax
    push ecx
    push edi

    mov edi, VGA_BUFFER
    mov ah, [vga_color]
    mov al, ' '
    mov ecx, VGA_SIZE
    rep stosw

    mov dword [vga_row], 0
    mov dword [vga_col], 0
    call vga_update_cursor

    pop edi
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_putchar - Print a single character
; Input: al = character
; =============================================================================
vga_putchar:
    push ebx
    push ecx
    push edx
    push edi

    cmp al, 10                  ; Newline?
    je .newline
    cmp al, 13                  ; Carriage return?
    je .carriage_return
    cmp al, 8                   ; Backspace?
    je .backspace

    ; Calculate offset: (row * 80 + col) * 2
    mov ecx, [vga_row]
    imul ecx, VGA_WIDTH
    add ecx, [vga_col]
    shl ecx, 1

    ; Write character + attribute
    mov edi, VGA_BUFFER
    add edi, ecx
    mov ah, [vga_color]
    mov [edi], ax

    ; Advance cursor
    inc dword [vga_col]
    cmp dword [vga_col], VGA_WIDTH
    jl .done
    ; Wrap to next line
    mov dword [vga_col], 0
    inc dword [vga_row]
    jmp .check_scroll

.newline:
    mov dword [vga_col], 0
    inc dword [vga_row]
    jmp .check_scroll

.carriage_return:
    mov dword [vga_col], 0
    jmp .done

.backspace:
    cmp dword [vga_col], 0
    je .done
    dec dword [vga_col]
    ; Clear the character at cursor
    mov ecx, [vga_row]
    imul ecx, VGA_WIDTH
    add ecx, [vga_col]
    shl ecx, 1
    mov edi, VGA_BUFFER
    add edi, ecx
    mov byte [edi], ' '
    mov byte [edi+1], DEFAULT_COLOR
    jmp .done

.check_scroll:
    cmp dword [vga_row], VGA_HEIGHT
    jl .done
    call vga_scroll
    mov dword [vga_row], VGA_HEIGHT - 1

.done:
    call vga_update_cursor
    pop edi
    pop edx
    pop ecx
    pop ebx
    ret

; =============================================================================
; vga_scroll - Scroll screen up by one line
; =============================================================================
vga_scroll:
    push eax
    push ecx
    push esi
    push edi

    ; Copy lines 1-24 to lines 0-23
    mov edi, VGA_BUFFER
    mov esi, VGA_BUFFER + (VGA_WIDTH * 2)
    mov ecx, VGA_WIDTH * (VGA_HEIGHT - 1)
    rep movsw

    ; Clear the last line
    mov ah, [vga_color]
    mov al, ' '
    mov ecx, VGA_WIDTH
    rep stosw

    pop edi
    pop esi
    pop ecx
    pop eax
    ret

; =============================================================================
; vga_print - Print a null-terminated string
; Input: esi = pointer to string
; =============================================================================
vga_print:
    push eax
    push esi

.loop:
    lodsb
    test al, al
    jz .done
    call vga_putchar
    jmp .loop

.done:
    pop esi
    pop eax
    ret

; =============================================================================
; vga_newline - Print a newline
; =============================================================================
vga_newline:
    push eax
    mov al, 10
    call vga_putchar
    pop eax
    ret

; =============================================================================
; vga_print_hex - Print a 32-bit value in hexadecimal
; Input: eax = value to print
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

    mov ecx, 8                  ; 8 hex digits
.hex_loop:
    rol ebx, 4                  ; Rotate left to get next nibble
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
; vga_print_dec - Print a 32-bit unsigned value in decimal
; Input: eax = value to print
; =============================================================================
vga_print_dec:
    push eax
    push ebx
    push ecx
    push edx

    mov ebx, eax
    mov ecx, 0                  ; Digit counter

    test ebx, ebx
    jnz .push_digits
    ; Handle zero
    mov al, '0'
    call vga_putchar
    jmp .dec_done

.push_digits:
    test ebx, ebx
    jz .pop_digits
    mov eax, ebx
    xor edx, edx
    mov ebx, 10
    div ebx                     ; eax = quotient, edx = remainder
    mov ebx, eax
    push edx                    ; Push digit
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

; =============================================================================
; vga_set_color - Set the current text color
; Input: al = color attribute (bg << 4 | fg)
; =============================================================================
vga_set_color:
    mov [vga_color], al
    ret

; =============================================================================
; vga_update_cursor - Update hardware cursor position
; =============================================================================
vga_update_cursor:
    push eax
    push ebx
    push edx

    ; Calculate linear position
    mov eax, [vga_row]
    imul eax, VGA_WIDTH
    add eax, [vga_col]
    mov ebx, eax

    ; Set low byte
    mov dx, 0x3D4
    mov al, 0x0F
    out dx, al
    mov dx, 0x3D5
    mov al, bl
    out dx, al

    ; Set high byte
    mov dx, 0x3D4
    mov al, 0x0E
    out dx, al
    mov dx, 0x3D5
    mov al, bh
    out dx, al

    pop edx
    pop ebx
    pop eax
    ret

; =============================================================================
; vga_get_cursor_row / vga_get_cursor_col
; =============================================================================
vga_get_cursor_row:
    mov eax, [vga_row]
    ret

vga_get_cursor_col:
    mov eax, [vga_col]
    ret
