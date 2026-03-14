; =============================================================================
; AsmOS - PIT Timer Driver
; Programmable Interval Timer, Channel 0 at ~100 Hz
; =============================================================================

%include "include/constants.inc"

section .data
global timer_ticks
timer_ticks: dd 0

section .text
global timer_init
global timer_handler
global timer_get_ticks
global timer_get_uptime_secs

; =============================================================================
; timer_init - Initialize PIT Channel 0
; =============================================================================
timer_init:
    push eax

    ; Calculate divisor: 1193180 / 100 = 11931
    mov eax, PIT_FREQ / PIT_HZ

    ; Send command byte: Channel 0, lobyte/hibyte, square wave, binary
    push eax
    mov al, 0x36                ; 00 11 011 0
    out PIT_CMD, al
    pop eax

    ; Send divisor
    out PIT_CH0, al             ; Low byte
    mov al, ah
    out PIT_CH0, al             ; High byte

    mov dword [timer_ticks], 0
    pop eax
    ret

; =============================================================================
; timer_handler - Called from IRQ0 interrupt
; Sets scheduler_due flag once per second for main loop to act on.
; =============================================================================
global scheduler_due
section .bss
scheduler_due: resd 1

section .text
timer_handler:
    inc dword [timer_ticks]

    ; Every 100 ticks (1 second), set scheduler_due flag
    mov eax, [timer_ticks]
    xor edx, edx
    mov ecx, PIT_HZ
    div ecx
    test edx, edx
    jnz .no_flag
    mov dword [scheduler_due], 1
.no_flag:
    ret

; =============================================================================
; timer_get_ticks - Get current tick count
; Output: eax = tick count
; =============================================================================
timer_get_ticks:
    mov eax, [timer_ticks]
    ret

; =============================================================================
; timer_get_uptime_secs - Get uptime in seconds
; Output: eax = seconds since boot
; =============================================================================
timer_get_uptime_secs:
    mov eax, [timer_ticks]
    xor edx, edx
    mov ecx, PIT_HZ
    div ecx                     ; eax = ticks / 100 = seconds
    ret
