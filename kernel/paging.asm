; =============================================================================
; AsmOS - Paging / Virtual Memory Manager
; Identity-maps kernel region, provides physical page allocator,
; and enables x86 paging (CR0.PG)
; =============================================================================

%include "include/constants.inc"

section .bss

; Page directory and initial page tables (must be 4KB-aligned)
align 4096
global page_directory
page_directory:     resb 4096               ; 1024 entries * 4 bytes

align 4096
page_table_0:       resb 4096               ; Identity map 0-4MB

align 4096
page_table_1:       resb 4096               ; Identity map 4-8MB

align 4096
page_table_2:       resb 4096               ; Identity map 8-12MB

align 4096
page_table_3:       resb 4096               ; Identity map 12-16MB

; Physical page bitmap (1 bit per 4KB page)
; Supports up to MAX_PHYS_PAGES pages (64MB = 16384 pages = 2048 bytes)
page_bitmap:        resb BITMAP_SIZE

; Statistics
phys_pages_total:   resd 1
phys_pages_free:    resd 1

section .data

pf_msg:     db 10, '*** PAGE FAULT ***', 10, 0
pf_addr:    db 'Fault address (CR2): ', 0
pf_code:    db 'Error code: ', 0
pf_halt:    db 10, 'System halted.', 10, 0
paging_ok:  db '[OK] Paging enabled (identity-mapped 16 MB)', 10, 0

section .text
global paging_init
global page_alloc
global page_free
global page_get_free_count
global page_fault_handler

extern mem_total
extern vga_print
extern vga_print_hex
extern vga_set_color
extern idt_set_gate

; =============================================================================
; paging_init - Set up paging with identity-mapped first 16MB
;
; Steps:
;   1. Zero the page directory
;   2. Fill 4 page tables with identity mappings (0-16MB)
;   3. Point page directory entries 0-3 at the page tables
;   4. Initialize the physical page bitmap
;   5. Install page fault handler (ISR 14)
;   6. Load CR3 and enable paging (CR0 bit 31)
; =============================================================================
paging_init:
    push ebp
    mov ebp, esp
    push ebx
    push ecx
    push edx
    push esi
    push edi

    ; -----------------------------------------------------------------
    ; Step 1: Zero the page directory
    ; -----------------------------------------------------------------
    mov edi, page_directory
    xor eax, eax
    mov ecx, 1024
    rep stosd

    ; -----------------------------------------------------------------
    ; Step 2: Fill page tables with identity mappings
    ; Each page table maps 4MB: 1024 entries * 4KB = 4MB
    ; -----------------------------------------------------------------

    ; Page table 0: maps 0x00000000 - 0x003FFFFF
    mov edi, page_table_0
    mov eax, PAGE_RW                ; Start at phys 0 | present+writable
    mov ecx, 1024
.fill_pt0:
    stosd                            ; Write entry
    add eax, PAGE_SIZE               ; Next 4KB page
    loop .fill_pt0

    ; Page table 1: maps 0x00400000 - 0x007FFFFF
    mov edi, page_table_1
    mov eax, (0x00400000 | PAGE_RW)
    mov ecx, 1024
.fill_pt1:
    stosd
    add eax, PAGE_SIZE
    loop .fill_pt1

    ; Page table 2: maps 0x00800000 - 0x00BFFFFF
    mov edi, page_table_2
    mov eax, (0x00800000 | PAGE_RW)
    mov ecx, 1024
.fill_pt2:
    stosd
    add eax, PAGE_SIZE
    loop .fill_pt2

    ; Page table 3: maps 0x00C00000 - 0x00FFFFFF
    mov edi, page_table_3
    mov eax, (0x00C00000 | PAGE_RW)
    mov ecx, 1024
.fill_pt3:
    stosd
    add eax, PAGE_SIZE
    loop .fill_pt3

    ; -----------------------------------------------------------------
    ; Step 3: Point page directory entries at the page tables
    ; -----------------------------------------------------------------
    mov eax, page_table_0
    or eax, PAGE_RW
    mov [page_directory + 0*4], eax

    mov eax, page_table_1
    or eax, PAGE_RW
    mov [page_directory + 1*4], eax

    mov eax, page_table_2
    or eax, PAGE_RW
    mov [page_directory + 2*4], eax

    mov eax, page_table_3
    or eax, PAGE_RW
    mov [page_directory + 3*4], eax

    ; -----------------------------------------------------------------
    ; Step 4: Initialize physical page bitmap
    ; Mark pages 0 to KERNEL_RESERVED_PAGES-1 (first 2MB) as used
    ; Mark the rest as free (up to available memory)
    ; -----------------------------------------------------------------
    call bitmap_init

    ; -----------------------------------------------------------------
    ; Step 5: Install page fault handler at IDT vector 14
    ; -----------------------------------------------------------------
    push dword 14
    push dword page_fault_handler
    call idt_set_gate
    add esp, 8

    ; -----------------------------------------------------------------
    ; Step 6: Load CR3 and enable paging
    ; -----------------------------------------------------------------
    mov eax, page_directory
    mov cr3, eax

    ; Set PG bit (bit 31) in CR0
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    ; Paging is now active - we're still running because of identity mapping

    pop edi
    pop esi
    pop edx
    pop ecx
    pop ebx
    pop ebp
    ret

; =============================================================================
; bitmap_init - Initialize the physical page bitmap
; Uses mem_total (KB) to determine how many pages exist
; =============================================================================
bitmap_init:
    push eax
    push ebx
    push ecx
    push edi

    ; Zero the entire bitmap first (all pages marked as used/nonexistent)
    mov edi, page_bitmap
    xor eax, eax
    mov ecx, (BITMAP_SIZE / 4)
    rep stosd

    ; Calculate total physical pages from mem_total (in KB)
    mov eax, [mem_total]
    shr eax, 2                      ; KB / 4 = number of 4KB pages
    cmp eax, MAX_PHYS_PAGES
    jle .cap_ok
    mov eax, MAX_PHYS_PAGES         ; Cap at max supported
.cap_ok:
    mov [phys_pages_total], eax

    ; Mark pages from KERNEL_RESERVED_PAGES to phys_pages_total as free
    ; Free = bit set to 1 in our bitmap
    mov ebx, KERNEL_RESERVED_PAGES  ; Start page index
    xor ecx, ecx                    ; Free counter
.mark_free:
    cmp ebx, eax                    ; Compare with total pages
    jge .done_marking

    ; Set bit ebx in bitmap
    push eax
    mov eax, ebx
    shr eax, 3                      ; Byte index = page / 8
    mov cl, bl
    and cl, 7                        ; Bit index = page % 8
    mov ch, 1
    shl ch, cl                       ; Create bit mask
    or [page_bitmap + eax], ch       ; Set the bit (mark as free)
    pop eax

    inc dword [phys_pages_free]
    inc ebx
    jmp .mark_free

.done_marking:
    pop edi
    pop ecx
    pop ebx
    pop eax
    ret

; =============================================================================
; page_alloc - Allocate a single physical page
; Returns: eax = physical address of allocated page, or 0 if out of memory
; =============================================================================
page_alloc:
    push ebx
    push ecx
    push edx

    ; Scan bitmap for first free page (bit = 1)
    mov ecx, [phys_pages_total]
    mov ebx, 0                      ; Current page index

.scan_loop:
    cmp ebx, ecx
    jge .alloc_fail

    ; Check bit ebx in bitmap
    mov eax, ebx
    shr eax, 3                      ; Byte index
    movzx edx, byte [page_bitmap + eax]
    mov eax, ebx
    and eax, 7                       ; Bit index
    bt edx, eax                      ; Test bit
    jc .found_free                   ; Carry set = bit is 1 = free

    inc ebx
    jmp .scan_loop

.found_free:
    ; Clear the bit (mark as used)
    mov eax, ebx
    shr eax, 3                      ; Byte index
    mov cl, bl
    and cl, 7                        ; Bit index
    mov ch, 1
    shl ch, cl                       ; Create bit mask
    not ch                           ; Invert mask
    and [page_bitmap + eax], ch      ; Clear the bit

    dec dword [phys_pages_free]

    ; Convert page index to physical address
    mov eax, ebx
    shl eax, PAGE_SHIFT              ; page_index * 4096

    pop edx
    pop ecx
    pop ebx
    ret

.alloc_fail:
    xor eax, eax                     ; Return 0 = out of memory
    pop edx
    pop ecx
    pop ebx
    ret

; =============================================================================
; page_free - Free a physical page
; Input: eax = physical address of page to free
; =============================================================================
page_free:
    push ebx
    push ecx

    ; Convert physical address to page index
    mov ebx, eax
    shr ebx, PAGE_SHIFT              ; phys_addr / 4096

    ; Bounds check
    cmp ebx, [phys_pages_total]
    jge .free_done
    cmp ebx, KERNEL_RESERVED_PAGES
    jl .free_done                     ; Don't free kernel pages

    ; Set the bit (mark as free)
    mov eax, ebx
    shr eax, 3                       ; Byte index
    mov cl, bl
    and cl, 7                         ; Bit index
    mov ch, 1
    shl ch, cl                        ; Create bit mask
    or [page_bitmap + eax], ch        ; Set the bit

    inc dword [phys_pages_free]

.free_done:
    pop ecx
    pop ebx
    ret

; =============================================================================
; page_get_free_count - Return number of free pages
; Returns: eax = number of free 4KB pages
; =============================================================================
page_get_free_count:
    mov eax, [phys_pages_free]
    ret

; =============================================================================
; page_fault_handler - ISR 14 handler
; x86 pushes error code before calling this handler
; CR2 contains the faulting virtual address
; =============================================================================
page_fault_handler:
    ; CPU pushed: [SS, ESP, EFLAGS, CS, EIP, error_code] (if from ring change)
    ; or: [EFLAGS, CS, EIP, error_code] (if same ring)
    ; Error code is at [esp]
    pushad

    ; Save error code (it's at esp+32 because pushad pushed 8 dwords)
    mov ebx, [esp + 32]

    ; Set color to red for error
    mov al, (COLOR_BLACK << 4) | COLOR_LRED
    call vga_set_color

    ; Print page fault message
    mov esi, pf_msg
    call vga_print

    ; Print fault address from CR2
    mov esi, pf_addr
    call vga_print
    mov eax, cr2
    call vga_print_hex

    ; Newline + error code
    push eax
    mov al, 10
    extern vga_putchar
    call vga_putchar
    pop eax

    mov esi, pf_code
    call vga_print
    mov eax, ebx
    call vga_print_hex

    ; Print halt message
    mov esi, pf_halt
    call vga_print

    ; Restore default color
    mov al, DEFAULT_COLOR
    call vga_set_color

    ; Halt - unrecoverable for now
    cli
    hlt
    jmp $ - 2
