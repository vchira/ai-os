; =============================================================================
; AsmOS - Interrupt Descriptor Table (IDT)
; Handles CPU exceptions (ISR 0-31) and hardware interrupts (IRQ 0-15)
; =============================================================================

%include "include/constants.inc"

section .data

; IDT table - 256 entries, 8 bytes each
global idt_start
idt_start:
    times 256 dq 0
idt_end:

; IDT descriptor
idt_descriptor:
    dw idt_end - idt_start - 1
    dd idt_start

section .text
global idt_init
global idt_set_gate
extern keyboard_handler
extern timer_handler
extern mouse_handler

; =============================================================================
; idt_init - Set up IDT, remap PIC, install handlers
; =============================================================================
idt_init:
    ; Remap the PIC
    call pic_remap

    ; Install ISR handlers (CPU exceptions 0-31)
    mov eax, isr_default
    mov ecx, 0
.install_isrs:
    cmp ecx, 32
    jge .install_irqs
    push ecx
    push eax
    call idt_set_gate
    add esp, 8
    inc ecx
    jmp .install_isrs

.install_irqs:
    ; IRQ0 - Timer (interrupt 32)
    push dword 32
    push dword irq0_handler
    call idt_set_gate
    add esp, 8

    ; IRQ1 - Keyboard (interrupt 33)
    push dword 33
    push dword irq1_handler
    call idt_set_gate
    add esp, 8

    ; Install default handlers for remaining IRQs (34-47)
    mov ecx, 34
.install_remaining_irqs:
    cmp ecx, 48
    jge .install_mouse
    push ecx
    push dword irq_default
    call idt_set_gate
    add esp, 8
    inc ecx
    jmp .install_remaining_irqs

.install_mouse:
    ; IRQ12 - Mouse (interrupt 44) — overrides the default handler
    push dword 44
    push dword irq12_handler
    call idt_set_gate
    add esp, 8

.load_idt:
    lidt [idt_descriptor]
    sti                         ; Enable interrupts
    ret

; =============================================================================
; idt_set_gate - Set an IDT entry
; [esp+4] = handler address, [esp+8] = interrupt number
; =============================================================================
idt_set_gate:
    push ebp
    mov ebp, esp
    push ebx
    push eax

    mov eax, [ebp+8]           ; Handler address
    mov ebx, [ebp+12]          ; Interrupt number

    ; Calculate IDT entry address: idt_start + (num * 8)
    shl ebx, 3
    add ebx, idt_start

    ; Low 16 bits of handler
    mov word [ebx], ax

    ; Code segment selector
    mov word [ebx+2], 0x08     ; CODE_SEG

    ; Reserved byte
    mov byte [ebx+4], 0

    ; Type and attributes: Present, Ring 0, 32-bit interrupt gate
    mov byte [ebx+5], 10001110b

    ; High 16 bits of handler
    shr eax, 16
    mov word [ebx+6], ax

    pop eax
    pop ebx
    pop ebp
    ret

; =============================================================================
; pic_remap - Remap PIC to interrupts 32-47
; =============================================================================
pic_remap:
    ; Save masks
    in al, PIC1_DATA
    push eax
    in al, PIC2_DATA
    push eax

    ; ICW1: Initialize + ICW4 needed
    mov al, 0x11
    out PIC1_CMD, al
    out PIC2_CMD, al

    ; ICW2: Vector offset
    mov al, 0x20            ; Master PIC: IRQ 0-7 -> INT 32-39
    out PIC1_DATA, al
    mov al, 0x28            ; Slave PIC: IRQ 8-15 -> INT 40-47
    out PIC2_DATA, al

    ; ICW3: Master/Slave wiring
    mov al, 0x04            ; Master: slave on IRQ2
    out PIC1_DATA, al
    mov al, 0x02            ; Slave: cascade identity
    out PIC2_DATA, al

    ; ICW4: 8086 mode
    mov al, 0x01
    out PIC1_DATA, al
    out PIC2_DATA, al

    ; Clear saved masks from stack
    pop eax
    pop eax

    ; Enable all IRQs
    mov al, 0x0
    out PIC1_DATA, al
    out PIC2_DATA, al

    ret

; =============================================================================
; IRQ Handlers
; =============================================================================

; IRQ0 - Timer interrupt
irq0_handler:
    pushad
    call timer_handler
    mov al, 0x20
    out PIC1_CMD, al        ; Send EOI to master PIC
    popad
    iret

; IRQ1 - Keyboard interrupt
irq1_handler:
    pushad
    call keyboard_handler
    mov al, 0x20
    out PIC1_CMD, al        ; Send EOI to master PIC
    popad
    iret

; IRQ12 - Mouse interrupt
irq12_handler:
    pushad
    call mouse_handler
    mov al, 0x20
    out PIC2_CMD, al            ; Send EOI to slave PIC
    out PIC1_CMD, al            ; Send EOI to master PIC
    popad
    iret

; Default IRQ handler (just sends EOI)
irq_default:
    pushad
    mov al, 0x20
    out PIC1_CMD, al
    out PIC2_CMD, al
    popad
    iret

; Default ISR handler (CPU exceptions)
isr_default:
    pushad
    ; For now, just return - a real OS would handle faults
    popad
    iret
