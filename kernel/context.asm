; =============================================================================
; AiOS - Context Frame
; A structured memory region the kernel maintains as "ground truth"
; The AI reads this; only the kernel writes it.
; =============================================================================

%include "include/constants.inc"

section .data

; =============================================================================
; Context Frame Layout (at context_frame label)
; Offsets are documented for C access via include/context.h
;
;   +0x00  user_state       (1 byte)  - 0=Idle, 1=Active, 2=Busy
;   +0x01  activity         (1 byte)  - 0=Desktop, 1=Shell, 2=AI_Prompt
;   +0x02  privacy_level    (1 byte)  - 0=Open, 1=Standard, 2=Restricted
;   +0x03  power_state      (1 byte)  - 0=Battery_Low, 1=Normal, 2=Wall_Power
;   +0x04  output_mode      (1 byte)  - 0=Text, 1=Minimal, 2=Silent
;   +0x05  ai_tier          (1 byte)  - 0=Local_SLM, 1=Local_Pro, 2=Remote
;   +0x06  kbd_layout       (1 byte)  - Current keyboard layout ID
;   +0x07  net_status       (1 byte)  - 0=Down, 1=DHCP_Pending, 2=Up
;   +0x08  uptime_secs      (4 bytes) - Seconds since boot
;   +0x0C  total_memory_kb  (4 bytes) - Total RAM in KB
;   +0x10  free_heap_bytes  (4 bytes) - Free heap space
;   +0x14  intent_count     (4 bytes) - Number of intents processed
;   +0x18  last_intent_id   (1 byte)  - Last intent syscall ID
;   +0x19  last_intent_err  (1 byte)  - 0=OK, nonzero=error code
;   +0x1A  reserved         (6 bytes) - Future use
;   +0x20  [end: 32 bytes total]
; =============================================================================

global context_frame
global context_init
global context_update

context_frame:
    ; User State
    .user_state:     db 0       ; +0x00: Idle
    .activity:       db 2       ; +0x01: AI_Prompt (default boot mode)
    .privacy_level:  db 1       ; +0x02: Standard
    .power_state:    db 2       ; +0x03: Wall_Power (VM)
    .output_mode:    db 0       ; +0x04: Text
    .ai_tier:        db 2       ; +0x05: Remote (using API)
    .kbd_layout:     db 0       ; +0x06: US QWERTY default
    .net_status:     db 0       ; +0x07: Down (updated by kernel)
    .uptime_secs:    dd 0       ; +0x08
    .total_mem_kb:   dd 0       ; +0x0C
    .free_heap:      dd 0       ; +0x10
    .intent_count:   dd 0       ; +0x14
    .last_intent_id: db 0       ; +0x18
    .last_intent_err:db 0       ; +0x19
    .reserved:       times 6 db 0 ; +0x1A-0x1F

CONTEXT_SIZE equ 32

section .text

extern timer_get_uptime_secs
extern memory_get_total
extern keyboard_get_layout
extern net_is_up

; =============================================================================
; context_init - Initialize the context frame with static values
; Called once during boot
; =============================================================================
context_init:
    ; Total memory (stays constant)
    call memory_get_total
    mov [context_frame + 0x0C], eax

    ; Set power state to wall power (VM)
    mov byte [context_frame + 0x03], 2

    ; Set default activity to AI prompt
    mov byte [context_frame + 0x01], 2

    ret

; =============================================================================
; context_update - Refresh dynamic fields in the context frame
; Called periodically (e.g., from timer or before intent processing)
; =============================================================================
context_update:
    push eax

    ; Update uptime
    call timer_get_uptime_secs
    mov [context_frame + 0x08], eax

    ; Update keyboard layout
    call keyboard_get_layout
    mov [context_frame + 0x06], al

    ; Update network status
    call net_is_up
    test eax, eax
    jz .net_down
    mov byte [context_frame + 0x07], 2      ; Up
    jmp .done
.net_down:
    ; Could be 0 (down) or 1 (DHCP pending), simplify to 0
    mov byte [context_frame + 0x07], 0

.done:
    pop eax
    ret
