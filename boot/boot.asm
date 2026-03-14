; =============================================================================
; AiOS Bootloader Entry Point
; Multiboot-compliant entry loaded by GRUB2
; Requests VESA linear framebuffer for graphical console
; =============================================================================

MBOOT_PAGE_ALIGN    equ 1 << 0
MBOOT_MEM_INFO      equ 1 << 1
MBOOT_VIDEO_MODE    equ 1 << 2
MBOOT_HEADER_MAGIC  equ 0x1BADB002
MBOOT_HEADER_FLAGS  equ MBOOT_PAGE_ALIGN | MBOOT_MEM_INFO | MBOOT_VIDEO_MODE
MBOOT_CHECKSUM      equ -(MBOOT_HEADER_MAGIC + MBOOT_HEADER_FLAGS)

section .multiboot
align 4
    dd MBOOT_HEADER_MAGIC
    dd MBOOT_HEADER_FLAGS
    dd MBOOT_CHECKSUM
    ; Address fields (offsets 12-28, unused but must be present for video fields)
    dd 0                    ; header_addr
    dd 0                    ; load_addr
    dd 0                    ; load_end_addr
    dd 0                    ; bss_end_addr
    dd 0                    ; entry_addr
    ; Video mode fields (offset 32-44, flag bit 2)
    dd 0                    ; mode_type: 0 = linear framebuffer
    dd 1024                 ; preferred width
    dd 768                  ; preferred height
    dd 32                   ; preferred depth (32 bpp ARGB)

section .bss
align 16
stack_bottom:
    resb 16384              ; 16 KB stack
stack_top:

section .text
global _start
extern kernel_main

_start:
    mov esp, stack_top
    push ebx                ; multiboot info pointer
    push eax                ; multiboot magic
    call kernel_main

    ; If kernel returns, halt
.hang:
    cli
    hlt
    jmp .hang
