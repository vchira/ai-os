; =============================================================================
; AsmOS - Global Descriptor Table (GDT)
; Flat memory model: full 4GB address space for code and data
; =============================================================================

section .data

; GDT structure
gdt_start:

; Null descriptor (required)
gdt_null:
    dq 0

; Kernel code segment descriptor
; Base=0, Limit=0xFFFFF, 4KB granularity, 32-bit, ring 0, execute/read
gdt_code:
    dw 0xFFFF           ; Limit (bits 0-15)
    dw 0x0000           ; Base (bits 0-15)
    db 0x00             ; Base (bits 16-23)
    db 10011010b        ; Access: Present, Ring 0, Code segment, Executable, Readable
    db 11001111b        ; Flags: 4KB granularity, 32-bit + Limit (bits 16-19)
    db 0x00             ; Base (bits 24-31)

; Kernel data segment descriptor
; Base=0, Limit=0xFFFFF, 4KB granularity, 32-bit, ring 0, read/write
gdt_data:
    dw 0xFFFF           ; Limit (bits 0-15)
    dw 0x0000           ; Base (bits 0-15)
    db 0x00             ; Base (bits 16-23)
    db 10010010b        ; Access: Present, Ring 0, Data segment, Writable
    db 11001111b        ; Flags: 4KB granularity, 32-bit + Limit (bits 16-19)
    db 0x00             ; Base (bits 24-31)

gdt_end:

; GDT descriptor (pointer)
gdt_descriptor:
    dw gdt_end - gdt_start - 1     ; Size of GDT - 1
    dd gdt_start                     ; Address of GDT

; Segment selectors
CODE_SEG equ gdt_code - gdt_start   ; 0x08
DATA_SEG equ gdt_data - gdt_start   ; 0x10

section .text
global gdt_init
global CODE_SEG
global DATA_SEG

; =============================================================================
; gdt_init - Load the GDT and reload segment registers
; =============================================================================
gdt_init:
    lgdt [gdt_descriptor]

    ; Reload CS via far jump
    jmp CODE_SEG:.reload_segments

.reload_segments:
    ; Reload data segment registers
    mov ax, DATA_SEG
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    ret
