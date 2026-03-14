/*
 * AiOS — ARM Boot Code (Raspberry Pi 3/4/5)
 * Entry point: _start
 *
 * The RPi GPU loads kernel8.img (AArch64) or kernel7.img (AArch32) to 0x80000
 * (64-bit) or 0x8000 (32-bit). We target AArch32 (ARMv7) for RPi 3/4
 * compatibility (32-bit mode).
 *
 * Boot sequence:
 *   1. GPU loads kernel7.img to 0x8000 on all 4 cores
 *   2. Core 0 runs this code; cores 1-3 are parked
 *   3. We set up stack, clear BSS, call kernel_main_arm
 */

.section .text.boot
.global _start

_start:
    /* Only proceed on core 0; park other cores */
    mrc p15, 0, r0, c0, c0, 5    @ MPIDR register
    and r0, r0, #3                @ core ID
    cmp r0, #0
    bne park_core

    /* Set up stack pointer (grow down from 0x8000) */
    ldr sp, =0x8000

    /* Clear BSS */
    ldr r0, =__bss_start
    ldr r1, =__bss_end
    mov r2, #0
bss_clear:
    cmp r0, r1
    bge bss_done
    str r2, [r0], #4
    b bss_clear
bss_done:

    /* Set up exception vector table */
    ldr r0, =vector_table
    mcr p15, 0, r0, c12, c0, 0   @ VBAR

    /* Enable interrupts */
    cpsie if

    /* Jump to C kernel entry */
    bl kernel_main_arm

    /* Should not return */
park_core:
    wfe
    b park_core

/*
 * Exception vector table (ARMv7)
 */
.align 5
vector_table:
    ldr pc, =_start           @ Reset
    ldr pc, =undef_handler    @ Undefined instruction
    ldr pc, =svc_handler      @ SVC (supervisor call)
    ldr pc, =prefetch_handler @ Prefetch abort
    ldr pc, =data_handler     @ Data abort
    nop                       @ Reserved
    ldr pc, =irq_handler      @ IRQ
    ldr pc, =fiq_handler      @ FIQ

undef_handler:
svc_handler:
prefetch_handler:
data_handler:
fiq_handler:
    b park_core               @ Hang on unhandled exceptions

.global irq_handler
irq_handler:
    push {r0-r12, lr}
    bl arm_irq_dispatch
    pop {r0-r12, lr}
    subs pc, lr, #4
