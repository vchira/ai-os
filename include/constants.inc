; =============================================================================
; AsmOS Global Constants
; =============================================================================

; VGA text mode
VGA_BUFFER      equ 0xB8000
VGA_WIDTH       equ 80
VGA_HEIGHT      equ 25
VGA_SIZE        equ VGA_WIDTH * VGA_HEIGHT

; VGA colors
COLOR_BLACK     equ 0x0
COLOR_BLUE      equ 0x1
COLOR_GREEN     equ 0x2
COLOR_CYAN      equ 0x3
COLOR_RED       equ 0x4
COLOR_MAGENTA   equ 0x5
COLOR_BROWN     equ 0x6
COLOR_LGRAY     equ 0x7
COLOR_DGRAY     equ 0x8
COLOR_LBLUE     equ 0x9
COLOR_LGREEN    equ 0xA
COLOR_LCYAN     equ 0xB
COLOR_LRED      equ 0xC
COLOR_LMAGENTA  equ 0xD
COLOR_YELLOW    equ 0xE
COLOR_WHITE     equ 0xF

; Default color: light gray on black
DEFAULT_COLOR   equ (COLOR_BLACK << 4) | COLOR_LGRAY

; PIC ports
PIC1_CMD        equ 0x20
PIC1_DATA       equ 0x21
PIC2_CMD        equ 0xA0
PIC2_DATA       equ 0xA1

; PIT ports
PIT_CH0         equ 0x40
PIT_CMD         equ 0x43
PIT_FREQ        equ 1193180     ; Base PIT frequency
PIT_HZ          equ 100         ; Target frequency: 100 Hz

; Keyboard ports
KBD_DATA        equ 0x60
KBD_STATUS      equ 0x64

; Keyboard buffer size
KBD_BUF_SIZE    equ 256

; IRQ numbers (remapped)
IRQ0            equ 32          ; Timer
IRQ1            equ 33          ; Keyboard
IRQ12           equ 44          ; Mouse

; Multiboot magic
MULTIBOOT_MAGIC equ 0x2BADB002

; Paging
PAGE_SIZE       equ 4096
PAGE_SHIFT      equ 12
PAGE_PRESENT    equ 0x01
PAGE_WRITABLE   equ 0x02
PAGE_USER       equ 0x04
PAGE_RW         equ (PAGE_PRESENT | PAGE_WRITABLE)           ; 0x03 - kernel R/W
PAGE_RW_USER    equ (PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER) ; 0x07 - user R/W
PAGES_PER_TABLE equ 1024
PAGE_DIR_ENTRIES equ 1024

; Maximum physical memory supported (64 MB default for VM)
MAX_PHYS_MB     equ 64
MAX_PHYS_PAGES  equ (MAX_PHYS_MB * 256)    ; 16384 pages
BITMAP_SIZE     equ (MAX_PHYS_PAGES / 8)   ; 2048 bytes

; Kernel reserved region (first 2MB is off-limits to allocator)
KERNEL_RESERVED_PAGES equ 512              ; 2MB / 4KB

; OS version
OS_VERSION_MAJOR equ 1
OS_VERSION_MINOR equ 0
