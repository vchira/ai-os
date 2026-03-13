; =============================================================================
; AiOS - AI Prompt
; Default interface — everything typed is sent to Claude AI
; Slash commands (e.g., /shell) trigger special actions
; =============================================================================

%include "include/constants.inc"

section .data

; AI prompt
ai_prompt_str:  db 10, 'ai> ', 0

; Welcome message
ai_welcome:     db 'Welcome to AiOS. Type anything to talk to AI.', 10
                db 'Use /help to see available commands.', 10, 0

; Slash command strings
slash_help:     db '/help', 0
slash_shell:    db '/shell', 0
slash_clear:    db '/clear', 0
slash_net:      db '/net', 0
slash_pci:      db '/pci', 0
slash_reboot:   db '/reboot', 0
slash_halt:     db '/halt', 0
slash_ver:      db '/ver', 0
slash_color:    db '/color', 0
slash_uptime:   db '/uptime', 0
slash_meminfo:  db '/meminfo', 0
slash_cpuinfo:  db '/cpuinfo', 0
slash_keyboard: db '/keyboard', 0
slash_provider: db '/provider', 0

; Help text
ai_help_text:
    db '  /help    - Show this help message', 10
    db '  /shell   - Enter command shell (type "exit" to return)', 10
    db '  /clear   - Clear the screen', 10
    db '  /net     - Show network status', 10
    db '  /pci     - List PCI devices', 10
    db '  /meminfo - Display memory information', 10
    db '  /cpuinfo - Display CPU information', 10
    db '  /uptime  - Show system uptime', 10
    db '  /color   - Change text color (usage: /color <0-F>)', 10
    db '  /ver     - Show OS version', 10
    db '  /reboot  - Reboot the system', 10
    db '  /keyboard - Switch keyboard layout', 10
    db '  /provider - Switch AI provider (Claude/OpenAI/Ollama)', 10
    db '  /halt    - Halt the CPU', 10
    db 10
    db '  Anything else is sent directly to the active AI.', 10, 0

; Thinking indicator
ai_thinking:    db '[AI] Thinking...', 10, 0
ai_err_msg:     db '[Error] Claude API request failed', 10, 0

; Shell messages
shell_enter_msg: db 'Entering shell. Type "exit" to return to AI prompt.', 10, 0
shell_exit_msg:  db 'Returned to AI prompt.', 10, 0

; Unknown slash command
ai_unknown_cmd: db 'Unknown command: ', 0
ai_use_help:    db 10, "Type '/help' for a list of commands.", 10, 0

; Net command strings
ai_net_ip_msg:   db 'IP: ', 0
ai_net_noip_msg: db 'No IP address (DHCP pending)', 10, 0

; Version strings
ai_ver_text:    db 'AiOS v0.1 - AI-Native Operating System', 10
                db 'The Deterministic Substrate', 10
                db 'Built with NASM + GNU ld + GRUB2', 10, 0

; Halt message
ai_halt_msg:    db 'System halted. You can safely power off.', 10, 0

; Reboot message
ai_reboot_msg:  db 'Rebooting...', 10, 0

; Memory info strings
ai_meminfo_total: db 'Total memory: ', 0
ai_meminfo_kb:    db ' KB (', 0
ai_meminfo_mb:    db ' MB)', 10, 0

; CPU info strings
ai_cpuinfo_vendor: db 'CPU Vendor: ', 0
ai_cpuinfo_buf:    times 13 db 0

; Uptime strings
ai_uptime_msg:  db 'Uptime: ', 0
ai_uptime_secs: db ' seconds', 10, 0

; Color strings
ai_color_help:  db 'Usage: /color <hex 0-F>', 10
                db 'Colors: 0=Black 1=Blue 2=Green 3=Cyan', 10
                db '  4=Red 5=Magenta 6=Brown 7=LightGray', 10
                db '  8=DarkGray 9=LightBlue A=LightGreen', 10
                db '  B=LightCyan C=LightRed D=LightMagenta', 10
                db '  E=Yellow F=White', 10, 0
ai_color_set:   db 'Color set.', 10, 0

; Keyboard layout strings
ai_kbd_current: db 'Current layout: ', 0
ai_kbd_avail:   db 'Available layouts:', 10, 0
ai_kbd_prefix:  db '  ', 0
ai_kbd_arrow:   db ' <- active', 0
ai_kbd_set_ok:  db 'Keyboard layout changed to: ', 0
ai_kbd_invalid: db 'Invalid layout. Use /keyboard to see available layouts.', 10, 0
ai_kbd_usage:   db 'Usage: /keyboard <number>', 10, 0

; Provider strings
ai_prov_current: db 'Active provider: ', 0
ai_prov_model:   db ' (model: ', 0
ai_prov_model_e: db ')', 10, 0
ai_prov_avail:   db 'Available providers:', 10, 0
ai_prov_nokey:   db ' [no API key]', 0
ai_prov_set_ok:  db 'Switched to: ', 0
ai_prov_invalid: db 'Invalid provider. Use /provider to see list.', 10, 0

section .bss
; Input buffer
ai_input:       resb 256
ai_input_pos:   resd 1

; Response buffer
ai_resp:        resb 4096

; Temp buffer for net_get_ip
ai_net_buf:     resb 32

section .text
global ai_prompt_run

extern vga_print
extern vga_putchar
extern vga_clear
extern vga_newline
extern vga_print_hex
extern vga_print_dec
extern vga_set_color
extern vga_color
extern keyboard_getchar
extern memory_get_total
extern timer_get_uptime_secs
extern llm_ask
extern llm_get_num_providers
extern llm_get_active
extern llm_set_active
extern llm_get_provider_name
extern llm_get_provider_model
extern llm_is_configured
extern net_is_up
extern net_get_ip
extern net_poll
extern pci_scan
extern shell_run_interactive
extern keyboard_set_layout
extern keyboard_get_layout
extern keyboard_get_layout_name
extern keyboard_get_num_layouts

; =============================================================================
; ai_prompt_run - Main AI prompt loop
; =============================================================================
ai_prompt_run:
    ; Print welcome
    mov al, (COLOR_BLACK << 4) | COLOR_LGREEN
    call vga_set_color
    mov esi, ai_welcome
    call vga_print
    mov al, DEFAULT_COLOR
    call vga_set_color

.loop:
    ; Print prompt in light cyan
    mov al, (COLOR_BLACK << 4) | COLOR_LCYAN
    call vga_set_color
    mov esi, ai_prompt_str
    call vga_print
    mov al, DEFAULT_COLOR
    call vga_set_color

    ; Read input
    call ai_readline

    ; Skip empty lines
    cmp byte [ai_input], 0
    je .loop

    ; Check if it's a slash command
    cmp byte [ai_input], '/'
    je .slash_command

    ; Otherwise, send to Claude AI
    call ai_send_to_claude
    jmp .loop

.slash_command:
    call ai_exec_slash
    jmp .loop

; =============================================================================
; ai_readline - Read a line of input
; =============================================================================
ai_readline:
    push eax
    push edi

    mov dword [ai_input_pos], 0

.read_loop:
    call keyboard_getchar

    cmp al, 10                  ; Enter?
    je .line_done

    cmp al, 8                   ; Backspace?
    je .backspace

    cmp al, 127                 ; Delete
    je .backspace

    ; Check buffer overflow
    cmp dword [ai_input_pos], 254
    jge .read_loop

    ; Store character and echo it
    mov edi, [ai_input_pos]
    mov [ai_input + edi], al
    inc dword [ai_input_pos]
    call vga_putchar
    jmp .read_loop

.backspace:
    cmp dword [ai_input_pos], 0
    je .read_loop
    dec dword [ai_input_pos]
    mov al, 8
    call vga_putchar
    jmp .read_loop

.line_done:
    mov edi, [ai_input_pos]
    mov byte [ai_input + edi], 0
    call vga_newline

    pop edi
    pop eax
    ret

; =============================================================================
; ai_send_to_claude - Send ai_input to Claude and print response
; =============================================================================
ai_send_to_claude:
    push eax
    push esi

    ; Print thinking indicator
    mov al, (COLOR_BLACK << 4) | COLOR_DGRAY
    call vga_set_color
    mov esi, ai_thinking
    call vga_print

    ; Call llm_ask(question, response_buf, max_len)
    push dword 4095
    push dword ai_resp
    push dword ai_input
    call llm_ask
    add esp, 12

    ; Check return value
    cmp eax, 0
    jl .ask_error

    ; Print response in light cyan
    mov al, (COLOR_BLACK << 4) | COLOR_LCYAN
    call vga_set_color
    mov esi, ai_resp
    call vga_print
    call vga_newline
    mov al, DEFAULT_COLOR
    call vga_set_color
    jmp .ask_done

.ask_error:
    ; ai_resp contains the detailed error message from claude_ask
    mov al, (COLOR_BLACK << 4) | COLOR_LRED
    call vga_set_color
    mov esi, ai_resp
    call vga_print
    call vga_newline
    mov al, DEFAULT_COLOR
    call vga_set_color

.ask_done:
    pop esi
    pop eax
    ret

; =============================================================================
; ai_exec_slash - Execute a slash command from ai_input
; =============================================================================
ai_exec_slash:
    push eax
    push esi
    push edi

    ; /help
    mov esi, ai_input
    mov edi, slash_help
    call ai_str_startswith
    test eax, eax
    jnz .do_help

    ; /shell
    mov esi, ai_input
    mov edi, slash_shell
    call ai_str_compare
    test eax, eax
    jnz .do_shell

    ; /clear
    mov esi, ai_input
    mov edi, slash_clear
    call ai_str_compare
    test eax, eax
    jnz .do_clear

    ; /net
    mov esi, ai_input
    mov edi, slash_net
    call ai_str_compare
    test eax, eax
    jnz .do_net

    ; /pci
    mov esi, ai_input
    mov edi, slash_pci
    call ai_str_compare
    test eax, eax
    jnz .do_pci

    ; /reboot
    mov esi, ai_input
    mov edi, slash_reboot
    call ai_str_compare
    test eax, eax
    jnz .do_reboot

    ; /halt
    mov esi, ai_input
    mov edi, slash_halt
    call ai_str_compare
    test eax, eax
    jnz .do_halt

    ; /ver
    mov esi, ai_input
    mov edi, slash_ver
    call ai_str_compare
    test eax, eax
    jnz .do_ver

    ; /color
    mov esi, ai_input
    mov edi, slash_color
    call ai_str_startswith
    test eax, eax
    jnz .do_color

    ; /uptime
    mov esi, ai_input
    mov edi, slash_uptime
    call ai_str_compare
    test eax, eax
    jnz .do_uptime

    ; /meminfo
    mov esi, ai_input
    mov edi, slash_meminfo
    call ai_str_compare
    test eax, eax
    jnz .do_meminfo

    ; /cpuinfo
    mov esi, ai_input
    mov edi, slash_cpuinfo
    call ai_str_compare
    test eax, eax
    jnz .do_cpuinfo

    ; /keyboard
    mov esi, ai_input
    mov edi, slash_keyboard
    call ai_str_startswith
    test eax, eax
    jnz .do_keyboard

    ; /provider
    mov esi, ai_input
    mov edi, slash_provider
    call ai_str_startswith
    test eax, eax
    jnz .do_provider

    ; Unknown slash command
    mov esi, ai_unknown_cmd
    call vga_print
    mov esi, ai_input
    call vga_print
    mov esi, ai_use_help
    call vga_print
    jmp .slash_done

.do_help:
    mov esi, ai_help_text
    call vga_print
    jmp .slash_done

.do_shell:
    mov esi, shell_enter_msg
    call vga_print
    call shell_run_interactive  ; Runs shell until user types "exit"
    mov esi, shell_exit_msg
    call vga_print
    jmp .slash_done

.do_clear:
    call vga_clear
    jmp .slash_done

.do_net:
    call net_is_up
    test eax, eax
    jz .net_no_ip

    mov esi, ai_net_ip_msg
    call vga_print
    push dword 32
    push dword ai_net_buf
    call net_get_ip
    add esp, 8
    mov esi, ai_net_buf
    call vga_print
    call vga_newline
    jmp .slash_done

.net_no_ip:
    mov esi, ai_net_noip_msg
    call vga_print
    jmp .slash_done

.do_pci:
    call pci_scan
    jmp .slash_done

.do_reboot:
    mov esi, ai_reboot_msg
    call vga_print
    lidt [.null_idt]
    int 3
    jmp $
.null_idt:
    dw 0
    dd 0

.do_halt:
    mov esi, ai_halt_msg
    call vga_print
    cli
    hlt
    jmp $

.do_ver:
    mov esi, ai_ver_text
    call vga_print
    jmp .slash_done

.do_color:
    ; Parse color value after "/color "
    mov esi, ai_input
    add esi, 6
    cmp byte [esi], ' '
    jne .color_show_help
    inc esi

    mov al, [esi]
    cmp al, 0
    je .color_show_help

    call ai_hex_char_to_val
    cmp al, 0xFF
    je .color_show_help

    mov ah, COLOR_BLACK
    shl ah, 4
    or al, ah
    call vga_set_color

    mov esi, ai_color_set
    call vga_print
    jmp .slash_done

.color_show_help:
    mov esi, ai_color_help
    call vga_print
    jmp .slash_done

.do_uptime:
    mov esi, ai_uptime_msg
    call vga_print
    call timer_get_uptime_secs
    call vga_print_dec
    mov esi, ai_uptime_secs
    call vga_print
    jmp .slash_done

.do_meminfo:
    mov esi, ai_meminfo_total
    call vga_print
    call memory_get_total
    push eax
    call vga_print_dec
    mov esi, ai_meminfo_kb
    call vga_print
    pop eax
    shr eax, 10
    call vga_print_dec
    mov esi, ai_meminfo_mb
    call vga_print
    jmp .slash_done

.do_cpuinfo:
    mov esi, ai_cpuinfo_vendor
    call vga_print
    mov eax, 0
    cpuid
    mov [ai_cpuinfo_buf], ebx
    mov [ai_cpuinfo_buf+4], edx
    mov [ai_cpuinfo_buf+8], ecx
    mov byte [ai_cpuinfo_buf+12], 0
    mov esi, ai_cpuinfo_buf
    call vga_print
    call vga_newline
    jmp .slash_done

.do_keyboard:
    ; Check if argument provided: "/keyboard " is 10 chars
    mov esi, ai_input
    add esi, 9                  ; skip "/keyboard"
    cmp byte [esi], 0
    je .kbd_show_layouts        ; No argument — show list
    cmp byte [esi], ' '
    jne .kbd_show_layouts
    inc esi                     ; skip space

    ; Parse layout number (single digit 0-9)
    mov al, [esi]
    cmp al, '0'
    jl .kbd_invalid
    cmp al, '9'
    jg .kbd_invalid
    sub al, '0'
    movzx eax, al

    ; Call keyboard_set_layout(eax)
    push eax
    call keyboard_set_layout
    add esp, 4
    cmp eax, 0
    jl .kbd_invalid

    ; Print success message with layout name
    mov esi, ai_kbd_set_ok
    call vga_print
    call keyboard_get_layout
    push eax
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call vga_print
    call vga_newline
    jmp .slash_done

.kbd_show_layouts:
    ; Show current layout
    mov esi, ai_kbd_current
    call vga_print
    call keyboard_get_layout
    push eax                    ; save current layout ID
    push eax
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call vga_print
    call vga_newline

    ; List all layouts
    mov esi, ai_kbd_avail
    call vga_print

    call keyboard_get_num_layouts
    mov ecx, eax               ; num layouts
    xor ebx, ebx               ; index = 0
    pop edx                     ; current layout ID

.kbd_list_loop:
    cmp ebx, ecx
    jge .slash_done

    ; Print "  "
    push ecx
    push edx
    mov esi, ai_kbd_prefix
    call vga_print

    ; Print index number
    mov eax, ebx
    call vga_print_dec

    ; Print ": "
    mov al, ':'
    call vga_putchar
    mov al, ' '
    call vga_putchar

    ; Print layout name
    push ebx
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call vga_print

    ; Mark active layout
    pop edx
    pop ecx
    cmp ebx, edx
    jne .kbd_not_active
    mov esi, ai_kbd_arrow
    call vga_print
.kbd_not_active:
    call vga_newline
    inc ebx
    push ecx
    push edx
    pop edx
    pop ecx
    jmp .kbd_list_loop

.kbd_invalid:
    mov esi, ai_kbd_invalid
    call vga_print
    jmp .slash_done

.do_provider:
    ; Check if argument provided: "/provider " is 10 chars
    mov esi, ai_input
    add esi, 9                  ; skip "/provider"
    cmp byte [esi], 0
    je .prov_show_list
    cmp byte [esi], ' '
    jne .prov_show_list
    inc esi                     ; skip space

    ; Parse provider number (single digit 0-9)
    mov al, [esi]
    cmp al, '0'
    jl .prov_invalid
    cmp al, '9'
    jg .prov_invalid
    sub al, '0'
    movzx eax, al

    ; Call llm_set_active(eax)
    push eax
    call llm_set_active
    add esp, 4
    cmp eax, 0
    jl .prov_invalid

    ; Print success
    mov esi, ai_prov_set_ok
    call vga_print
    call llm_get_active
    push eax
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call vga_print
    call vga_newline
    jmp .slash_done

.prov_show_list:
    ; Show current provider
    mov esi, ai_prov_current
    call vga_print
    call llm_get_active
    push eax                    ; save active ID
    push eax
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call vga_print

    ; Show model
    mov esi, ai_prov_model
    call vga_print
    ; active ID still on stack from saved push
    mov eax, [esp]              ; peek at saved active ID
    push eax
    call llm_get_provider_model
    add esp, 4
    mov esi, eax
    call vga_print
    mov esi, ai_prov_model_e
    call vga_print

    ; List all providers
    mov esi, ai_prov_avail
    call vga_print

    call llm_get_num_providers
    mov ecx, eax               ; num providers
    xor ebx, ebx               ; index = 0
    pop edx                     ; active provider ID

.prov_list_loop:
    cmp ebx, ecx
    jge .slash_done

    push ecx
    push edx

    ; Print "  "
    mov esi, ai_kbd_prefix      ; reuse "  " string
    call vga_print

    ; Print index
    mov eax, ebx
    call vga_print_dec

    ; Print ": "
    mov al, ':'
    call vga_putchar
    mov al, ' '
    call vga_putchar

    ; Print provider name
    push ebx
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call vga_print

    ; Print model in parens
    mov esi, ai_prov_model
    call vga_print
    push ebx
    call llm_get_provider_model
    add esp, 4
    mov esi, eax
    call vga_print
    mov al, ')'
    call vga_putchar

    ; Check if configured
    push ebx
    call llm_is_configured
    add esp, 4
    test eax, eax
    jnz .prov_is_configured
    mov esi, ai_prov_nokey
    call vga_print
.prov_is_configured:

    ; Mark active
    pop edx
    pop ecx
    cmp ebx, edx
    jne .prov_not_active
    mov esi, ai_kbd_arrow       ; reuse " <- active"
    call vga_print
.prov_not_active:
    call vga_newline
    inc ebx
    push ecx
    push edx
    pop edx
    pop ecx
    jmp .prov_list_loop

.prov_invalid:
    mov esi, ai_prov_invalid
    call vga_print
    jmp .slash_done

.slash_done:
    pop edi
    pop esi
    pop eax
    ret

; =============================================================================
; ai_str_compare - Compare two null-terminated strings
; =============================================================================
ai_str_compare:
    push esi
    push edi
.cmp_loop:
    mov al, [esi]
    mov bl, [edi]
    cmp al, bl
    jne .not_equal
    test al, al
    jz .equal
    inc esi
    inc edi
    jmp .cmp_loop
.equal:
    mov eax, 1
    pop edi
    pop esi
    ret
.not_equal:
    xor eax, eax
    pop edi
    pop esi
    ret

; =============================================================================
; ai_str_startswith - Check if string starts with prefix
; =============================================================================
ai_str_startswith:
    push esi
    push edi
    push ebx
.sw_loop:
    mov bl, [edi]
    test bl, bl
    jz .sw_match
    mov al, [esi]
    cmp al, bl
    jne .sw_nomatch
    inc esi
    inc edi
    jmp .sw_loop
.sw_match:
    mov eax, 1
    pop ebx
    pop edi
    pop esi
    ret
.sw_nomatch:
    xor eax, eax
    pop ebx
    pop edi
    pop esi
    ret

; =============================================================================
; ai_hex_char_to_val - Convert hex character to value
; =============================================================================
ai_hex_char_to_val:
    cmp al, '0'
    jl .invalid
    cmp al, '9'
    jle .digit
    cmp al, 'A'
    jl .check_lower
    cmp al, 'F'
    jle .upper
.check_lower:
    cmp al, 'a'
    jl .invalid
    cmp al, 'f'
    jg .invalid
    sub al, 'a' - 10
    ret
.upper:
    sub al, 'A' - 10
    ret
.digit:
    sub al, '0'
    ret
.invalid:
    mov al, 0xFF
    ret
