# AiOS Implementation Plan

Practical task list for building AiOS from its current state (v0.1, booting with
paging) to a working AI-native OS that can talk to Claude.

---

## Phase 1: Paging & Memory -- COMPLETE

- [x] Page directory + 4 page tables (identity-map 16MB)
- [x] Physical page bitmap allocator (up to 64MB)
- [x] Page fault handler (ISR 14)
- [x] Enable paging (CR3 + CR0.PG)
- [x] Boot messages, banner, interactive shell

---

## Phase 2: Serial Bridge to Claude (VM fast-path)

**Goal**: AiOS sends user input via COM1 serial → host proxy → Claude API →
response displayed on screen. First working AI interaction.

### 2.1 COM1 Serial Driver (assembly)

File: `drivers/serial.asm`

- [ ] Initialize COM1 UART (0x3F8)
  - Set baud rate (115200)
  - 8 data bits, no parity, 1 stop bit (8N1)
  - Enable FIFO (FCR)
  - Disable interrupts initially (polling mode)
- [ ] `serial_init` — called from kernel_main
- [ ] `serial_write_byte(al)` — send one byte (poll TX ready)
- [ ] `serial_read_byte() -> al` — read one byte (poll RX ready)
- [ ] `serial_write_string(esi)` — send null-terminated string
- [ ] `serial_read_line(edi, max_len) -> ecx` — read until newline
- [ ] `serial_data_available() -> ZF` — check if data waiting
- [ ] Add serial constants to `include/constants.inc`
- [ ] Add `serial_init` call to kernel_main, with boot message
- [ ] Add `drivers/serial.asm` to Makefile

### 2.2 Shell "ask" Command (assembly)

File: `shell/shell.asm` (modify)

- [ ] Add `ask` command: `ask <question>`
  - Sends the question as a line over serial (newline-terminated)
  - Reads response lines from serial until delimiter (e.g., `\x04` ETX)
  - Prints response to VGA with color formatting
- [ ] Add `ai` as alias for `ask`
- [ ] Handle timeout (if no response within ~10 seconds, print error)
- [ ] Update help text

### 2.3 Host Proxy Script (Python)

File: `tools/claude_proxy.py`

- [ ] Open serial port (for VM: connect to QEMU's serial socket/pty)
- [ ] Read line from serial (user's question)
- [ ] Call Claude API (`anthropic` Python SDK)
  - System prompt: "You are AiOS, an AI-native operating system. Keep responses
    concise and terminal-friendly (no markdown, max 80 chars wide)."
  - Send user question as message
- [ ] Write response back to serial, terminated with `\x04`
- [ ] Loop forever (handle multiple questions)
- [ ] Document how to configure QEMU serial pass-through
- [ ] Document how to set `ANTHROPIC_API_KEY`

### 2.4 VM Configuration

- [ ] Update VM XML or QEMU flags to expose COM1 as a socket/pty
  - For libvirt: `<serial type='pty'>` or `<serial type='unix'>`
  - For QEMU: `-serial unix:/tmp/aios-serial,server,nowait`
- [ ] Document the setup in ARCHITECTURE.md
- [ ] Test end-to-end: boot AiOS → type `ask hello` → see Claude response

### 2.5 Milestone Deliverable

Boot AiOS in GNOME Boxes, type `ask "what is the meaning of life"`, see
Claude's response rendered on the VGA text screen.

---

## Phase 3: Heap + C Runtime Foundation

**Goal**: Enable linking freestanding C code into the kernel.

### 3.1 Minimal C Library Functions

File: `lib/string.c`

- [ ] `memcpy(dst, src, n)`
- [ ] `memset(dst, val, n)`
- [ ] `memcmp(a, b, n)`
- [ ] `strlen(s)`
- [ ] `strcmp(a, b)`
- [ ] `strncpy(dst, src, n)`

### 3.2 Heap Allocator

File: `lib/heap.c`

- [ ] Simple first-fit allocator
- [ ] `heap_init(base_addr, size)` — initialize heap region
- [ ] `malloc(size)` — allocate from heap
- [ ] `free(ptr)` — return to heap (coalesce adjacent free blocks)
- [ ] `calloc(n, size)` — zero-initialized allocation
- [ ] `realloc(ptr, size)` — resize

Assembly glue (`kernel/heap.asm`):
- [ ] Call `page_alloc` to give heap a pool of pages
- [ ] Export `heap_init` call, integrate into kernel_main

### 3.3 Build System Update

File: `Makefile`

- [ ] Add GCC cross-compilation rules (`-m32 -ffreestanding -nostdlib -O2`)
- [ ] Add `lib/*.c` → `build/lib/*.o` pattern
- [ ] Link C objects alongside assembly objects
- [ ] Test: compile and link a trivial C function, call it from assembly

### 3.4 Milestone Deliverable

A C function called from assembly that uses `malloc`, prints to VGA via the
assembly `vga_print` export, and frees memory. Proves the hybrid build works.

---

## Phase 4: Networking (lwIP + NIC Driver)

**Goal**: TCP/IP stack on real and virtual hardware.

### 4.1 PCI Bus Enumeration (assembly)

File: `drivers/pci.asm`

- [ ] Read PCI config space (ports 0xCF8/0xCFC)
- [ ] Enumerate all devices on bus 0 (brute-force scan)
- [ ] Find NIC by vendor/device ID
- [ ] Read BAR registers for MMIO/IO addresses
- [ ] Add shell `pci` command to list detected devices

### 4.2 NIC Driver (assembly)

File: `drivers/virtio_net.asm` (for QEMU/KVM — simplest)
Alt: `drivers/rtl8139.asm` (for real hardware — very common, well-documented)

- [ ] Initialize NIC (reset, configure, set up ring buffers)
- [ ] `nic_send(buf, len)` — transmit an ethernet frame
- [ ] IRQ handler — receive incoming frames, queue for processing
- [ ] MAC address retrieval

### 4.3 lwIP Integration (C)

File: `lib/nic_glue.c` + vendored `lib/lwip/`

- [ ] Download and vendor lwIP source
- [ ] Write `nic_glue.c`: connect asm NIC driver to lwIP's netif callbacks
- [ ] Configure `lwipopts.h` for AiOS (no threads, polling mode, buffer sizes)
- [ ] Compile lwIP freestanding (`-m32 -ffreestanding`)
- [ ] Initialize lwIP from kernel_main
- [ ] Test: DHCP acquire, then ping

### 4.4 Shell Network Commands

- [ ] `net ifconfig` — show IP, mask, gateway
- [ ] `net ping <ip>` — ICMP echo via lwIP
- [ ] `net dhcp` — request DHCP lease

### 4.5 Milestone Deliverable

Boot AiOS, `net dhcp` gets an IP, `net ping 8.8.8.8` gets replies.

---

## Phase 5: TLS + Claude API (real hardware)

**Goal**: HTTPS to Claude API from bare metal. No proxy needed.

### 5.1 BearSSL Integration (C)

- [ ] Vendor BearSSL source into `lib/bearssl/`
- [ ] Compile freestanding (`-m32 -ffreestanding`)
- [ ] Write TLS wrapper: `tls_connect(host, port) -> tls_context`
- [ ] `tls_send(ctx, buf, len)` / `tls_recv(ctx, buf, len)`
- [ ] Embed root CA certificate (Anthropic's CA) in binary

### 5.2 HTTP Client (C)

File: `lib/http_client.c`

- [ ] `http_post(host, path, headers, body, response_buf)`
- [ ] Minimal HTTP/1.1: build request, parse status + body
- [ ] Chunked transfer encoding support (Claude API uses it)

### 5.3 Claude API Wrapper (C)

File: `lib/claude_api.c`

- [ ] `claude_ask(question, response_buf, max_len)`
- [ ] Format Messages API JSON request (using cJSON)
- [ ] Parse response JSON, extract `content[0].text`
- [ ] API key stored in kernel data section (compile-time)

### 5.4 Shell Integration

- [ ] `ask` command now works via real HTTPS (when serial not available)
- [ ] Auto-detect: try serial first, fall back to network
- [ ] `net claude status` — test API connectivity

### 5.5 LAN Proxy Mode (alternative)

- [ ] `ask` command can also use plain TCP to a LAN proxy
- [ ] Config: `set ai.proxy 192.168.1.100:8080`
- [ ] Proxy handles TLS; AiOS sends plain HTTP over TCP
- [ ] Useful as stepping stone before BearSSL is integrated

### 5.6 Milestone Deliverable

Boot AiOS on real hardware (or VM with virtio-net), no proxy running,
`ask "hello"` gets a response from Claude over HTTPS.

---

## Phase 6: User Mode & System Calls

**Goal**: Ring 3 process execution, preemptive scheduling.

- [ ] GDT: add user code/data segments (ring 3) + TSS
- [ ] INT 0x80 syscall handler + dispatch table
- [ ] Process control block (PCB) structure
- [ ] Context switch (save/restore regs + CR3)
- [ ] Round-robin scheduler driven by timer IRQ
- [ ] Ring 0 → Ring 3 transition via IRET
- [ ] `sys_exit`, `sys_write`, `sys_read`, `sys_exec`, `sys_fork`
- [ ] Load and run a simple user-mode binary

---

## Phase 7: Container Runtime

**Goal**: Docker-like isolation using hardware protection.

- [ ] Container control block (CCB)
- [ ] Create isolated page directory per container
- [ ] Capability bitmask enforcement on every syscall
- [ ] Container groups with shared virtual bridge
- [ ] Compose file parser
- [ ] Shell: `container create/start/stop/ls`, `compose up/down`

---

## Phase 8: Graphics

**Goal**: Pixel framebuffer with compositor.

- [ ] VESA/VBE framebuffer setup (via GRUB multiboot info)
- [ ] PS/2 mouse driver
- [ ] Drawing primitives (pixel, line, rect, blit)
- [ ] PSF bitmap font renderer
- [ ] Window compositor (z-order, damage rects, double buffer)
- [ ] Window manager (title bars, move, resize, focus)

---

## Phase 9: AI Runtime

**Goal**: Intent-to-execution pipeline.

- [ ] Intent resolver (natural language → structured tasks)
- [ ] Module composer (JIT linker for verified blocks)
- [ ] Privacy kernel (scoped context, output sandbox, audit log)
- [ ] Full AIP (AI Interaction Protocol) between agents

---

## Phase 10: Storage

**Goal**: AI-native persistent storage.

- [ ] ATA disk driver (PIO mode)
- [ ] Key-value store on block device
- [ ] Content-addressable storage (SHA-256 hash → blob)
- [ ] Semantic vector index for retrieval

---

## Quick Reference: What To Build Next

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| **NOW** | Phase 2: Serial driver + proxy | 1-2 days | AiOS talks to Claude |
| Next | Phase 3: Heap + C foundation | 1 day | Unlocks C libraries |
| Next | Phase 4: lwIP + NIC driver | 1 week | Real networking |
| Next | Phase 5: BearSSL + Claude API | 1 week | Standalone on real HW |
| Later | Phase 6-10 | Weeks each | Full AI-native OS |
