; =============================================================================
; AiOS - Intent Syscall Handler (INT 0x80)
; The AI expresses INTENT; the kernel fulfills it based on context.
;
; Calling convention (from AI sandbox / C code):
;   EAX = Intent ID (syscall number)
;   EBX = Arg1 (varies by intent)
;   ECX = Arg2 (varies by intent)
;   EDX = Arg3 (varies by intent)
;
; Returns:
;   EAX = 0 on success, negative on error
;
; Intent Table:
;   0x01 INTENT_RENDER   - Display text/data to user
;   0x02 INTENT_NOTIFY   - Alert the user (urgency-aware)
;   0x03 INTENT_RECALL   - Query stored knowledge (future)
;   0x04 INTENT_MEMORIZE - Store data persistently (future)
;   0x05 INTENT_COMM     - Send a message externally (future)
;   0x06 INTENT_QUERY    - Ask the remote AI model
;   0x07 INTENT_SETCTX   - Modify a context frame field
;   0x08 INTENT_GETCTX   - Read the context frame
;   0x09 INTENT_SETLAYOUT- Switch keyboard layout
; =============================================================================

%include "include/constants.inc"

; Intent IDs
INTENT_RENDER     equ 0x01
INTENT_NOTIFY     equ 0x02
INTENT_RECALL     equ 0x03
INTENT_MEMORIZE   equ 0x04
INTENT_COMM       equ 0x05
INTENT_QUERY      equ 0x06
INTENT_SETCTX     equ 0x07
INTENT_GETCTX     equ 0x08
INTENT_SETLAYOUT  equ 0x09

section .data

intent_not_impl_msg: db '[Intent] Not yet implemented (ID=0x', 0
intent_not_impl_end: db ')', 10, 0
intent_notify_prefix_low:  db '[Info] ', 0
intent_notify_prefix_med:  db '[Note] ', 0
intent_notify_prefix_high: db '[ALERT] ', 0

section .text

global syscall_handler
global syscall_init

extern vga_print
extern vga_putchar
extern vga_newline
extern vga_print_hex
extern vga_set_color
extern context_frame
extern context_update
extern keyboard_set_layout
extern claude_ask

; =============================================================================
; syscall_init - Register INT 0x80 in IDT
; Called from idt_init or kernel_main
; =============================================================================
syscall_init:
    ; Install INT 0x80 handler via idt_set_gate
    push dword 0x80
    push dword _int80_entry
    call idt_set_gate_ring3
    add esp, 8
    ret

extern idt_set_gate

; idt_set_gate_ring3 - Same as idt_set_gate but with DPL=3 (user-callable)
; [esp+4] = handler, [esp+8] = interrupt number
idt_set_gate_ring3:
    push ebp
    mov ebp, esp
    push ebx
    push eax

    mov eax, [ebp+8]           ; Handler address
    mov ebx, [ebp+12]          ; Interrupt number

    shl ebx, 3
    add ebx, idt_start

    ; Low 16 bits of handler
    mov word [ebx], ax

    ; Code segment selector
    mov word [ebx+2], 0x08     ; CODE_SEG

    ; Reserved byte
    mov byte [ebx+4], 0

    ; Type: Present, DPL=3, 32-bit interrupt gate (11101110b = 0xEE)
    mov byte [ebx+5], 11101110b

    ; High 16 bits of handler
    shr eax, 16
    mov word [ebx+6], ax

    pop eax
    pop ebx
    pop ebp
    ret

extern idt_start            ; Import IDT table base from idt.asm

; =============================================================================
; _int80_entry - INT 0x80 entry point (interrupt context)
; =============================================================================
_int80_entry:
    pushad

    ; Update context frame before processing intent
    call context_update

    ; Record this intent
    mov [context_frame + 0x18], al      ; last_intent_id
    inc dword [context_frame + 0x14]    ; intent_count++

    ; Dispatch by intent ID
    cmp eax, INTENT_RENDER
    je .do_render
    cmp eax, INTENT_NOTIFY
    je .do_notify
    cmp eax, INTENT_QUERY
    je .do_query
    cmp eax, INTENT_SETCTX
    je .do_setctx
    cmp eax, INTENT_GETCTX
    je .do_getctx
    cmp eax, INTENT_SETLAYOUT
    je .do_setlayout

    ; Unimplemented intent
    jmp .not_implemented

; -----------------------------------------------------------------------------
; INTENT_RENDER (0x01) - Display text to user
;   EBX = pointer to null-terminated string
;   ECX = type: 0=plain text, 1=info (green), 2=error (red)
; -----------------------------------------------------------------------------
.do_render:
    ; Check output_mode in context — if Silent, suppress
    cmp byte [context_frame + 0x04], 2
    je .render_silent

    ; Set color based on type
    cmp ecx, 1
    je .render_info
    cmp ecx, 2
    je .render_error

    ; Plain text — default color
    mov al, DEFAULT_COLOR
    jmp .render_print

.render_info:
    mov al, (COLOR_BLACK << 4) | COLOR_LGREEN
    jmp .render_print

.render_error:
    mov al, (COLOR_BLACK << 4) | COLOR_LRED

.render_print:
    call vga_set_color
    mov esi, ebx
    call vga_print
    call vga_newline
    mov al, DEFAULT_COLOR
    call vga_set_color

.render_silent:
    mov byte [context_frame + 0x19], 0  ; no error
    ; Set EAX=0 in the saved pushad frame (offset 28 from ESP)
    mov dword [esp + 28], 0
    jmp .done

; -----------------------------------------------------------------------------
; INTENT_NOTIFY (0x02) - Alert the user (urgency-based)
;   EBX = urgency: 0=low, 1=normal, 2=high
;   EDX = pointer to message string
; -----------------------------------------------------------------------------
.do_notify:
    ; Check output_mode — if Silent AND urgency < 2, suppress
    cmp byte [context_frame + 0x04], 2
    jne .notify_proceed
    cmp ebx, 2
    jl .notify_suppressed

.notify_proceed:
    ; Choose prefix and color based on urgency
    cmp ebx, 2
    je .notify_high
    cmp ebx, 1
    je .notify_med

    ; Low urgency — gray prefix
    mov al, (COLOR_BLACK << 4) | COLOR_DGRAY
    call vga_set_color
    mov esi, intent_notify_prefix_low
    jmp .notify_print

.notify_med:
    mov al, (COLOR_BLACK << 4) | COLOR_YELLOW
    call vga_set_color
    mov esi, intent_notify_prefix_med
    jmp .notify_print

.notify_high:
    mov al, (COLOR_BLACK << 4) | COLOR_LRED
    call vga_set_color
    mov esi, intent_notify_prefix_high

.notify_print:
    call vga_print
    mov esi, edx
    call vga_print
    call vga_newline
    mov al, DEFAULT_COLOR
    call vga_set_color

.notify_suppressed:
    mov byte [context_frame + 0x19], 0
    mov dword [esp + 28], 0
    jmp .done

; -----------------------------------------------------------------------------
; INTENT_QUERY (0x06) - Ask the remote AI model
;   EBX = pointer to question string
;   ECX = pointer to response buffer
;   EDX = max response length
; -----------------------------------------------------------------------------
.do_query:
    ; Check if network is available
    cmp byte [context_frame + 0x07], 2
    jne .query_no_net

    ; Check privacy level — if Restricted, block remote queries
    cmp byte [context_frame + 0x02], 2
    je .query_blocked

    ; Call claude_ask(question, response, max_len) via cdecl
    push edx            ; arg3: max_len
    push ecx            ; arg2: response buffer
    push ebx            ; arg1: question
    call claude_ask
    add esp, 12

    ; Return value already in eax
    cmp eax, 0
    jl .query_error

    mov byte [context_frame + 0x19], 0
    mov dword [esp + 28], eax       ; return bytes written
    jmp .done

.query_no_net:
    mov byte [context_frame + 0x19], 1  ; error: no network
    mov dword [esp + 28], -1
    jmp .done

.query_blocked:
    mov byte [context_frame + 0x19], 2  ; error: privacy restricted
    mov dword [esp + 28], -2
    jmp .done

.query_error:
    mov byte [context_frame + 0x19], 3  ; error: API failure
    mov dword [esp + 28], eax           ; pass through negative error
    jmp .done

; -----------------------------------------------------------------------------
; INTENT_SETCTX (0x07) - Modify a context frame field
;   EBX = field offset (0x00-0x1F)
;   ECX = new value (byte)
;   Policy: only certain fields are writable by intent
; -----------------------------------------------------------------------------
.do_setctx:
    ; Validate offset range
    cmp ebx, 0x20
    jge .setctx_denied

    ; Only allow writing to: output_mode (0x04), ai_tier (0x05),
    ; privacy_level (0x02), user_state (0x00)
    cmp ebx, 0x00
    je .setctx_allowed
    cmp ebx, 0x02
    je .setctx_allowed
    cmp ebx, 0x04
    je .setctx_allowed
    cmp ebx, 0x05
    je .setctx_allowed
    jmp .setctx_denied

.setctx_allowed:
    mov [context_frame + ebx], cl
    mov byte [context_frame + 0x19], 0
    mov dword [esp + 28], 0
    jmp .done

.setctx_denied:
    mov byte [context_frame + 0x19], 0xFF  ; permission denied
    mov dword [esp + 28], -1
    jmp .done

; -----------------------------------------------------------------------------
; INTENT_GETCTX (0x08) - Read the full context frame
;   EBX = pointer to destination buffer (must be >= 32 bytes)
; -----------------------------------------------------------------------------
.do_getctx:
    ; Copy context frame to caller's buffer
    mov esi, context_frame
    mov edi, ebx
    mov ecx, CONTEXT_SIZE
    rep movsb

    mov byte [context_frame + 0x19], 0
    mov dword [esp + 28], 0
    jmp .done

; -----------------------------------------------------------------------------
; INTENT_SETLAYOUT (0x09) - Switch keyboard layout
;   EBX = layout ID
; -----------------------------------------------------------------------------
.do_setlayout:
    push ebx
    call keyboard_set_layout
    add esp, 4

    cmp eax, 0
    jl .layout_error

    mov byte [context_frame + 0x19], 0
    mov dword [esp + 28], 0
    jmp .done

.layout_error:
    mov byte [context_frame + 0x19], 4  ; invalid layout
    mov dword [esp + 28], -1
    jmp .done

; -----------------------------------------------------------------------------
; Not implemented
; -----------------------------------------------------------------------------
.not_implemented:
    mov byte [context_frame + 0x19], 0xFE
    mov dword [esp + 28], -1

.done:
    popad
    iret

CONTEXT_SIZE equ 32
