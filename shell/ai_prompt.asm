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
slash_time:     db '/time', 0
slash_memory:   db '/memory', 0
slash_key:      db '/key', 0
slash_selftest: db '/selftest', 0
slash_update:   db '/update', 0
slash_theme:    db '/theme', 0
slash_debug:    db '/debug', 0
slash_settings: db '/settings', 0

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
    db '  /time    - Show current date and time', 10
    db '  /memory  - Dump AI memory store', 10
    db '  /key     - Set API key (usage: /key claude <key>)', 10
    db '  /halt    - Halt the CPU', 10
    db '  /selftest - Run AI self-test suite (no tokens used)', 10
    db '  /update   - Update kernel from URL (usage: /update https://...)', 10
    db '  /theme    - Switch UI theme (usage: /theme <0-4>)', 10
    db '  /debug    - Show debug log window', 10
    db '  /settings - Show/manage OS settings', 10
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

; Time/Memory strings
ai_time_label:  db 'Date/Time: ', 0
ai_mem_label:   db 'AI Memory:', 10, 0

; Key management strings
ai_key_usage:   db 'Usage: /key <provider> <api-key>', 10
                db '  /key claude sk-ant-api03-...', 10
                db '  /key openai sk-...', 10, 0
ai_key_set_ok:  db 'API key set for: ', 0
ai_key_invalid: db 'Unknown provider. Use: claude, openai', 10, 0
ai_key_claude:  db 'claude', 0
ai_key_openai:  db 'openai', 0

; Network wait strings
ai_net_wait:    db 'Waiting for network... ', 0
ai_net_ready:   db '[OK] Network ready - IP: ', 0
ai_net_fail:    db '[!!] Network timeout - AI needs network to work', 10
                db '     Try /net to check status later', 10, 0
spinner_chars:  db '|/-', 0x5C    ; | / - backslash

; Update strings
ai_update_usage: db 'Usage: /update https://example.com/aios.bin', 10, 0

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

; Buffers for /time and /memory
ai_time_buf:    resb 32
ai_mem_buf:     resb 2048

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
extern prompt_win_init
extern prompt_win_print
extern prompt_win_putchar
extern prompt_win_clear
extern prompt_win_set_color
extern prompt_win_newline
extern prompt_win_print_dec
extern prompt_win_print_hex
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
extern rtc_get_datetime_str
extern tool_memory_dump
extern llm_set_api_key
extern sys_now
extern selftest_run
extern update_kernel
extern theme_set
extern theme_name
extern theme_count
extern theme_current_id
extern dbg_show
extern dbg_clear
extern settings_dump

; =============================================================================
; ai_prompt_run - Main AI prompt loop
; =============================================================================
ai_prompt_run:
    ; Create the prompt window (nearly maximized)
    call prompt_win_init

    ; Wait for network before showing prompt
    call ai_wait_for_network

    ; Print welcome
    mov esi, ai_welcome
    call pw_print

.loop:
    ; Print prompt
    mov esi, ai_prompt_str
    call pw_print

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
; ai_wait_for_network - Wait for DHCP with spinner animation
; =============================================================================
ai_wait_for_network:
    push ebx
    push esi
    push edi
    push ebp

    ; Check if already up
    call net_is_up
    test eax, eax
    jnz .nw_already_up

    ; Print waiting message
    mov esi, ai_net_wait
    call pw_print

    ; Get start time for timeout
    call timer_get_uptime_secs
    mov ebp, eax                    ; ebp = start time (callee-saved)
    xor ebx, ebx                    ; ebx = tick counter (callee-saved)

.nw_loop:
    ; Poll network stack
    call net_poll

    ; Check if network is up
    call net_is_up
    test eax, eax
    jnz .nw_ready

    ; Timeout after 30 seconds
    call timer_get_uptime_secs
    sub eax, ebp
    cmp eax, 30
    jge .nw_timeout

    ; Wait for next interrupt (~10ms at 100Hz PIT)
    hlt

    ; Print a dot every 64 ticks (~640ms) for progress
    inc ebx
    test ebx, 63
    jnz .nw_loop
    mov esi, .nw_dot
    call pw_print
    jmp .nw_loop

section .data
.nw_dot: db '.', 0

section .text

.nw_already_up:
.nw_ready:
    call prompt_win_newline

    ; Print success
    mov esi, ai_net_ready
    call pw_print

    ; Print IP address
    push dword 32
    push dword ai_net_buf
    call net_get_ip
    add esp, 8
    mov esi, ai_net_buf
    call pw_print
    call prompt_win_newline

    pop ebp
    pop edi
    pop esi
    pop ebx
    ret

.nw_timeout:
    call prompt_win_newline
    mov esi, ai_net_fail
    call pw_print

    pop ebp
    pop edi
    pop esi
    pop ebx
    ret

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
    movzx eax, al
    push eax
    call prompt_win_putchar
    add esp, 4
    jmp .read_loop

.backspace:
    cmp dword [ai_input_pos], 0
    je .read_loop
    dec dword [ai_input_pos]
    push dword 8
    call prompt_win_putchar
    add esp, 4
    jmp .read_loop

.line_done:
    mov edi, [ai_input_pos]
    mov byte [ai_input + edi], 0
    call prompt_win_newline

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
    mov esi, ai_thinking
    call pw_print

    ; Call llm_ask(question, response_buf, max_len)
    push dword 4095
    push dword ai_resp
    push dword ai_input
    call llm_ask
    add esp, 12

    ; Check return value
    cmp eax, 0
    jl .ask_error

    ; Print response
    mov esi, ai_resp
    call pw_print
    call prompt_win_newline
    jmp .ask_done

.ask_error:
    ; ai_resp contains the detailed error message from claude_ask
    mov esi, ai_resp
    call pw_print
    call prompt_win_newline

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

    ; /time
    mov esi, ai_input
    mov edi, slash_time
    call ai_str_compare
    test eax, eax
    jnz .do_time

    ; /memory
    mov esi, ai_input
    mov edi, slash_memory
    call ai_str_compare
    test eax, eax
    jnz .do_memory

    ; /key
    mov esi, ai_input
    mov edi, slash_key
    call ai_str_startswith
    test eax, eax
    jnz .do_key

    ; /selftest
    mov esi, ai_input
    mov edi, slash_selftest
    call ai_str_compare
    test eax, eax
    jnz .do_selftest

    ; /theme
    mov esi, ai_input
    mov edi, slash_theme
    call ai_str_startswith
    test eax, eax
    jnz .do_theme

    ; /debug
    mov esi, ai_input
    mov edi, slash_debug
    call ai_str_startswith
    test eax, eax
    jnz .do_debug

    ; /settings
    mov esi, ai_input
    mov edi, slash_settings
    call ai_str_compare
    test eax, eax
    jnz .do_settings

    ; /update
    mov esi, ai_input
    mov edi, slash_update
    call ai_str_startswith
    test eax, eax
    jnz .do_update

    ; Unknown slash command
    mov esi, ai_unknown_cmd
    call pw_print
    mov esi, ai_input
    call pw_print
    mov esi, ai_use_help
    call pw_print
    jmp .slash_done

.do_help:
    mov esi, ai_help_text
    call pw_print
    jmp .slash_done

.do_shell:
    mov esi, shell_enter_msg
    call pw_print
    call shell_run_interactive  ; Runs shell until user types "exit"
    mov esi, shell_exit_msg
    call pw_print
    jmp .slash_done

.do_clear:
    call pw_clear
    jmp .slash_done

.do_net:
    call net_is_up
    test eax, eax
    jz .net_no_ip

    mov esi, ai_net_ip_msg
    call pw_print
    push dword 32
    push dword ai_net_buf
    call net_get_ip
    add esp, 8
    mov esi, ai_net_buf
    call pw_print
    call pw_newline
    jmp .slash_done

.net_no_ip:
    mov esi, ai_net_noip_msg
    call pw_print
    jmp .slash_done

.do_pci:
    call pci_scan
    jmp .slash_done

.do_reboot:
    mov esi, ai_reboot_msg
    call pw_print
    lidt [.null_idt]
    int 3
    jmp $
.null_idt:
    dw 0
    dd 0

.do_halt:
    mov esi, ai_halt_msg
    call pw_print
    cli
    hlt
    jmp $

.do_ver:
    mov esi, ai_ver_text
    call pw_print
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
    call pw_set_color

    mov esi, ai_color_set
    call pw_print
    jmp .slash_done

.color_show_help:
    mov esi, ai_color_help
    call pw_print
    jmp .slash_done

.do_uptime:
    mov esi, ai_uptime_msg
    call pw_print
    call timer_get_uptime_secs
    call pw_print_dec
    mov esi, ai_uptime_secs
    call pw_print
    jmp .slash_done

.do_meminfo:
    mov esi, ai_meminfo_total
    call pw_print
    call memory_get_total
    push eax
    call pw_print_dec
    mov esi, ai_meminfo_kb
    call pw_print
    pop eax
    shr eax, 10
    call pw_print_dec
    mov esi, ai_meminfo_mb
    call pw_print
    jmp .slash_done

.do_cpuinfo:
    mov esi, ai_cpuinfo_vendor
    call pw_print
    mov eax, 0
    cpuid
    mov [ai_cpuinfo_buf], ebx
    mov [ai_cpuinfo_buf+4], edx
    mov [ai_cpuinfo_buf+8], ecx
    mov byte [ai_cpuinfo_buf+12], 0
    mov esi, ai_cpuinfo_buf
    call pw_print
    call pw_newline
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
    call pw_print
    call keyboard_get_layout
    push eax
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call pw_print
    call pw_newline
    jmp .slash_done

.kbd_show_layouts:
    ; Show current layout
    mov esi, ai_kbd_current
    call pw_print
    call keyboard_get_layout
    push eax                    ; save current layout ID
    push eax
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call pw_print
    call pw_newline

    ; List all layouts
    mov esi, ai_kbd_avail
    call pw_print

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
    call pw_print

    ; Print index number
    mov eax, ebx
    call pw_print_dec

    ; Print ": "
    mov al, ':'
    call pw_putchar
    mov al, ' '
    call pw_putchar

    ; Print layout name
    push ebx
    call keyboard_get_layout_name
    add esp, 4
    mov esi, eax
    call pw_print

    ; Mark active layout
    pop edx
    pop ecx
    cmp ebx, edx
    jne .kbd_not_active
    mov esi, ai_kbd_arrow
    call pw_print
.kbd_not_active:
    call pw_newline
    inc ebx
    push ecx
    push edx
    pop edx
    pop ecx
    jmp .kbd_list_loop

.kbd_invalid:
    mov esi, ai_kbd_invalid
    call pw_print
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
    call pw_print
    call llm_get_active
    push eax
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call pw_print
    call pw_newline
    jmp .slash_done

.prov_show_list:
    ; Show current provider
    mov esi, ai_prov_current
    call pw_print
    call llm_get_active
    push eax                    ; save active ID
    push eax
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call pw_print

    ; Show model
    mov esi, ai_prov_model
    call pw_print
    ; active ID still on stack from saved push
    mov eax, [esp]              ; peek at saved active ID
    push eax
    call llm_get_provider_model
    add esp, 4
    mov esi, eax
    call pw_print
    mov esi, ai_prov_model_e
    call pw_print

    ; List all providers
    mov esi, ai_prov_avail
    call pw_print

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
    call pw_print

    ; Print index
    mov eax, ebx
    call pw_print_dec

    ; Print ": "
    mov al, ':'
    call pw_putchar
    mov al, ' '
    call pw_putchar

    ; Print provider name
    push ebx
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call pw_print

    ; Print model in parens
    mov esi, ai_prov_model
    call pw_print
    push ebx
    call llm_get_provider_model
    add esp, 4
    mov esi, eax
    call pw_print
    mov al, ')'
    call pw_putchar

    ; Check if configured
    push ebx
    call llm_is_configured
    add esp, 4
    test eax, eax
    jnz .prov_is_configured
    mov esi, ai_prov_nokey
    call pw_print
.prov_is_configured:

    ; Mark active
    pop edx
    pop ecx
    cmp ebx, edx
    jne .prov_not_active
    mov esi, ai_kbd_arrow       ; reuse " <- active"
    call pw_print
.prov_not_active:
    call pw_newline
    inc ebx
    push ecx
    push edx
    pop edx
    pop ecx
    jmp .prov_list_loop

.prov_invalid:
    mov esi, ai_prov_invalid
    call pw_print
    jmp .slash_done

.do_time:
    mov esi, ai_time_label
    call pw_print
    push dword 32
    push dword ai_time_buf
    call rtc_get_datetime_str
    add esp, 8
    mov esi, ai_time_buf
    call pw_print
    call pw_newline
    jmp .slash_done

.do_memory:
    mov esi, ai_mem_label
    call pw_print
    push dword 2048
    push dword ai_mem_buf
    call tool_memory_dump
    add esp, 8
    mov esi, ai_mem_buf
    call pw_print
    jmp .slash_done

.do_key:
    ; Parse "/key " — need at least "/key " (4 chars + space)
    mov esi, ai_input
    add esi, 4                  ; skip "/key"
    cmp byte [esi], 0
    je .key_show_usage
    cmp byte [esi], ' '
    jne .key_show_usage
    inc esi                     ; skip space

    ; Now esi points to provider name. Check "claude" or "openai"
    ; Compare first word against "claude"
    push esi                    ; save start of provider
    mov edi, ai_key_claude
    call .key_match_word
    test eax, eax
    jnz .key_is_claude

    pop esi
    push esi
    mov edi, ai_key_openai
    call .key_match_word
    test eax, eax
    jnz .key_is_openai

    pop esi
    mov esi, ai_key_invalid
    call pw_print
    jmp .slash_done

.key_is_claude:
    pop esi
    add esi, 7                  ; skip "claude " (6 chars + space)
    mov eax, 0                  ; provider_id = 0 (Claude)
    jmp .key_set

.key_is_openai:
    pop esi
    add esi, 7                  ; skip "openai " (6 chars + space)
    mov eax, 1                  ; provider_id = 1 (OpenAI)
    jmp .key_set

.key_set:
    ; esi = pointer to API key string, eax = provider_id
    ; Check key is not empty
    cmp byte [esi], 0
    je .key_show_usage

    push esi                    ; key string
    push eax                    ; provider_id
    call llm_set_api_key
    add esp, 8
    cmp eax, 0
    jl .key_show_usage

    mov esi, ai_key_set_ok
    call pw_print
    ; Print provider name
    call llm_get_active
    push eax
    call llm_get_provider_name
    add esp, 4
    mov esi, eax
    call pw_print
    call pw_newline
    jmp .slash_done

.key_show_usage:
    mov esi, ai_key_usage
    call pw_print
    jmp .slash_done

.do_selftest:
    call selftest_run
    jmp .slash_done

.do_theme:
    ; Parse "/theme " — check for digit after space
    mov esi, ai_input
    add esi, 6                  ; skip "/theme"
    cmp byte [esi], 0
    je .theme_show              ; no arg — show current
    cmp byte [esi], ' '
    jne .theme_show
    inc esi                     ; skip space
    cmp byte [esi], 0
    je .theme_show

    ; Parse single digit 0-9
    movzx eax, byte [esi]
    sub eax, '0'
    cmp eax, 9
    ja .theme_show

    ; Set theme
    push eax
    call theme_set
    add esp, 4

    ; Print confirmation
    mov al, 0x0A                ; light green
    call pw_set_color
    mov esi, .theme_set_msg
    call pw_print

    ; Print theme name
    call theme_current_id
    push eax
    call theme_name
    add esp, 4
    mov esi, eax
    call pw_print
    call pw_newline

    mov al, 0x07
    call pw_set_color
    jmp .slash_done

.theme_show:
    ; List all themes
    mov al, 0x0B                ; light cyan
    call pw_set_color
    mov esi, .theme_list_hdr
    call pw_print

    xor ebx, ebx               ; theme index
.theme_list_loop:
    push ebx
    call theme_count
    pop ebx
    cmp ebx, eax
    jge .theme_list_done

    ; Print "  N: ThemeName"
    mov al, ' '
    call pw_putchar
    mov al, ' '
    call pw_putchar
    mov eax, ebx
    add al, '0'
    call pw_putchar
    mov al, ':'
    call pw_putchar
    mov al, ' '
    call pw_putchar

    push ebx
    call theme_name
    add esp, 4
    mov esi, eax
    call pw_print

    ; Check if current
    push ebx
    call theme_current_id
    pop ebx
    cmp eax, ebx
    jne .theme_not_current
    mov esi, .theme_current_tag
    call pw_print
.theme_not_current:
    call pw_newline
    inc ebx
    jmp .theme_list_loop

.theme_list_done:
    mov al, 0x07
    call pw_set_color
    jmp .slash_done

section .data
.theme_set_msg: db 'Theme set to: ', 0
.theme_list_hdr: db 'UI Themes (usage: /theme <id>):', 10, 0
.theme_current_tag: db '  [active]', 0

section .text

.do_settings:
    mov esi, .settings_hdr
    call pw_print
    push dword 2048
    push dword ai_mem_buf       ; reuse temp buffer
    call settings_dump
    add esp, 8
    mov esi, ai_mem_buf
    call pw_print
    jmp .slash_done

section .data
.settings_hdr: db 'OS Settings:', 10, 0

section .text

.do_debug:
    ; Check for "/debug clear"
    mov esi, ai_input
    add esi, 6                  ; skip "/debug"
    cmp byte [esi], ' '
    jne .debug_show
    inc esi
    cmp byte [esi], 'c'
    jne .debug_show
    ; It's "/debug clear"
    call dbg_clear
    mov esi, .debug_cleared_msg
    call pw_print
    jmp .slash_done
.debug_show:
    call dbg_show
    jmp .slash_done

section .data
.debug_cleared_msg: db 'Debug log cleared.', 10, 0

section .text

.do_update:
    ; Parse "/update " — need "/update" (7 chars) + space + URL
    mov esi, ai_input
    add esi, 7                  ; skip "/update"
    cmp byte [esi], 0
    je .update_show_usage
    cmp byte [esi], ' '
    jne .update_show_usage
    inc esi                     ; skip space — esi now points to URL

    ; Check URL is not empty
    cmp byte [esi], 0
    je .update_show_usage

    ; Call update_kernel(url)
    push esi
    call update_kernel
    add esp, 4
    jmp .slash_done

.update_show_usage:
    mov esi, ai_update_usage
    call pw_print
    jmp .slash_done

; Helper: check if string at esi starts with word at edi (until null/space)
; Returns eax=1 if match, 0 if not
.key_match_word:
    push esi
    push edi
.kmw_loop:
    mov al, [edi]
    test al, al
    jz .kmw_check_end
    cmp al, [esi]
    jne .kmw_fail
    inc esi
    inc edi
    jmp .kmw_loop
.kmw_check_end:
    ; Word matched — esi should be at space or null
    mov al, [esi]
    cmp al, ' '
    je .kmw_ok
    cmp al, 0
    je .kmw_ok
.kmw_fail:
    xor eax, eax
    pop edi
    pop esi
    ret
.kmw_ok:
    mov eax, 1
    pop edi
    pop esi
    ret

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

; =============================================================================
; Prompt window wrappers — same calling convention as vga_* but output to window
; =============================================================================

; pw_print: like vga_print — esi = string pointer
pw_print:
    push esi
    call prompt_win_print
    add esp, 4
    ret

; pw_putchar: like vga_putchar — al = character
pw_putchar:
    movzx eax, al
    push eax
    call prompt_win_putchar
    add esp, 4
    ret

; pw_newline: like vga_newline
pw_newline:
    call prompt_win_newline
    ret

; pw_print_dec: like vga_print_dec — eax = integer
pw_print_dec:
    push eax
    call pw_print_dec
    add esp, 4
    ret

; pw_set_color: like vga_set_color — al = attr (no-op for window)
pw_set_color:
    ret

; pw_clear: like vga_clear — clears prompt window
pw_clear:
    call prompt_win_clear
    ret
