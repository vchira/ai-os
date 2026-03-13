; =============================================================================
; AiOS - Command Shell
; Interactive command-line interface
; =============================================================================

%include "include/constants.inc"

section .data

; Shell prompt
shell_prompt:   db 'aios> ', 0
shell_unknown:  db 'Unknown command: ', 0
shell_type_help: db 10, "Type 'help' for a list of commands.", 10, 0

; Command strings
cmd_help:       db 'help', 0
cmd_clear:      db 'clear', 0
cmd_echo:       db 'echo', 0
cmd_meminfo:    db 'meminfo', 0
cmd_cpuinfo:    db 'cpuinfo', 0
cmd_reboot:     db 'reboot', 0
cmd_halt:       db 'halt', 0
cmd_color:      db 'color', 0
cmd_uptime:     db 'uptime', 0
cmd_ver:        db 'ver', 0
cmd_ls:         db 'ls', 0
cmd_ask:        db 'ask', 0
cmd_ai:         db 'ai', 0
cmd_net:        db 'net', 0
cmd_pci:        db 'pci', 0
cmd_exit:       db 'exit', 0

; Help text
help_text:
    db '  help    - Show this help message', 10
    db '  clear   - Clear the screen', 10
    db '  echo    - Echo text to screen (usage: echo <text>)', 10
    db '  meminfo - Display memory information', 10
    db '  cpuinfo - Display CPU information', 10
    db '  uptime  - Show system uptime', 10
    db '  color   - Change text color (usage: color <0-F>)', 10
    db '  ver     - Show OS version', 10
    db '  ls      - List commands', 10
    db '  ask     - Ask Claude AI (usage: ask <question>)', 10
    db '  ai      - Alias for ask', 10
    db '  net     - Show network status', 10
    db '  pci     - List PCI devices', 10
    db '  reboot  - Reboot the system', 10
    db '  halt    - Halt the CPU', 10, 0

; Memory info strings
meminfo_total:  db 'Total memory: ', 0
meminfo_kb:     db ' KB (', 0
meminfo_mb:     db ' MB)', 10, 0

; CPU info strings
cpuinfo_vendor: db 'CPU Vendor: ', 0
cpuinfo_buf:    times 13 db 0

; Uptime strings
uptime_msg:     db 'Uptime: ', 0
uptime_secs:    db ' seconds', 10, 0

; Version strings
ver_text:       db 'AiOS v0.1 - AI-Native Operating System', 10
                db 'The Deterministic Substrate', 10
                db 'Built with NASM + GNU ld + GRUB2', 10, 0

; Halt message
halt_msg:       db 'System halted. You can safely power off.', 10, 0

; Reboot message
reboot_msg:     db 'Rebooting...', 10, 0

; Color help
color_help:     db 'Usage: color <hex 0-F>', 10
                db 'Colors: 0=Black 1=Blue 2=Green 3=Cyan', 10
                db '  4=Red 5=Magenta 6=Brown 7=LightGray', 10
                db '  8=DarkGray 9=LightBlue A=LightGreen', 10
                db '  B=LightCyan C=LightRed D=LightMagenta', 10
                db '  E=Yellow F=White', 10, 0

color_set_msg:  db 'Color set.', 10, 0

; Ask command strings
ask_thinking:   db '[Claude] Thinking...', 10, 0
ask_error:      db '[Error] Claude API request failed', 10, 0
ask_no_query:   db 'Usage: ask <question>', 10, 0

; Net command strings
net_ip_msg:     db 'IP: ', 0
net_noip_msg:   db 'No IP address (DHCP pending)', 10, 0

section .bss
; Command input buffer
input_buffer:   resb 256
input_pos:      resd 1

; Network response buffer (used by ask command when network is ready)
net_resp:       resb 4096

section .text
global shell_run
global shell_run_interactive

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
extern claude_ask
extern net_is_up
extern net_get_ip
extern net_poll
extern pci_scan

; =============================================================================
; shell_run - Main shell loop (never returns)
; =============================================================================
shell_run:
.loop:
    ; Print prompt
    mov esi, shell_prompt
    call vga_print

    ; Read a line of input
    call shell_readline

    ; Skip empty lines
    cmp byte [input_buffer], 0
    je .loop

    ; Parse and execute command
    call shell_execute

    jmp .loop

; =============================================================================
; shell_run_interactive - Shell loop that returns on "exit" command
; =============================================================================
shell_run_interactive:
.loop:
    ; Print prompt
    mov esi, shell_prompt
    call vga_print

    ; Read a line of input
    call shell_readline

    ; Skip empty lines
    cmp byte [input_buffer], 0
    je .loop

    ; Check for "exit" command
    mov esi, input_buffer
    mov edi, cmd_exit
    call str_compare
    test eax, eax
    jnz .exit_shell

    ; Parse and execute command
    call shell_execute

    jmp .loop

.exit_shell:
    ret

; =============================================================================
; shell_readline - Read a line from keyboard into input_buffer
; =============================================================================
shell_readline:
    push eax
    push edi

    mov dword [input_pos], 0

.read_loop:
    call keyboard_getchar       ; al = character

    cmp al, 10                  ; Enter?
    je .line_done

    cmp al, 8                   ; Backspace?
    je .backspace

    cmp al, 127                 ; Delete (also backspace on some terminals)
    je .backspace

    ; Check buffer overflow
    cmp dword [input_pos], 254
    jge .read_loop

    ; Store character and echo it
    mov edi, [input_pos]
    mov [input_buffer + edi], al
    inc dword [input_pos]
    call vga_putchar
    jmp .read_loop

.backspace:
    cmp dword [input_pos], 0
    je .read_loop
    dec dword [input_pos]
    mov al, 8
    call vga_putchar
    jmp .read_loop

.line_done:
    ; Null-terminate
    mov edi, [input_pos]
    mov byte [input_buffer + edi], 0
    call vga_newline

    pop edi
    pop eax
    ret

; =============================================================================
; shell_execute - Parse and execute the command in input_buffer
; =============================================================================
shell_execute:
    push eax
    push esi
    push edi

    ; Try each command
    mov esi, input_buffer
    mov edi, cmd_help
    call str_startswith
    test eax, eax
    jnz .do_help

    mov esi, input_buffer
    mov edi, cmd_clear
    call str_compare
    test eax, eax
    jnz .do_clear

    mov esi, input_buffer
    mov edi, cmd_echo
    call str_startswith
    test eax, eax
    jnz .do_echo

    mov esi, input_buffer
    mov edi, cmd_meminfo
    call str_compare
    test eax, eax
    jnz .do_meminfo

    mov esi, input_buffer
    mov edi, cmd_cpuinfo
    call str_compare
    test eax, eax
    jnz .do_cpuinfo

    mov esi, input_buffer
    mov edi, cmd_reboot
    call str_compare
    test eax, eax
    jnz .do_reboot

    mov esi, input_buffer
    mov edi, cmd_halt
    call str_compare
    test eax, eax
    jnz .do_halt

    mov esi, input_buffer
    mov edi, cmd_color
    call str_startswith
    test eax, eax
    jnz .do_color

    mov esi, input_buffer
    mov edi, cmd_uptime
    call str_compare
    test eax, eax
    jnz .do_uptime

    mov esi, input_buffer
    mov edi, cmd_ver
    call str_compare
    test eax, eax
    jnz .do_ver

    mov esi, input_buffer
    mov edi, cmd_ls
    call str_compare
    test eax, eax
    jnz .do_help

    mov esi, input_buffer
    mov edi, cmd_ask
    call str_startswith
    test eax, eax
    jnz .do_ask

    mov esi, input_buffer
    mov edi, cmd_ai
    call str_startswith
    test eax, eax
    jnz .do_ai

    mov esi, input_buffer
    mov edi, cmd_net
    call str_startswith
    test eax, eax
    jnz .do_net

    mov esi, input_buffer
    mov edi, cmd_pci
    call str_compare
    test eax, eax
    jnz .do_pci

    ; Unknown command
    mov esi, shell_unknown
    call vga_print
    mov esi, input_buffer
    call vga_print
    mov esi, shell_type_help
    call vga_print
    jmp .exec_done

.do_help:
    mov esi, help_text
    call vga_print
    jmp .exec_done

.do_clear:
    call vga_clear
    jmp .exec_done

.do_echo:
    ; Skip "echo" and the space after it
    mov esi, input_buffer
    add esi, 4
    ; Skip leading space
    cmp byte [esi], ' '
    jne .echo_print
    inc esi
.echo_print:
    call vga_print
    call vga_newline
    jmp .exec_done

.do_meminfo:
    mov esi, meminfo_total
    call vga_print
    call memory_get_total       ; eax = total KB
    push eax
    call vga_print_dec
    mov esi, meminfo_kb
    call vga_print
    pop eax
    shr eax, 10                 ; KB to MB
    call vga_print_dec
    mov esi, meminfo_mb
    call vga_print
    jmp .exec_done

.do_cpuinfo:
    ; Use CPUID to get vendor string
    mov esi, cpuinfo_vendor
    call vga_print

    mov eax, 0                  ; CPUID function 0
    cpuid

    ; Vendor string is in EBX:EDX:ECX
    mov [cpuinfo_buf], ebx
    mov [cpuinfo_buf+4], edx
    mov [cpuinfo_buf+8], ecx
    mov byte [cpuinfo_buf+12], 0

    mov esi, cpuinfo_buf
    call vga_print
    call vga_newline
    jmp .exec_done

.do_reboot:
    mov esi, reboot_msg
    call vga_print

    ; Triple fault reboot: load null IDT and trigger interrupt
    lidt [.null_idt]
    int 3
    jmp $

.null_idt:
    dw 0
    dd 0

.do_halt:
    mov esi, halt_msg
    call vga_print
    cli
    hlt
    jmp $

.do_color:
    ; Parse color value after "color "
    mov esi, input_buffer
    add esi, 5
    cmp byte [esi], ' '
    jne .color_show_help
    inc esi

    ; Parse hex digit
    mov al, [esi]
    cmp al, 0
    je .color_show_help

    ; Convert hex char to value
    call hex_char_to_val
    cmp al, 0xFF
    je .color_show_help

    ; Set color: keep black background, set foreground
    mov ah, COLOR_BLACK
    shl ah, 4
    or al, ah
    call vga_set_color

    mov esi, color_set_msg
    call vga_print
    jmp .exec_done

.color_show_help:
    mov esi, color_help
    call vga_print
    jmp .exec_done

.do_uptime:
    mov esi, uptime_msg
    call vga_print
    call timer_get_uptime_secs
    call vga_print_dec
    mov esi, uptime_secs
    call vga_print
    jmp .exec_done

.do_ver:
    mov esi, ver_text
    call vga_print
    jmp .exec_done

.do_ai:
    ; "ai " -> skip 2 chars, then check for space
    mov esi, input_buffer
    add esi, 2
    jmp .ask_parse_space

.do_ask:
    ; "ask " -> skip 3 chars
    mov esi, input_buffer
    add esi, 3

.ask_parse_space:
    ; Skip leading space
    cmp byte [esi], ' '
    jne .ask_check_empty
    inc esi
    jmp .ask_parse_space

.ask_check_empty:
    cmp byte [esi], 0
    je .ask_show_usage

    ; Print thinking indicator
    push esi
    mov esi, ask_thinking
    call vga_print
    pop esi

    ; Call claude_ask(question, response_buf, max_len)
    push dword 4095
    push dword net_resp
    push esi
    call claude_ask
    add esp, 12

    ; Check return value
    cmp eax, 0
    jl .ask_error

    ; Print response in light cyan
    mov al, (COLOR_BLACK << 4) | COLOR_LCYAN
    call vga_set_color
    mov esi, net_resp
    call vga_print
    call vga_newline
    mov al, DEFAULT_COLOR
    call vga_set_color
    jmp .exec_done

.ask_error:
    mov esi, ask_error
    call vga_print
    jmp .exec_done

.ask_show_usage:
    mov esi, ask_no_query
    call vga_print
    jmp .exec_done

.do_net:
    ; Show network IP status
    call net_is_up
    test eax, eax
    jz .net_no_ip

    mov esi, net_ip_msg
    call vga_print
    ; net_get_ip(buf, max_len)
    push dword 32
    push dword net_resp
    call net_get_ip
    add esp, 8
    mov esi, net_resp
    call vga_print
    call vga_newline
    jmp .exec_done

.net_no_ip:
    mov esi, net_noip_msg
    call vga_print
    jmp .exec_done

.do_pci:
    call pci_scan
    jmp .exec_done

.exec_done:
    pop edi
    pop esi
    pop eax
    ret

; =============================================================================
; str_compare - Compare two null-terminated strings
; Input: esi = string1, edi = string2
; Output: eax = 1 if equal, 0 if not
; =============================================================================
str_compare:
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
; str_startswith - Check if string starts with prefix
; Input: esi = string, edi = prefix
; Output: eax = 1 if starts with prefix, 0 if not
; =============================================================================
str_startswith:
    push esi
    push edi
    push ebx

.sw_loop:
    mov bl, [edi]
    test bl, bl
    jz .sw_match               ; End of prefix = match
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
; hex_char_to_val - Convert hex character to value
; Input: al = hex char ('0'-'9', 'A'-'F', 'a'-'f')
; Output: al = value (0-15) or 0xFF if invalid
; =============================================================================
hex_char_to_val:
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
