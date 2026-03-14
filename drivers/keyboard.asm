; =============================================================================
; AsmOS - PS/2 Keyboard Driver
; Scancode Set 1, with shift/caps lock support
; Switchable keyboard layouts (US QWERTY, German QWERTZ, etc.)
; =============================================================================

%include "include/constants.inc"

section .data

; --- Layout 0: US QWERTY (lowercase) ---
layout_us_lower:
    db 0, 27                                    ; 0x00-0x01: none, ESC
    db '1234567890-='                            ; 0x02-0x0D
    db 8, 9                                      ; 0x0E-0x0F: backspace, tab
    db 'qwertyuiop[]'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D: enter, left ctrl
    db 'asdfghjkl', 0x3B, 0x27                   ; 0x1E-0x28
    db '`', 0                                    ; 0x29-0x2A: `, left shift
    db 0x5C, 'zxcvbnm,./'                        ; 0x2B-0x35
    db 0, '*', 0, ' '                            ; 0x36-0x39: rshift, *, lalt, space
    times (128 - 58) db 0

; --- Layout 0: US QWERTY (shifted) ---
layout_us_upper:
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

; --- Layout 1: German QWERTZ (lowercase) ---
; CP437 codes: ä=0x84, ö=0x94, ü=0x81, ß=0xE1
layout_de_lower:
    db 0, 27                                    ; 0x00-0x01: none, ESC
    db '1234567890', 0xE1, 0x27                 ; 0x02-0x0D: ...ß ´
    db 8, 9                                      ; 0x0E-0x0F: backspace, tab
    db 'qwertzuiop', 0x81, '+'                  ; 0x10-0x1B: ...ü +
    db 10, 0                                     ; 0x1C-0x1D: enter, left ctrl
    db 'asdfghjkl', 0x94, 0x84                  ; 0x1E-0x28: ...ö ä
    db '^', 0                                    ; 0x29-0x2A: ^, left shift
    db '#yxcvbnm,.-'                             ; 0x2B-0x35: # yxcvbnm,.-
    db 0, '*', 0, ' '                            ; 0x36-0x39: rshift, *, lalt, space
    times (128 - 58) db 0

; --- Layout 1: German QWERTZ (shifted) ---
; CP437 codes: Ä=0x8E, Ö=0x99, Ü=0x9A, §=0x15
layout_de_upper:
    db 0, 27                                     ; 0x00-0x01
    db '!"', 0x15, '$%&/()=?`'                  ; 0x02-0x0D: !"§$%&/()=?`
    db 8, 9                                      ; 0x0E-0x0F
    db 'QWERTZUIOP', 0x9A, '*'                  ; 0x10-0x1B: ...Ü *
    db 10, 0                                     ; 0x1C-0x1D
    db 'ASDFGHJKL', 0x99, 0x8E                  ; 0x1E-0x28: ...Ö Ä
    db 0xF8, 0                                   ; 0x29-0x2A: ° (0xF8 in CP437), lshift
    db 0x27, 'YXCVBNM;:_'                       ; 0x2B-0x35: ' YXCVBNM;:_
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

; --- Layout 2: French AZERTY (lowercase) ---
; CP437 codes: é=0x82, è=0x8A, ù=0x97, ç=0x87
layout_fr_lower:
    db 0, 27                                    ; 0x00-0x01
    db '&', 0x82, '"', 0x27, '(', '-', 0x8A, '_', 0x87, 0x85, ')', '='  ; 0x02-0x0D
    db 8, 9                                      ; 0x0E-0x0F
    db 'azertyuiop^$'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D
    db 'qsdfghjklm', 0x97                       ; 0x1E-0x28: ...m ù
    db 0, 0                                      ; 0x29-0x2A
    db '*wxcvbn,;:!'                             ; 0x2B-0x35
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

; --- Layout 2: French AZERTY (shifted) ---
layout_fr_upper:
    db 0, 27                                     ; 0x00-0x01
    db '1234567890', 0xF8, '+'                   ; 0x02-0x0D: ...° +
    db 8, 9                                      ; 0x0E-0x0F
    db 'AZERTYUIOP', 0x22, 0x9C                 ; 0x10-0x1B: ..." £
    db 10, 0                                     ; 0x1C-0x1D
    db 'QSDFGHJKLM%'                             ; 0x1E-0x28
    db 0, 0                                      ; 0x29-0x2A
    db 0xE6, 'WXCVBN?./+'                       ; 0x2B-0x35: µ WXCVBN?./+
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

; --- Layout 3: Spanish QWERTY (lowercase) ---
; CP437 codes: ñ=0xA4, ¡=0xAD, ¿=0xA8
layout_es_lower:
    db 0, 27                                    ; 0x00-0x01
    db '1234567890', 0x27, 0xAD                 ; 0x02-0x0D: ...' ¡
    db 8, 9                                      ; 0x0E-0x0F
    db 'qwertyuiop`+'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D
    db 'asdfghjkl', 0xA4, 0x27                  ; 0x1E-0x28: ...ñ '
    db 0, 0                                      ; 0x29-0x2A
    db 0x5C, 'zxcvbnm,./'                       ; 0x2B-0x35
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

; --- Layout 3: Spanish QWERTY (shifted) ---
layout_es_upper:
    db 0, 27                                     ; 0x00-0x01
    db '!"#$%&/()=', 0xA8, 0x22                 ; 0x02-0x0D: ...¿ "
    db 8, 9                                      ; 0x0E-0x0F
    db 'QWERTYUIOP^*'                            ; 0x10-0x1B
    db 10, 0                                     ; 0x1C-0x1D
    db 'ASDFGHJKL', 0xA5, '"'                   ; 0x1E-0x28: ...Ñ "
    db 0, 0                                      ; 0x29-0x2A
    db '|ZXCVBNM<>?'                             ; 0x2B-0x35
    db 0, '*', 0, ' '                            ; 0x36-0x39
    times (128 - 58) db 0

; Layout table pointers: [lower_ptr, upper_ptr] for each layout
; Layout IDs: 0 = US, 1 = DE, 2 = FR, 3 = ES
layout_table:
    dd layout_us_lower, layout_us_upper     ; Layout 0: US
    dd layout_de_lower, layout_de_upper     ; Layout 1: DE
    dd layout_fr_lower, layout_fr_upper     ; Layout 2: FR
    dd layout_es_lower, layout_es_upper     ; Layout 3: ES

; Layout name strings (for display)
layout_names:
    dd layout_name_us
    dd layout_name_de
    dd layout_name_fr
    dd layout_name_es

layout_name_us: db 'US QWERTY', 0
layout_name_de: db 'German QWERTZ', 0
layout_name_fr: db 'French AZERTY', 0
layout_name_es: db 'Spanish QWERTY', 0

; Number of available layouts
NUM_LAYOUTS equ 4

section .bss
; Keyboard state
kbd_shift:      resb 1
kbd_caps:       resb 1
kbd_ctrl:       resb 1

; Current layout ID (0 = US, 1 = DE, ...)
kbd_layout:     resd 1

; Active layout table pointers (set by keyboard_set_layout)
kbd_lower_ptr:  resd 1          ; Pointer to current lowercase table
kbd_upper_ptr:  resd 1          ; Pointer to current uppercase/shift table

; Extended key state (0xE0 prefix)
kbd_extended:   resb 1

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
global keyboard_set_layout
global keyboard_get_layout
global keyboard_get_layout_name
global keyboard_get_num_layouts
global keyboard_inject_char

extern system_poll
extern fb_page_up
extern fb_page_down
extern win_poll

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
    mov byte [kbd_extended], 0

    ; Default to layout 0 (US QWERTY)
    mov dword [kbd_layout], 0
    mov dword [kbd_lower_ptr], layout_us_lower
    mov dword [kbd_upper_ptr], layout_us_upper

    ; Flush keyboard buffer
    in al, KBD_DATA
    in al, KBD_DATA
    ret

; =============================================================================
; keyboard_set_layout - Switch keyboard layout at runtime
; Called from C: void keyboard_set_layout(int layout_id)
; Input: layout_id on stack (cdecl)
; Returns: 0 on success, -1 on invalid layout
; =============================================================================
keyboard_set_layout:
    push ebp
    mov ebp, esp
    push ebx

    mov eax, [ebp+8]               ; layout_id argument

    ; Validate range
    cmp eax, NUM_LAYOUTS
    jge .invalid_layout
    cmp eax, 0
    jl .invalid_layout

    ; Store layout ID
    mov [kbd_layout], eax

    ; Calculate offset into layout_table: eax * 8 (two dwords per layout)
    shl eax, 3
    mov ebx, [layout_table + eax]       ; lower table pointer
    mov [kbd_lower_ptr], ebx
    mov ebx, [layout_table + eax + 4]   ; upper table pointer
    mov [kbd_upper_ptr], ebx

    xor eax, eax                    ; return 0 (success)
    pop ebx
    pop ebp
    ret

.invalid_layout:
    mov eax, -1
    pop ebx
    pop ebp
    ret

; =============================================================================
; keyboard_get_layout - Get current layout ID
; Called from C: int keyboard_get_layout(void)
; Returns: layout ID in eax
; =============================================================================
keyboard_get_layout:
    mov eax, [kbd_layout]
    ret

; =============================================================================
; keyboard_get_layout_name - Get name string for a layout
; Called from C: const char* keyboard_get_layout_name(int layout_id)
; Returns: pointer to name string, or NULL if invalid
; =============================================================================
keyboard_get_layout_name:
    push ebp
    mov ebp, esp

    mov eax, [ebp+8]
    cmp eax, NUM_LAYOUTS
    jge .name_invalid
    cmp eax, 0
    jl .name_invalid

    mov eax, [layout_names + eax*4]
    pop ebp
    ret

.name_invalid:
    xor eax, eax
    pop ebp
    ret

; =============================================================================
; keyboard_get_num_layouts - Get number of available layouts
; Called from C: int keyboard_get_num_layouts(void)
; Returns: number of layouts
; =============================================================================
keyboard_get_num_layouts:
    mov eax, NUM_LAYOUTS
    ret

; =============================================================================
; keyboard_handler - Called from IRQ1 interrupt
; Reads scancode, translates to ASCII using active layout, stores in buffer
; =============================================================================
keyboard_handler:
    push eax
    push ebx
    push ecx

    ; Read scancode
    in al, KBD_DATA
    mov bl, al

    ; Check for extended key prefix (0xE0)
    cmp bl, 0xE0
    je .set_extended

    ; Check if this is the second byte of an extended key
    cmp byte [kbd_extended], 1
    je .handle_extended

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
    ; Caps lock only affects letters — use lower table to check
    mov ecx, [kbd_lower_ptr]
    mov al, [ecx + ebx]
    cmp al, 'a'
    jl .use_normal_table
    cmp al, 'z'
    jg .use_normal_table
    jmp .use_shift_table

.use_shift_table:
    mov ecx, [kbd_upper_ptr]
    mov al, [ecx + ebx]
    jmp .check_valid

.use_normal_table:
    mov ecx, [kbd_lower_ptr]
    mov al, [ecx + ebx]

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

.set_extended:
    mov byte [kbd_extended], 1
    jmp .done

.handle_extended:
    mov byte [kbd_extended], 0
    ; Ignore extended key releases (bit 7 set)
    test bl, 0x80
    jnz .done
    ; Page Up = 0x49, Page Down = 0x51
    cmp bl, 0x49
    je .do_page_up
    cmp bl, 0x51
    je .do_page_down
    jmp .done

.do_page_up:
    push edx
    call fb_page_up
    pop edx
    jmp .done

.do_page_down:
    push edx
    call fb_page_down
    pop edx
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
    call system_poll            ; Network + scheduler + periodic tasks
    call win_poll               ; Update window manager (mouse, rendering)
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
; keyboard_inject_char - Inject a character into the keyboard buffer (from USB)
; Called from C: void keyboard_inject_char(char c)
; =============================================================================
keyboard_inject_char:
    push ebp
    mov ebp, esp
    push ebx

    mov al, [ebp+8]            ; character to inject
    test al, al
    jz .inject_done

    ; Store in circular buffer if not full
    cmp dword [kbd_buf_count], KBD_BUF_SIZE
    jge .inject_done

    mov ebx, [kbd_buf_head]
    mov [kbd_buffer + ebx], al

    inc ebx
    and ebx, KBD_BUF_SIZE - 1
    mov [kbd_buf_head], ebx
    inc dword [kbd_buf_count]

.inject_done:
    pop ebx
    pop ebp
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
