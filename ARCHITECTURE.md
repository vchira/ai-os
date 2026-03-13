# AiOS - The AI-Native Operating System

> *An operating system built from first principles for a world where AI generates,
> composes, and executes software on the fly — written entirely in x86 assembly.*

## Vision

In the near future, users won't "install apps." They'll state intent — "edit this
photo," "analyze this dataset," "build me a dashboard" — and the OS will resolve
that intent into executable pipelines in microseconds. AiOS is the deterministic
substrate that makes this possible: a bare-metal x86 kernel that provides
hardware-enforced isolation, capability-based security, and composable verified
modules — so AI-generated code can run at full speed in a zero-trust environment.

**AiOS is not an AI bolted onto a traditional OS. It is an OS designed from
the ground up so that AI is the primary interface between human intent and
hardware execution.**

---

## Core Design Principles

### 1. Structural Enforcement over Code Auditing

AI-generated code can be opaque, high-density machine code — no human needs to
read it. Safety is not a property of the code; it's a property of the kernel:

- Every syscall is gated by a capability check (`caps.asm`)
- Each container has its own page directory (CR3) — physically unable to access
  memory it wasn't granted
- No network access unless explicitly wired via virtual bridges (`vnet.asm`)
- The "audit" is the hardware protection, not a human reading source code

### 2. Composition over Generation

The AI doesn't generate a TCP stack from scratch for every request. It acts as
a high-speed **JIT linker**, composing pre-compiled, formally verified assembly
modules:

- AiOS provides a library of verified "blocks" (networking, graphics, crypto)
- AI selects, parameterizes, and links blocks into task-specific pipelines
- Only the "glue logic" between blocks is generated on the fly
- Composition is microseconds; generation from scratch is seconds

### 3. Scoped Context via Privacy Kernel

The AI needs context to be useful, but context must be controlled:

- `CAP_CONTEXT_READ` grants scoped, virtualized access to specific data schemas
- Containers processing user intent have **no outbound network** by default
- Results are staged in an "output sandbox" until the user approves
- Context never leaves the device; inference runs locally or in a sealed enclave

---

## Full System Architecture

```
+=====================================================================+
|                     USER INTENT LAYER                                |
|                                                                      |
|  Intent Resolver        →  "edit this photo" → task decomposition    |
|  Module Composer (JIT)  →  Links verified blocks into pipelines      |
|  Privacy Kernel         →  Scoped context, output sandbox            |
+=====================================================================+
|                     CONTAINERIZED EXECUTION                          |
|                                                                      |
|  +--Container Group A (compose)---+  +--Container Group B---------+ |
|  |                                |  |                            | |
|  | +-----------+ +-----------+    |  | +-----------+              | |
|  | | Agent 1   | | Agent 2   |    |  | | Agent 3   |              | |
|  | | (web srv) | | (db)      |    |  | | (editor)  |              | |
|  | +-----+-----+ +-----+-----+   |  | +-----------+              | |
|  |       |              |         |  |                            | |
|  |  +----+----+  +------+------+  |  |  No network access to     | |
|  |  |vNIC br0 |  |vNIC br0    |  |  |  Group A (isolated)       | |
|  |  +---------+  +------------+   |  |                            | |
|  |  Private bridge: 10.0.1.0/24  |  |  Private bridge: 10.0.2.0  | |
|  +--------------------------------+  +----------------------------+ |
|        |                                                             |
|        | (only if port-mapped)                                       |
+========|============================================================+
|  SYSTEM CALL INTERFACE (INT 0x80)                                    |
|  sys_write, sys_read, sys_exec, sys_exit, sys_fork, sys_mmap,       |
|  sys_socket, sys_bind, sys_listen, sys_accept, sys_connect,         |
|  sys_send, sys_recv, sys_container_create, sys_container_destroy,    |
|  sys_intent_submit, sys_compose_pipeline, sys_inference_submit       |
+======================================================================+
|                          AiOS KERNEL                                  |
|                                                                       |
|  +--PROCESS & CONTAINER MANAGEMENT--+  +--MEMORY MANAGEMENT--------+ |
|  |                                  |  |                            | |
|  |  Process scheduler (round-robin) |  |  Physical page allocator   | |
|  |  Context switching (TSS-based)   |  |  (bitmap, 4KB pages)       | |
|  |  Container runtime:              |  |                            | |
|  |   - Isolated page directories    |  |  Virtual memory (paging)   | |
|  |   - Per-container resource limits |  |  (CR3 per process,        | |
|  |   - Capability-based security    |  |   page tables, PDE/PTE)   | |
|  |   - Compose groups (shared net)  |  |                            | |
|  |                                  |  |  Kernel heap allocator     | |
|  +----------------------------------+  +----------------------------+ |
|                                                                       |
|  +--NETWORKING STACK----------------+  +--GRAPHICS SUBSYSTEM--------+ |
|  |                                  |  |                            | |
|  |  NIC driver (virtio-net/e1000)   |  |  VESA/VBE framebuffer     | |
|  |  Ethernet / ARP / IPv4           |  |  Pixel primitives          | |
|  |  ICMP / UDP / TCP                |  |  Bitmap font renderer     | |
|  |  DHCP client / DNS resolver      |  |  Window compositor         | |
|  |                                  |  |  Window manager            | |
|  |  Virtual networking:             |  |                            | |
|  |   - vNIC per container           |  |  (AI generates layouts;   | |
|  |   - Software bridge per group    |  |   AiOS provides the       | |
|  |   - NAT + port mapping           |  |   rendering canvas)       | |
|  +----------------------------------+  +----------------------------+ |
|                                                                       |
|  +--AI RUNTIME----------------------+  +--IPC + SECURITY-----------+ |
|  |  Intent resolver                 |  |  Message passing           | |
|  |  Module composer (JIT linker)    |  |  Shared memory (mapped)   | |
|  |  Privacy kernel (context scope)  |  |  Capability enforcement   | |
|  |  NPU/inference bridge            |  |  Audit log                | |
|  +----------------------------------+  +----------------------------+ |
|                                                                       |
|  +--DEVICE DRIVERS----------------------------------------------+    |
|  |  VGA text (80x25)  |  VESA framebuffer  |  PS/2 mouse        |    |
|  |  PS/2 keyboard     |  PIT timer (100Hz) |  virtio-net / e1000|    |
|  |  ATA/ATAPI (CD)    |  Serial (COM1)     |  PCI bus / NPU     |    |
|  +---------------------------------------------------------------+    |
|                                                                       |
|  +--CORE TABLES-------------------------------------------------+    |
|  |  GDT (segments + TSS)  |  IDT (256 vectors)  |  PIC remap    |    |
|  +---------------------------------------------------------------+    |
+======================================================================+
|  GRUB2 BOOTLOADER (Multiboot protocol)                               |
+======================================================================+
|  HARDWARE: CPU (i386+) | RAM | GPU/NPU | NIC | PIC | PIT | PS/2     |
+======================================================================+
```

---

## Container Isolation Architecture

AiOS containers provide hardware-enforced process isolation — the same
mechanism that makes AI-generated code safe to execute without auditing.

### Isolation Mechanisms

```
+--Container---------------------------+
|  Page Directory (CR3)                |  <-- Separate virtual address space
|  +--------------------------------+  |
|  | User pages: private, not       |  |
|  | visible to other containers    |  |
|  +--------------------------------+  |
|  | Kernel pages: mapped read-only |  |
|  | (shared, for syscall entry)    |  |
|  +--------------------------------+  |
|                                      |
|  Capabilities (bitmask):             |  <-- What this container can do
|    NET_BIND, NET_CONNECT,            |
|    FS_READ, FS_WRITE, GPU_ACCESS,    |
|    CONTEXT_READ, SPAWN, HOST_NET     |
|                                      |
|  Resource Limits:                    |  <-- Enforced by scheduler + allocator
|    max_pages, max_cpu_ms,            |
|    max_open_fds, max_children        |
|                                      |
|  Network Namespace:                  |  <-- Virtual NIC + routing
|    vNIC (MAC, IP via bridge/DHCP)    |
|    Routing table (bridge-local)      |
+--------------------------------------+
```

### Container Groups (Compose Model)

A container group is a set of containers that share a private virtual network
bridge. Agents within a group can communicate freely; agents in different groups
are invisible to each other unless ports are explicitly mapped.

```
  Container Group "webapp"                    Host / Other Groups
  ========================                    ====================

  +---------+    +---------+
  | web:80  |    | db:5432 |    These can talk to each other
  +----+----+    +----+----+    freely on bridge 10.0.1.0/24
       |              |
  +----+--------------+----+
  |   Virtual Bridge        |    Private network, not routable
  |   10.0.1.0/24           |    from outside the group
  +------------+------------+
               |
               | ONLY if port-mapped:
               | "80:80" in compose config
               |
  +------------+------------+
  |   Host Network Stack     |    NAT / port forwarding
  +-------------------------+
```

### Compose File Format

```
# /etc/compose/webapp.conf
group webapp
  container web
    image /apps/webserver.bin
    memory 4M
    ports 80:80
    caps NET_BIND,NET_CONNECT
    net bridge0
  end
  container db
    image /apps/database.bin
    memory 8M
    caps FS_READ,FS_WRITE
    net bridge0
  end
end
```

### Container Lifecycle

1. `sys_container_create(config)` — Kernel allocates:
   - New page directory (cloned from kernel template)
   - Process control block with container metadata
   - Virtual NIC (if networking requested)
   - Attaches vNIC to specified bridge
2. Kernel loads binary into container's address space
3. Sets `CR3` to container's page directory
4. Jumps to user mode (`ring 3`) at binary entry point
5. Container runs isolated — all hardware access via syscalls only
6. `sys_container_destroy(id)` — Kernel reclaims all resources

---

## The AI-Native Stack

This is how user intent becomes hardware execution:

```
User: "edit this photo"
         |
         v
+--Intent Resolver--------------------------------------+
|  Parses natural language into structured intent        |
|  Decomposes into sub-tasks:                            |
|    1. Load image data (FS block)                       |
|    2. Apply filter (graphics block)                    |
|    3. Present result (compositor block)                |
+------------------------------|-------------------------+
                               v
+--Module Composer (JIT Linker)-----------------------------+
|  Selects pre-compiled, verified assembly blocks:          |
|    fs_read.block + image_decode.block + filter.block      |
|  Generates glue code (trampolines between blocks)         |
|  Maps everything into a new container's address space      |
+------------------------------|----------------------------+
                               v
+--Privacy Kernel------------------------------------------+
|  Grants CAP_FS_READ scoped to /photos/input.jpg          |
|  Grants CAP_GPU_ACCESS for framebuffer rendering          |
|  Denies CAP_NET_CONNECT (no data exfiltration)            |
|  Output staged in sandbox until user approves             |
+------------------------------|---------------------------+
                               v
+--Container Runtime---------------------------------------+
|  New page directory (CR3), isolated memory                |
|  Capability mask: FS_READ | GPU_ACCESS                    |
|  No vNIC (network disabled for this task)                 |
|  Executes composed pipeline at ring 3                     |
|  Result: edited image in output sandbox                   |
+------------------------------|---------------------------+
                               v
User sees result, approves → saved to /photos/output.jpg
```

### AI Runtime Components

```
kernel/intent.asm
  - Intent resolver: parses declarative requests into syscall sequences
  - AI Interaction Protocol (AIP): structured message format between agents
  - Priority queue for intent dispatch

kernel/composer.asm
  - Module registry: catalog of pre-compiled verified blocks
  - JIT linker: maps selected blocks into a container's address space
  - Glue code injection: AI-generated trampolines between blocks
  - Dependency resolution: block A requires block B's exports

kernel/privacy.asm
  - Context mediator: virtualizes user data for AI consumption
  - Data schema projections (AI sees structure, not raw bytes)
  - Output sandbox: staged results awaiting user approval
  - Audit log: which container accessed what context, when

drivers/npu.asm
  - NPU/GPU inference dispatch (for local AI models)
  - Tensor buffer management (DMA to accelerator)
  - Model loading (weights from verified module store)
  - Fallback: CPU-based inference (slow but always available)
```

### AI-Specific Syscalls

```
30  sys_intent_submit(intent_msg, len)       - Submit a user intent
31  sys_intent_status(intent_id)             - Check intent resolution status
32  sys_compose_pipeline(block_ids[], count)  - Compose verified blocks
33  sys_context_request(scope, schema_ptr)    - Request scoped data access
34  sys_context_release(scope)                - Release data scope
35  sys_output_stage(buf, len)                - Stage output for approval
36  sys_output_approve(staged_id)             - Approve staged output
37  sys_inference_submit(model, in, in_len, out, out_len) - Run inference
```

---

## Current Implementation (v0.1 — The Deterministic Substrate)

The kernel boots, initializes all hardware, enables paging, and drops into an
interactive shell. This is the foundation on which everything above will be built.

### Boot Sequence

1. **GRUB2** loads the Multiboot-compliant ELF kernel from CD-ROM ISO
2. **_start** (boot/boot.asm):
   - Sets up 16KB kernel stack (BSS-allocated)
   - Passes multiboot magic + info pointer to kernel_main
3. **kernel_main** (kernel/kernel.asm):
   - Initializes VGA text mode driver (80x25, 16 colors)
   - Initializes GDT (flat memory model, kernel code + data segments)
   - Initializes memory manager (reads multiboot info, fallback 16MB)
   - Initializes IDT + remaps PIC (8259) to IRQ 32-47
   - **Enables paging** (identity-maps first 16MB, bitmap page allocator)
   - Initializes PIT timer at 100 Hz
   - Initializes PS/2 keyboard driver
   - Prints boot status messages and ASCII banner
   - Launches interactive command shell

### Current Components

| File | Status | Description |
|------|--------|-------------|
| `boot/boot.asm` | DONE | Multiboot entry, stack setup, calls kernel_main |
| `kernel/kernel.asm` | DONE | Master init sequence, boot messages, banner |
| `kernel/gdt.asm` | DONE | GDT: null + kernel code + kernel data segments |
| `kernel/idt.asm` | DONE | 256-entry IDT, PIC remap, ISR/IRQ handlers |
| `kernel/memory.asm` | DONE | Memory info from multiboot (total/used/free) |
| `kernel/paging.asm` | DONE | Page directory, identity map 16MB, bitmap allocator, page fault handler |
| `drivers/vga.asm` | DONE | 80x25 text: putchar, print, hex, dec, scroll, cursor, color |
| `drivers/keyboard.asm` | DONE | PS/2 scancode set 1, modifiers, 256-byte ring buffer |
| `drivers/timer.asm` | DONE | PIT channel 0 at 100Hz, tick counter |
| `shell/shell.asm` | DONE | Command shell with built-in commands |
| `include/constants.inc` | DONE | Shared constants (VGA, PIC, PIT, paging, colors) |
| `linker.ld` | DONE | Linker script: sections at 1MB, kernel_end symbol |

### Shell Commands

| Command | Description |
|---------|-------------|
| `help` | List available commands |
| `clear` | Clear the screen |
| `echo` | Print text to screen |
| `meminfo` | Display memory information |
| `cpuinfo` | Display CPU vendor and features |
| `reboot` | Reboot the system |
| `halt` | Halt the CPU |
| `color` | Change text colors |
| `uptime` | Show system uptime |
| `ver` | Show OS version |
| `ls` | List commands (alias for help) |

---

## Hybrid Build: Assembly Core + C Libraries

AiOS uses a practical hybrid approach: the kernel core and hardware drivers are
written in x86 NASM assembly for full control. Complex protocol stacks (TCP/IP,
TLS, JSON) are pulled in as freestanding C libraries — battle-tested code that
links directly with the assembly objects.

```
What stays in assembly (the substrate):
  Boot, GDT, IDT, PIC, paging, page allocator
  VGA driver, keyboard driver, timer driver, serial driver
  NIC driver (hardware-specific, needs port I/O / MMIO)
  Shell (human debug interface)

What comes from C libraries (the protocol stack):
  lwIP        - TCP/IP (ARP, IPv4, TCP, UDP, DHCP, DNS)  ~30KB
  BearSSL     - TLS 1.2 (AES, SHA-256, ECDHE, X.509)    ~80KB
  cJSON       - JSON parsing/generation                    ~8KB

What is new C code (the glue):
  lib/heap.c          - Heap allocator on top of page_alloc
  lib/http_client.c   - Minimal HTTP/1.1 client
  lib/claude_api.c    - Claude API wrapper (format/parse JSON)
  lib/nic_glue.c      - Connects asm NIC driver to lwIP
```

### How It Links

NASM `.o` and GCC `.o` files share the same i386 cdecl calling convention and
link together with the same `ld` and linker script:

```
NASM assembly (.asm)              GCC freestanding C (.c)
     |                                  |
  nasm -f elf32                  gcc -m32 -ffreestanding -nostdlib -c
     |                                  |
     v                                  v
  kernel.o, drivers.o, ...        lwip.o, bearssl.o, heap.o, ...
     |                                  |
     +-------------- ld ---------------+
                     |
                 aios.bin (ELF)
```

Assembly functions call C (push args, call, clean stack). C functions call
assembly (declared as `extern`). Same address space, same binary.

### What C libraries need from the kernel

```c
// AiOS assembly exports these (already implemented or trivial):
void *page_alloc(void);              // Returns 4KB physical page
void  page_free(void *addr);         // Free a physical page
uint32_t timer_get_ticks(void);      // PIT tick counter

// New, needed for C integration:
void *malloc(size_t size);           // Heap allocator (lib/heap.c)
void  free(void *ptr);              // Heap free
void *memcpy(void *d, const void *s, size_t n);  // lib/string.c
void *memset(void *s, int c, size_t n);           // lib/string.c
```

---

## Roadmap

### Phase 1: Virtual Memory & Paging -- COMPLETE

- Identity-mapped first 16MB (4 page tables)
- Physical page allocator (bitmap-based, tracks up to 64MB)
- Page fault handler (ISR 14) with diagnostic output
- CR3 load + CR0.PG enable

### Phase 2: Serial Bridge to Claude (VM fast-path)

**Goal**: Get AiOS talking to Claude immediately via COM1 serial port

```
Assembly:
  drivers/serial.asm          - COM1 UART driver (I/O ports 0x3F8-0x3FD)
  shell/shell.asm             - Add "ask" command (send to serial, print response)

Host side:
  tools/claude_proxy.py       - Reads serial, calls Claude API, writes response
```

This gives a working AI-native demo in a VM: `aios> ask "what is 2+2"` → serial
→ host proxy → Claude API → response displayed on screen.

### Phase 3: Heap + C Runtime Foundation

**Goal**: Enable linking C libraries into the kernel

```
lib/heap.c                    - malloc/free on top of page_alloc (~100 lines)
lib/string.c                  - memcpy, memset, memcmp, strlen (~50 lines)
kernel/heap.asm               - Assembly exports for C heap integration
Makefile                      - Add gcc cross-compilation rules
```

### Phase 4: Networking (lwIP + NIC driver)

**Goal**: TCP/IP on real hardware via C library + assembly NIC driver

```
Assembly:
  drivers/pci.asm             - PCI bus enumeration (config space ports)
  drivers/virtio_net.asm      - virtio-net NIC driver (QEMU/KVM)
                                (or drivers/rtl8139.asm for real hardware)

C (lwIP integration):
  lib/nic_glue.c              - Connects asm NIC driver to lwIP callbacks
  lib/lwip/                   - lwIP source (compiled freestanding)
```

After this phase: AiOS can ping, do DHCP, open TCP connections.

### Phase 5: TLS + Claude API (real hardware path)

**Goal**: HTTPS to api.anthropic.com from bare metal

```
C (BearSSL + HTTP):
  lib/bearssl/                - BearSSL source (compiled freestanding)
  lib/http_client.c           - Minimal HTTP/1.1 over TLS
  lib/claude_api.c            - Claude Messages API wrapper
  lib/cjson/                  - cJSON for request/response parsing
```

After this phase: AiOS can talk to Claude from real hardware. No proxy needed.

**Alternative**: LAN proxy mode. AiOS sends plain TCP to a local machine that
handles TLS. Useful when BearSSL integration is still in progress.

### Phase 6: User Mode & System Calls

**Goal**: Ring 3 execution with controlled kernel entry

```
kernel/syscall.asm            - INT 0x80 handler, dispatch table
kernel/gdt.asm                - Add user code/data segments (ring 3) + TSS
kernel/process.asm            - PCB, context switch, round-robin scheduler
```

Syscall table:
```
 0  sys_exit         5  sys_waitpid     10  sys_socket
 1  sys_write        6  sys_mmap        11  sys_bind
 2  sys_read         7  sys_munmap      12  sys_connect
 3  sys_exec         8  sys_getpid      13  sys_send
 4  sys_fork         9  sys_sleep       14  sys_recv
```

### Phase 7: Container Runtime

**Goal**: Hardware-isolated execution environments for AI agents

```
kernel/container.asm          - Container/group lifecycle, CCB management
kernel/caps.asm               - Capability bitmask enforcement on every syscall
```

Capabilities:
```
CAP_NET_BIND     0x01    CAP_FS_READ      0x08    CAP_SPAWN       0x40
CAP_NET_CONNECT  0x02    CAP_FS_WRITE     0x10    CAP_HOST_NET    0x80
CAP_NET_RAW      0x04    CAP_GPU_ACCESS   0x20    CAP_CONTEXT     0x100
```

### Phase 8: Graphics Subsystem

**Goal**: Framebuffer canvas for AI-generated UIs

```
drivers/vesa.asm              - VESA/VBE linear framebuffer (1024x768x32)
drivers/mouse.asm             - PS/2 mouse (IRQ12)
graphics/draw.asm             - Primitives: pixel, line, rect, fill, blit
graphics/font.asm             - PSF bitmap font renderer
graphics/compositor.asm       - Z-ordered window compositor
graphics/wm.asm               - Window manager
```

### Phase 9: AI Runtime

**Goal**: The intent-to-execution pipeline

```
kernel/intent.asm             - Intent resolver + AIP protocol
kernel/composer.asm           - Module registry + JIT linker
kernel/privacy.asm            - Context mediator + output sandbox + audit log
```

### Phase 10: Key-Value Storage

**Goal**: AI-native persistent storage (not a traditional filesystem)

```
drivers/ata.asm               - ATA disk driver (PIO mode)
storage/kv.asm                - Key-value store on block device
storage/cas.asm               - Content-addressable storage (hash → blob)
storage/index.c               - Semantic index (vector similarity, C library)
```

---

## Implementation Dependencies

```
Phase 1: Paging ──────────────────┐  <-- COMPLETE
                                  v
Phase 2: Serial Bridge ───────────┤  <-- AI works in VM
                                  v
Phase 3: Heap + C Runtime ────────┤  <-- Enables C libraries
                                  v
              ┌───────────────────┴───────────────────┐
              v                                       v
Phase 4: Networking (lwIP)              Phase 6: User Mode
         PCI + NIC driver                        Syscalls
         TCP/IP via lwIP                         Processes
              │                                  Scheduler
              v                                       │
Phase 5: TLS + Claude API                             v
         BearSSL + HTTP                   Phase 7: Containers
         Claude on real hardware                  Isolation
              │                                   Compose groups
              │                                       │
              └───────────────────┬───────────────────┘
                                  v
                    Phase 8: Graphics (VESA + compositor)
                                  │
                                  v
                    Phase 9: AI Runtime (intent → execution)
                                  │
                                  v
                    Phase 10: Storage (KV + CAS + vector index)
```

---

## Memory Layout

### Physical Memory (current)

```
0x00000000 - 0x000003FF  Real Mode IVT (unused in protected mode)
0x00000400 - 0x000004FF  BIOS Data Area
0x000B8000 - 0x000B8F9F  VGA text mode buffer (memory-mapped I/O)
0x00100000 - 0x0010FFFF  Kernel: .multiboot, .text, .rodata, .data, .bss
                          (loaded by GRUB at 1MB, ~64KB currently)
0x00108000 - 0x0010CFFF  Page directory + 4 page tables (in BSS)
0x0010D000 - 0x0010D7FF  Physical page bitmap (in BSS)
```

### Virtual Address Space (planned, after higher-half remap)

```
0x00000000 - 0xBFFFFFFF  User space (3 GB, private per container)
0xC0000000 - 0xFFFFFFFF  Kernel space (1 GB, shared, ring 0 only)
```

---

## Build System

- **Assembler**: NASM (Netwide Assembler)
- **C Compiler**: GCC (cross-compile: `-m32 -ffreestanding -nostdlib`)
- **Linker**: GNU ld with custom linker script (`linker.ld`)
- **ISO Creation**: grub2-mkrescue + xorriso
- **Target**: i386 (32-bit protected mode)
- **Boot Protocol**: Multiboot (GRUB2)
- **VM**: QEMU-KVM via GNOME Boxes / libvirt

### Build Pipeline

```
boot/*.asm     →  nasm -f elf32  →  *.o  ─┐
kernel/*.asm   →  nasm -f elf32  →  *.o  ─┤
drivers/*.asm  →  nasm -f elf32  →  *.o  ─┤
shell/*.asm    →  nasm -f elf32  →  *.o  ─┤→  ld (linker.ld)  →  aios.bin (ELF)
lib/*.c        →  gcc -m32 -ffreestanding →  *.o  ─┤
lib/lwip/*.c   →  gcc -m32 -ffreestanding →  *.o  ─┤
lib/bearssl/*.c→  gcc -m32 -ffreestanding →  *.o  ─┘

grub2-mkrescue  →  aios.iso (bootable CD-ROM with GRUB2)
```

### Building & Running

```bash
make          # Build the ISO
make clean    # Remove build artifacts
make run      # Build and run in QEMU
```

---

## Directory Structure

```
asm-ai/
  boot/
    boot.asm                Multiboot entry point
  kernel/
    kernel.asm              Kernel main + init sequence
    gdt.asm                 Global Descriptor Table
    idt.asm                 Interrupt Descriptor Table + PIC
    memory.asm              Memory manager
    paging.asm              Virtual memory, page tables, page allocator
    heap.asm                [PLANNED] Assembly exports for heap integration
    process.asm             [PLANNED] Process management, scheduler
    syscall.asm             [PLANNED] INT 0x80 syscall dispatcher
    container.asm           [PLANNED] Container runtime
    caps.asm                [PLANNED] Capability-based security
    intent.asm              [PLANNED] Intent resolver + AIP
    composer.asm            [PLANNED] Module composer (JIT linker)
    privacy.asm             [PLANNED] Privacy kernel + context mediator
  drivers/
    vga.asm                 VGA 80x25 text mode driver
    keyboard.asm            PS/2 keyboard driver
    timer.asm               PIT timer driver
    serial.asm              [PLANNED] COM1 UART serial driver
    pci.asm                 [PLANNED] PCI bus enumeration
    virtio_net.asm          [PLANNED] virtio-net NIC driver
    rtl8139.asm             [PLANNED] RTL8139 NIC driver (real hardware)
    mouse.asm               [PLANNED] PS/2 mouse driver
    vesa.asm                [PLANNED] VESA framebuffer driver
    ata.asm                 [PLANNED] ATA disk driver
  lib/                        (C code — freestanding, linked with asm)
    heap.c                  [PLANNED] malloc/free on top of page_alloc
    string.c                [PLANNED] memcpy, memset, strlen, etc.
    nic_glue.c              [PLANNED] Connects asm NIC driver to lwIP
    http_client.c           [PLANNED] Minimal HTTP/1.1 client
    claude_api.c            [PLANNED] Claude Messages API wrapper
    lwip/                   [PLANNED] lwIP TCP/IP stack (vendored)
    bearssl/                [PLANNED] BearSSL TLS library (vendored)
    cjson/                  [PLANNED] cJSON parser (vendored)
  graphics/
    draw.asm                [PLANNED] Drawing primitives
    font.asm                [PLANNED] Bitmap font renderer
    compositor.asm          [PLANNED] Window compositor
    wm.asm                  [PLANNED] Window manager
  storage/
    kv.asm                  [PLANNED] Key-value store
    cas.asm                 [PLANNED] Content-addressable storage
    index.c                 [PLANNED] Semantic vector index
  shell/
    shell.asm               Command shell
  tools/
    claude_proxy.py         [PLANNED] Host-side serial-to-Claude proxy
  include/
    constants.inc           Shared constants
  iso/
    boot/grub/grub.cfg      GRUB configuration
  linker.ld                 Linker script
  Makefile                  Build system
  ARCHITECTURE.md           This file
  PLAN.md                   Implementation plan and task tracking
```
