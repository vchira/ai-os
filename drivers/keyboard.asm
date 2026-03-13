; =============================================================================
; AsmOS - PS/2 Keyboard Driver
; Scancode Set 1, with shift/caps lock support
; =============================================================================

%include "include/constants.inc"

section .data

; Scancode to ASCII lookup table (US QWERTY, lowercase)
scancode_table:
    db 0, 27                                    ; 0x00-0x01: none, ESC
    db '1234567890-='                            ; 0x02-0x0D
    db 8, 9                                      ; 0x0E-0x0F: backspace, tab
    db 'qwertyuiop[]'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D: enter, left ctrl
    db 'asdfghjkl', 0x3B, 0x27                   ; 0x1E-0x28: ;'
    db '`', 0                                    ; 0x29-0x2A: `, left shift
    db 0x5C, 'zxcvbnm,./'                        ; 0x2B-0x35: \, z..
    db 0, '*', 0, ' '                            ; 0x36-0x39: rshift, *, lalt, space
    times (128 - 58) db 0                        ; Fill rest

; Shifted scancode table
scancode_shift_table:
    db 0, 27                                     ; 0x00-0x01
    db '!@#$%^&*()_+'                            ; 0x02-0x0D
    db 8, 9                                      ; 0x0E-0x0F
    db 'QWERTYUIOP{}'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D
    db 'ASDFGHJKL:"'                             ; 0x1E-0x28
    db '~', 0                                    ; 0x29-0x2A
    db '|ZXCVBNM<>?'                             ; 0x2B-0x35
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

section .bss
; Keyboard state
kbd_shift:      resb 1
kbd_caps:       resb 1
kbd_ctrl:       resb 1

; Circular input buffer
kbd_buffer:     resb KBD_BUF_SIZE
kbd_buf_head:   resd 1          ; Write index
kbd_buf_tail:   resd 1          ; Read index
kbd_buf_count:  resd 1          ; Number of chars in buffer

section .text
global keyboard_init
global keyboard_handler
global keyboard_getchar
global keyboard_has_input

extern net_poll

; =============================================================================
; keyboard_init - Initialize keyboard driver
; =============================================================================
keyboard_init:
    mov byte [kbd_shift], 0
    mov byte [kbd_caps], 0
    mov byte [kbd_ctrl], 0
    mov dword [kbd_buf_head], 0
    mov dword [kbd_buf_tail], 0
    mov dword [kbd_buf_count], 0

    ; Flush keyboard buffer
    in al, KBD_DATA
    in al, KBD_DATA
    ret

; =============================================================================
; keyboard_handler - Called from IRQ1 interrupt
; Reads scancode, translates to ASCII, stores in buffer
; =============================================================================
keyboard_handler:
    push eax
    push ebx
    push ecx

    ; Read scancode
    in al, KBD_DATA
    mov bl, al

    ; Check for key release (bit 7 set)
    test bl, 0x80
    jnz .key_release

    ; Key press
    ; Check for modifier keys
    cmp bl, 0x2A                ; Left shift press
    je .shift_press
    cmp bl, 0x36                ; Right shift press
    je .shift_press
    cmp bl, 0x1D                ; Left ctrl press
    je .ctrl_press
    cmp bl, 0x3A                ; Caps lock press
    je .caps_toggle

    ; Regular key - translate scancode to ASCII
    movzx ebx, bl
    cmp ebx, 58                 ; Only handle scancodes < 58
    jge .done

    ; Check shift state
    mov cl, [kbd_shift]
    mov ch, [kbd_caps]

    ; Determine which table to use
    test cl, cl
    jnz .use_shift_table
    test ch, ch
    jnz .check_caps_alpha
    jmp .use_normal_table

.check_caps_alpha:
    ; Caps lock only affects letters
    mov al, [scancode_table + ebx]
    cmp al, 'a'
    jl .use_normal_table
    cmp al, 'z'
    jg .use_normal_table
    jmp .use_shift_table

.use_shift_table:
    mov al, [scancode_shift_table + ebx]
    jmp .check_valid

.use_normal_table:
    mov al, [scancode_table + ebx]

.check_valid:
    test al, al
    jz .done

    ; Check for ctrl combinations
    mov cl, [kbd_ctrl]
    test cl, cl
    jz .store_char

    ; Ctrl+C = 3, Ctrl+L = 12, etc.
    cmp al, 'a'
    jl .store_char
    cmp al, 'z'
    jg .store_char
    sub al, 96                  ; Convert to control character
    jmp .store_char

.store_char:
    ; Store in circular buffer if not full
    cmp dword [kbd_buf_count], KBD_BUF_SIZE
    jge .done

    mov ebx, [kbd_buf_head]
    mov [kbd_buffer + ebx], al

    ; Advance head
    inc ebx
    and ebx, KBD_BUF_SIZE - 1  ; Wrap around
    mov [kbd_buf_head], ebx
    inc dword [kbd_buf_count]
    jmp .done

.key_release:
    and bl, 0x7F                ; Clear release bit
    cmp bl, 0x2A                ; Left shift release
    je .shift_release
    cmp bl, 0x36                ; Right shift release
    je .shift_release
    cmp bl, 0x1D                ; Left ctrl release
    je .ctrl_release
    jmp .done

.shift_press:
    mov byte [kbd_shift], 1
    jmp .done

.shift_release:
    mov byte [kbd_shift], 0
    jmp .done

.ctrl_press:
    mov byte [kbd_ctrl], 1
    jmp .done

.ctrl_release:
    mov byte [kbd_ctrl], 0
    jmp .done

.caps_toggle:
    xor byte [kbd_caps], 1
    jmp .done

.done:
    pop ecx
    pop ebx
    pop eax
    ret

; =============================================================================
; keyboard_getchar - Get a character from the keyboard buffer (blocking)
; Output: al = ASCII character
; =============================================================================
keyboard_getchar:
.wait:
    call net_poll               ; Process network packets while idle
    hlt                         ; Wait for interrupt
    cmp dword [kbd_buf_count], 0
    je .wait

    ; Read from tail
    push ebx
    mov ebx, [kbd_buf_tail]
    mov al, [kbd_buffer + ebx]

    ; Advance tail
    inc ebx
    and ebx, KBD_BUF_SIZE - 1
    mov [kbd_buf_tail], ebx
    dec dword [kbd_buf_count]

    pop ebx
    ret

; =============================================================================
; keyboard_has_input - Check if keyboard buffer has data
; Output: eax = 1 if data available, 0 if not
; =============================================================================
keyboard_has_input:
    mov eax, [kbd_buf_count]
    test eax, eax
    jz .empty
    mov eax, 1
    ret
.empty:
    xor eax, eax
    ret
