# Machina: Full-System Emulation Design Spec

> **Status**: Draft
> **Date**: 2026-03-29
> **Scope**: Evolve tcg-rs into machina — a full-system emulator capable of
> running rCore-Tutorial v3, full rCore, and future OS targets.

---

## 1. Executive Summary

Machina is a Rust-native full-system emulator built on tcg-rs's proven JIT
translation engine. It extends the existing linux-user emulation with
privilege levels, MMU, interrupt/exception handling, device models, and a
machine definition layer. The project supports heterogeneous multi-arch
guests (RISC-V + LoongArch) in a single emulation instance, with RISC-V
as the priority target.

**Primary goal**: Boot rCore-Tutorial v3 on a RISC-V ref machine.
**Secondary goal**: Boot full rCore, then extend to LoongArch guests.

---

## 2. Naming

| Item | Name |
|------|------|
| Project | **machina** |
| Crate prefix | `machina-*` |
| Generic board | **ref** (reference machine) |
| Monitor protocol | **MMP** (Machina Monitor Protocol) |
| Binary (unified) | `machina` with `-machine` flag |
| Binary aliases | `machina-riscv64`, `machina-loongarch64` |

---

## 3. Architecture Overview

### 3.1 Translation Pipeline (existing, renamed)

```
Guest Binary → Frontend (decode) → IR → Optimizer → Backend (codegen) → Host Code
                                         ↓
                               TranslationBlock Cache
```

### 3.2 Full-System Emulation Pipeline (new)

```
┌─────────────────────────────────────────────────────┐
│                    softmmu (binary)                  │
│  CLI parsing → Machine build → Boot → Execution     │
├──────────┬──────────┬───────────┬───────────────────┤
│ system/  │ monitor/ │ hw/riscv/ │ hw/loongarch/     │
│ cpus     │ MMP/HMP  │ ref mach  │ ref mach (future) │
│ mainloop │ gdbstub  │ boot+SBI  │                   │
├──────────┴──────────┴───────────┴───────────────────┤
│                  hw/core                             │
│  qdev · bus · irq · clock · chardev · loader · fdt  │
├──────────────────────┬──────────────────────────────┤
│      hw/intc         │         hw/char              │
│  PLIC · ACLINT       │       ns16550a               │
├──────────────────────┴──────────────────────────────┤
│                    memory/                           │
│  AddressSpace · MemoryRegion tree · FlatView · RAM  │
├─────────────────────────────────────────────────────┤
│                     accel/                           │
│  IR · optimize · liveness · regalloc · codegen      │
│  host/x86_64 · exec/ (cpu_exec, TB, TLB) · timer   │
├──────────────┬──────────────────────────────────────┤
│ guest/riscv  │           guest/loongarch             │
│ CPU+CSR+MMU  │           CPU+CSR+MMU (future)       │
│ +translate   │           +translate                  │
├──────────────┴──────────────────────────────────────┤
│              core/ (traits + types)                  │
│         GuestCpu · Machine · GPA/GVA/HVA            │
├─────────────────────────────────────────────────────┤
│                    util/                             │
└─────────────────────────────────────────────────────┘
```

---

## 4. Crate Structure

### 4.1 Directory Layout

```
machina/
│
│  ══════ Foundation ══════
│
├── core/                        # machina-core
│   ├── cpu.rs                   #   GuestCpu trait (arch-agnostic)
│   ├── machine.rs               #   Machine trait
│   ├── address.rs               #   GPA, GVA, HVA (newtype wrappers)
│   └── error.rs                 #   Common error types
│
├── util/                        # machina-util
│   ├── bitmap.rs
│   ├── log.rs
│   └── host_utils.rs
│
│  ══════ JIT Engine ══════
│
├── accel/                       # machina-accel
│   ├── ir/                      #   IR (opcode, type, temp, op, context, label)
│   ├── optimize.rs              #   Constant folding, copy prop, algebraic simp
│   ├── liveness.rs              #   Liveness analysis
│   ├── constraint.rs            #   Constraint system
│   ├── regalloc.rs              #   Register allocation
│   ├── codegen.rs               #   HostCodeGen trait
│   ├── host/                    #   Host backends
│   │   └── x86_64/              #     x86-64 codegen
│   │       ├── regs.rs
│   │       └── emitter.rs
│   ├── exec/                    #   Execution engine
│   │   ├── cpu_exec.rs          #     Execution loop + interrupt checkpoints
│   │   ├── translate_all.rs     #     Translation dispatch + translate_lock
│   │   ├── tb.rs                #     TB store + hash + jump cache
│   │   ├── tb_maint.rs          #     TB linking / invalidation / flush
│   │   └── cputlb.rs            #     Software TLB fast path
│   └── timer.rs                 #   Virtual clock + timers + icount
│
│  ══════ Memory Subsystem ══════
│
├── memory/                      # machina-memory
│   ├── address_space.rs         #   AddressSpace (top-level view)
│   ├── memory_region.rs         #   MemoryRegion tree (container/alias/sub/prio)
│   ├── flat_view.rs             #   FlatView (flattened snapshot for fast lookup)
│   ├── ram.rs                   #   RAM backend (mmap)
│   └── rom.rs                   #   ROM backend
│
│  ══════ Guest Architectures ══════
│
├── guest/
│   ├── riscv/                   # machina-guest-riscv
│   │   ├── cpu.rs               #   CPU state + register file
│   │   ├── csr.rs               #   Full CSR set (M/S/U) + vendor dispatch
│   │   ├── mmu.rs               #   Sv39/Sv48 page table walk + TLB
│   │   ├── pmp.rs               #   Physical Memory Protection
│   │   ├── exception.rs         #   Exception/interrupt + privilege transitions
│   │   ├── time_helper.rs       #   Timer helpers
│   │   ├── translate/           #   Translator (insn → IR)
│   │   │   └── insn_trans/      #     Per-extension (RVI/RVM/RVA/RVF/RVD/RVC)
│   │   └── helper.rs            #   Runtime helper functions
│   │
│   └── loongarch/               # machina-guest-loongarch (future, same structure)
│       ├── cpu.rs               #   LA64 CPU state
│       ├── csr.rs               #   LoongArch CSR set
│       ├── mmu.rs               #   LoongArch page table + TLB
│       ├── exception.rs         #   Exception/interrupt model
│       ├── translate/           #   Translator
│       └── helper.rs            #   Helpers
│
│  ══════ Hardware Devices ══════
│
├── hw/
│   ├── core/                    # machina-hw-core
│   │   ├── bus.rs               #   BusDevice trait, SysBus, MMIO/PIO dispatch
│   │   ├── qdev.rs              #   Device lifecycle (QEMU Rust QOM pattern)
│   │   │                        #     ObjectType, IsA<P>, ObjectImpl traits
│   │   │                        #     realize / reset / unrealize
│   │   │                        #     #[property] attributes
│   │   ├── irq.rs               #   IRQ routing (IrqLine, OrIrq, SplitIrq)
│   │   ├── clock.rs             #   Device clock (freq/period propagation)
│   │   ├── chardev/             #   Chardev frontend-backend (aligned w/ QEMU)
│   │   │   ├── mod.rs           #     Chardev trait + CharFrontend
│   │   │   ├── null.rs          #     Null backend
│   │   │   ├── stdio.rs         #     Stdio backend
│   │   │   ├── socket.rs        #     TCP/Unix socket backend
│   │   │   ├── file.rs          #     File backend
│   │   │   ├── pty.rs           #     PTY backend
│   │   │   ├── pipe.rs          #     Pipe backend
│   │   │   ├── ringbuf.rs       #     Ring buffer backend
│   │   │   └── mux.rs           #     Multiplexer (1:N)
│   │   ├── loader.rs            #   Firmware/kernel loader (ELF, binary, FIT)
│   │   └── fdt.rs               #   FDT generator
│   │
│   ├── intc/                    # machina-hw-intc
│   │   ├── sifive_plic.rs       #   PLIC (ref: QEMU hw/intc/sifive_plic.c)
│   │   └── riscv_aclint.rs      #   ACLINT: MTIMER + MSWI + SSWI
│   │
│   ├── char/                    # machina-hw-char
│   │   └── ns16550a.rs          #   UART 16550A (ref: QEMU hw/char/serial.c)
│   │
│   ├── riscv/                   # machina-hw-riscv
│   │   ├── r#ref.rs             #   ref machine (QEMU virt-compatible layout)
│   │   ├── boot.rs              #   Boot flow (load SBI → kernel → DTB)
│   │   └── sbi.rs               #   SBI dispatch (RustSBI default, OpenSBI opt)
│   │
│   └── loongarch/               # machina-hw-loongarch (future)
│       └── r#ref.rs             #   ref machine
│
│  ══════ Monitor & Debug ══════
│
├── monitor/                     # machina-monitor
│   ├── mmp/                     #   MMP (Machina Monitor Protocol)
│   │   ├── protocol.rs          #     JSON-RPC message format
│   │   ├── commands.rs          #     MMP command handlers
│   │   └── server.rs            #     MMP server (chardev-backed)
│   ├── hmp/                     #   HMP (Human Monitor Protocol)
│   │   ├── parser.rs            #     Command-line parser
│   │   └── commands.rs          #     HMP commands (each calls MMP internally)
│   └── gdbstub/                 #   GDB remote stub
│       ├── protocol.rs          #     GDB RSP (Remote Serial Protocol)
│       ├── commands.rs          #     g/G/m/M/s/c/z/Z handlers
│       └── breakpoint.rs        #     SW/HW breakpoint management
│
│  ══════ System Layer ══════
│
├── system/                      # machina-system
│   ├── cpus.rs                  #   Multi-core CPU management + scheduling
│   ├── main_loop.rs             #   Main event loop (phase 1: poll in cpu_exec,
│   │                            #     phase 2: custom epoll-based loop)
│   └── device_tree.rs           #   System-level FDT integration
│
│  ══════ Emulation Entries (binaries) ══════
│
├── softmmu/                     # machina-softmmu (binary)
│   └── main.rs                  #   CLI → machine build → boot → run
│
├── linux-user/                  # machina-linux-user (binary)
│   ├── main.rs
│   ├── elfload.rs
│   ├── syscall.rs
│   └── mmap.rs
│
│  ══════ Testing ══════
│
├── tests/
│   ├── qtest/                   # machina-qtest (library)
│   │   ├── lib.rs
│   │   ├── protocol.rs          #   Text protocol (QEMU qtest-compatible)
│   │   ├── client.rs            #   Spawn machina subprocess + communicate
│   │   └── machine.rs           #   Test machine builders
│   │
│   └── cases/                   # machina-tests
│       ├── unit/                #   Component-level (CSR, MMU, PLIC regs)
│       ├── integration/         #   Subsystem-level (interrupt chain, TLB refill)
│       ├── difftest/            #   Compare against QEMU + Spike
│       └── system/              #   End-to-end (boot rCore, guest programs)
│
│  ══════ Tools ══════
│
├── decode/                      # machina-decode
├── disas/                       # machina-disas
├── tools/
│   ├── irdump/
│   └── irbackend/
│
│  ══════ Submodules ══════
│
└── third-party/
    └── rustsbi/                 # RustSBI (git submodule, built from source)
```

### 4.2 Crate Dependency Graph

```
core ← util
  ↑
  ├── accel (core, util)
  ├── memory (core, util)
  ├── guest/* (core, accel, memory)
  ├── hw/core (core, memory, util)
  │     ↑
  │     ├── hw/intc (hw/core)
  │     ├── hw/char (hw/core)
  │     └── hw/riscv (hw/core, hw/intc, hw/char, guest/riscv)
  ├── monitor (core, memory, hw/core, accel)
  ├── system (core, memory, hw/core, accel, monitor)
  │
  ├── softmmu (system, hw/*, accel, guest/*, monitor)  ← binary
  └── linux-user (core, accel, guest/*)                 ← binary
```

No circular dependencies. `linux-user` does not depend on `hw/` or
`monitor/`.

### 4.3 QEMU Mapping

| machina | QEMU | Role |
|---------|------|------|
| `core/` | `include/` common headers | Pure traits + types |
| `accel/` | `tcg/` + `accel/tcg/` | JIT full-stack |
| `memory/` | `system/memory.c` | MemoryRegion tree |
| `guest/` | `target/` | Guest arch (CPU + translator) |
| `hw/` | `hw/` | Devices + machine defs |
| `hw/core/qdev` | `hw/core/qdev.c` + `rust/qom/` | Device object model |
| `hw/core/chardev/` | `chardev/` | Char I/O backends |
| `monitor/mmp/` | `qapi/` + `monitor/` | Machine monitor protocol |
| `monitor/hmp/` | `monitor/hmp*.c` | Human monitor (wraps MMP) |
| `monitor/gdbstub/` | `gdbstub/` | GDB remote stub |
| `system/` | `system/` | CPU mgmt + main loop |
| `softmmu/` | `system/main.c` | Full-system binary |
| `tests/qtest/` | `tests/qtest/` | Test framework |

---

## 5. Core Traits

### 5.1 GuestCpu

```rust
pub trait GuestCpu: Send + Sync {
    // Existing (from tcg-rs)
    fn get_pc(&self) -> u64;
    fn get_flags(&self) -> u32;
    fn gen_code(&self, ctx: &mut Context) -> TranslateResult;
    fn env_ptr(&self) -> *mut u8;

    // New: full-system emulation
    fn pending_interrupt(&self) -> Option<u32>;
    fn is_halted(&self) -> bool;
    fn set_halted(&mut self, halted: bool);
    fn privilege_level(&self) -> u8;
    fn handle_interrupt(&mut self) -> bool;
    fn handle_exception(&mut self, excp: u32);
    fn tlb_flush(&mut self);
    fn tlb_flush_page(&mut self, addr: u64);

    // GDB support
    fn gdb_read_registers(&self, buf: &mut [u8]) -> usize;
    fn gdb_write_registers(&mut self, buf: &[u8]) -> usize;
    fn gdb_read_register(&self, reg: usize) -> u64;
    fn gdb_write_register(&mut self, reg: usize, val: u64);
}
```

### 5.2 Machine

```rust
pub trait Machine: Send + Sync {
    fn name(&self) -> &str;
    fn init(&mut self, opts: &MachineOpts) -> Result<()>;
    fn reset(&mut self);
    fn pause(&mut self);
    fn resume(&mut self);
    fn shutdown(&mut self);
    fn boot(&mut self) -> Result<()>;

    fn address_space(&self) -> &AddressSpace;
    fn cpu_count(&self) -> usize;
    fn ram_size(&self) -> u64;
}

pub struct MachineOpts {
    pub ram_size: u64,
    pub cpu_count: u32,
    pub kernel: Option<PathBuf>,
    pub bios: Option<PathBuf>,
    pub append: Option<String>,
    // ...
}
```

### 5.3 Address Types (Newtype)

```rust
/// Guest Physical Address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GPA(pub u64);

/// Guest Virtual Address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GVA(pub u64);

/// Host Virtual Address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HVA(pub u64);
```

---

## 6. Memory Subsystem

### 6.1 MemoryRegion Tree

Reference: QEMU `system/memory.c`, rust-vmm `vm-memory`.

```rust
pub enum RegionType {
    Ram,                    // Backed by host memory (mmap)
    Rom,                    // Read-only host memory
    Io { ops: MmioOps },   // MMIO with read/write callbacks
    Alias { target, offset },  // Alias into another region
    Container,              // Container-only, no backing
}

pub struct MemoryRegion {
    name: String,
    size: u64,
    region_type: RegionType,
    priority: i32,          // Higher priority wins on overlap
    subregions: Vec<MemoryRegion>,
    enabled: bool,
}
```

### 6.2 AddressSpace + FlatView

```rust
pub struct AddressSpace {
    root: MemoryRegion,
    flat_view: RwLock<FlatView>,  // Cached flattened mapping
}

pub struct FlatView {
    ranges: Vec<FlatRange>,  // Sorted, non-overlapping
}

pub struct FlatRange {
    addr: GPA,
    size: u64,
    region: *const MemoryRegion,
    offset_in_region: u64,
}
```

FlatView is rebuilt when the MemoryRegion tree changes (region
add/remove/enable/disable). Memory accesses go through FlatView for
O(log n) lookup.

### 6.3 Self-Modifying Code Detection

Per-page dirty tracking with TB invalidation on write, aligned with
QEMU's `translate-all.c` implementation. Each RAM page has a bitmap
tracking whether TBs exist for that page. Writes to code pages trigger
TB flush for the affected page.

---

## 7. JIT Engine (accel/)

### 7.1 IR

Carried over from tcg-rs `core/`. 158 unified polymorphic opcodes with
`OpFlags` for backend dispatch. Existing IR is sufficient for
full-system emulation; no new opcodes needed.

### 7.2 Translation Block Policy

- **No cross-page TBs**: A TB must not span a page boundary (aligned
  with QEMU). Guest page size determines max TB length.
- **TB invalidation**: Self-modifying code detected via page dirty bits
  triggers TB flush for affected pages.
- **TB chaining**: `goto_tb` slot patching for direct TB→TB jumps.
  `exit_target` atomic cache for indirect jumps.

### 7.3 Software TLB

Aligned with QEMU `accel/tcg/cputlb.c`:
- 256-entry, N-way set associative TLB
- Per-CPU, flushed on `sfence.vma` / ASID change
- Fast path inlined in generated code
- Slow path calls MMU page table walker

### 7.4 Clock Model (Dual-Layer)

**Layer 1 — Virtual Clock** (`accel/timer.rs`):

Reference: QEMU `util/qemu-timer.c`.

```rust
pub enum ClockType {
    Realtime,   // Wall clock, keeps running when paused
    Virtual,    // Stops when VM paused, used for guest timers
    Host,       // Monotonic host clock
}

pub struct VirtualClock {
    clock_type: ClockType,
    ns: AtomicI64,
    enabled: AtomicBool,
}

pub struct Timer {
    expire_time: i64,
    clock: ClockType,
    callback: Box<dyn FnMut()>,
}
```

**Layer 2 — Device Clock** (`hw/core/clock.rs`):

Reference: QEMU `hw/core/clock.c`.

Frequency/period propagation between devices (e.g., UART baud rate
derived from system clock). Separate from virtual timers.

### 7.5 Execution Models

**Multi-threaded (MTTCG)**:
- One host thread per vCPU
- Shared TB cache (lock-free reads, mutex on write)
- Interrupt delivery via atomic flags

**Single-threaded (RR)**:
- All vCPUs execute in one thread, round-robin
- Deterministic execution order
- Useful for debugging

**icount mode**:
- Clock advances by instruction count, not wall time
- In MTTCG: periodic synchronization — all vCPUs align icount at
  configurable intervals
- In RR: each vCPU advances by configurable icount steps
- Enables deterministic replay

---

## 8. Guest Architecture: RISC-V

### 8.1 CPU State

```rust
pub struct RiscvCpu {
    // GPR + FPR
    pub gpr: [u64; 32],
    pub fpr: [u64; 32],
    pub pc: u64,

    // Privilege level
    pub priv_level: PrivLevel,  // M, S, U

    // CSR banks (M/S/U full set)
    pub csr: CsrFile,

    // MMU
    pub mmu: Mmu,

    // PMP
    pub pmp: Pmp,

    // Interrupt state
    pub interrupt_request: AtomicU32,
    pub halted: AtomicBool,

    // FP status tracked via mstatus.FS
    // Lazy save/restore: check FS on every FP insn
}

pub enum PrivLevel { User = 0, Supervisor = 1, Machine = 3 }
```

### 8.2 CSR Implementation

Reference: QEMU `target/riscv/csr.c` + RISC-V Privileged Spec.

Strategy: Grouped match (M-mode / S-mode / U-mode CSR groups) with
function pointer table for vendor extensions.

```rust
pub struct CsrFile {
    // M-mode
    pub mstatus: u64, pub misa: u64, pub medeleg: u64,
    pub mideleg: u64, pub mie: u64, pub mtvec: u64,
    pub mscratch: u64, pub mepc: u64, pub mcause: u64,
    pub mtval: u64, pub mip: u64,
    // S-mode
    pub sstatus: u64, pub sie: u64, pub stvec: u64,
    pub sscratch: u64, pub sepc: u64, pub scause: u64,
    pub stval: u64, pub sip: u64, pub satp: u64,
    // U-mode (existing)
    pub fflags: u64, pub frm: u64,
    // Counters, PMP cfg, vendor CSRs...

    // Vendor extension dispatch table
    pub vendor_csr: Option<Box<dyn VendorCsr>>,
}

pub trait VendorCsr: Send + Sync {
    fn read(&self, addr: u16) -> Option<u64>;
    fn write(&mut self, addr: u16, val: u64) -> Option<()>;
}
```

### 8.3 MMU (Sv39/Sv48)

Reference: QEMU `target/riscv/cpu_helper.c`, RISC-V Privileged Spec
Ch. 4.

- Sv39: 3-level page table, 39-bit virtual address
- Sv48: 4-level (future)
- TLB: 256-entry, flushed on `sfence.vma`
- Access checks: R/W/X permissions, U-bit, MXR, SUM
- Page fault → exception delivery

### 8.4 Exception / Interrupt Model

Reference: QEMU `target/riscv/cpu_helper.c`.

```
Exception flow:
  1. Save PC → xepc, cause → xcause, trap val → xtval
  2. Update xstatus (save prev privilege in xPP, disable interrupts)
  3. Set PC → xtvec (direct or vectored mode)
  4. Switch to target privilege level

Interrupt priority:
  MEI > MSI > MTI > SEI > SSI > STI

Delegation:
  medeleg/mideleg: delegate specific exceptions/interrupts to S-mode
```

### 8.5 Floating Point

Check `mstatus.FS` on every floating-point instruction:
- `FS = Off (0)`: Generate illegal instruction exception
- `FS ≠ Off`: Execute FP operation, mark `FS = Dirty (3)`

### 8.6 Extensions (Minimal for rCore-Tutorial v3)

- RV64I (base integer)
- M (multiply/divide)
- A (atomics: LR/SC/AMO)
- F (single-precision FP)
- D (double-precision FP)
- C (compressed)
- Zicsr (CSR instructions)
- Zifencei (instruction fence)

Additional for full rCore: Zicntr (counters), Sstc (stimecmp) as
needed.

---

## 9. Hardware Devices

### 9.1 Device Object Model

Reference: QEMU `rust/qom/src/qom.rs` (Rust QOM bindings).

Reference QEMU's Rust QOM design (`rust/qom/src/qom.rs`) as the
primary inspiration, but adapt to machina's pure-Rust context. No C
FFI interop needed, so simplify where possible: drop `ParentInit`
lifetime tricks (no C init ordering), replace `Owned<T>` with
standard `Arc<Mutex<T>>`, and use Rust enums for type dispatch
instead of string-based `TYPE_NAME` registration. Retain the core
patterns (`ObjectType` / `IsA<P>` / `DeviceImpl` / `BusDevice`)
that provide compile-time type safety:

```rust
// Type identity
pub unsafe trait ObjectType: Sized {
    type Class;
    const TYPE_NAME: &'static str;
}

// Compile-time type hierarchy check
pub unsafe trait IsA<P: ObjectType>: ObjectType {}

// Device lifecycle
pub trait ObjectImpl: ObjectType + IsA<Object> {
    type ParentType: ObjectType;
    fn class_init(klass: &mut Self::Class);
    fn instance_init(&mut self) {}
    fn instance_post_init(&self) {}
}

// Realize / reset
pub trait DeviceImpl: ObjectImpl {
    fn realize(&mut self) -> Result<()>;
    fn reset(&mut self);
    fn properties() -> &'static [Property];
}

// MMIO access
pub trait BusDevice: DeviceImpl {
    fn read(&self, offset: u64, size: u32) -> u64;
    fn write(&mut self, offset: u64, size: u32, value: u64);
}
```

### 9.2 Chardev Architecture

Reference: QEMU `chardev/`, `include/chardev/char.h`.

Frontend-backend pattern aligned with QEMU:

```rust
pub trait Chardev: Send + Sync {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    fn can_read(&self) -> usize;
    fn set_handlers(&mut self, handlers: CharHandlers);
}

pub struct CharFrontend {
    backend: Option<Box<dyn Chardev>>,
    on_can_read: Option<Box<dyn Fn() -> usize>>,
    on_read: Option<Box<dyn FnMut(&[u8])>>,
    on_event: Option<Box<dyn FnMut(CharEvent)>>,
}

pub enum CharEvent { Opened, Closed, Break }
```

**Backends**: null, stdio, socket (TCP/Unix/telnet), file, pty, pipe,
ringbuf, mux. All aligned with QEMU chardev.

**CLI**:
```bash
machina -chardev stdio,id=char0 -serial chardev:char0
machina -serial stdio  # shorthand
```

### 9.3 PLIC (Platform-Level Interrupt Controller)

Reference: QEMU `hw/intc/sifive_plic.c`, SiFive PLIC spec.

- Configurable interrupt sources and priority levels
- Per-hart context (M-mode + S-mode)
- MMIO registers: priority, pending, enable, threshold, claim/complete

### 9.4 ACLINT (Advanced Core-Local Interruptor)

Reference: QEMU `hw/intc/riscv_aclint.c`, RISC-V ACLINT spec.

- MTIMER: 64-bit mtime + per-hart mtimecmp
- MSWI: Machine software interrupt (per-hart)
- SSWI: Supervisor software interrupt (per-hart)

### 9.5 UART (ns16550a)

Reference: QEMU `hw/char/serial.c`, 16550A datasheet.

- TX/RX with FIFO (16-byte)
- Interrupt generation (RX data available, TX holding empty, etc.)
- Connects to chardev backend via CharFrontend
- Line control, modem control, baud rate divisor

---

## 10. RISC-V ref Machine

### 10.1 Memory Map (QEMU virt-compatible)

```
0x0000_0000 - 0x00FF_FFFF : Debug/test area
0x0010_0000 - 0x0010_0FFF : VIRT_TEST (reboot/poweroff)
0x0010_1000 - 0x0010_1FFF : RTC
0x0200_0000 - 0x0200_FFFF : ACLINT (CLINT-compatible)
0x0C00_0000 - 0x0FFF_FFFF : PLIC
0x1000_0000 - 0x1000_00FF : UART0
0x1000_1000 - 0x1000_7FFF : VirtIO MMIO (future)
0x8000_0000 - ...         : RAM (configurable, default 128 MiB)
```

### 10.2 Boot Flow

```
1. Load RustSBI firmware to 0x8000_0000
   (or OpenSBI if -bios opensbi specified)
2. Load kernel to 0x8020_0000 (after SBI)
3. Generate FDT, place at top of RAM
4. Set a0 = hart ID, a1 = FDT address
5. Start execution at SBI entry point (M-mode)
6. SBI initializes → jumps to kernel (S-mode)
7. Kernel boots using SBI services
```

### 10.3 SBI

**Default**: RustSBI (git submodule at `third-party/rustsbi/`, built
from source as a RISC-V binary).

**Optional**: OpenSBI (user provides binary via `-bios`).

Required SBI extensions for rCore:
- Base (mandatory)
- Timer (set_timer)
- IPI (send_ipi)
- RFENCE (remote_sfence_vma)
- HSM (hart_start / hart_stop, for multi-core boot)
- SRST (system_reset / system_shutdown)

### 10.4 DTB (Device Tree Blob)

FDT generated at machine init time, describing:
- CPU topology (hart count, ISA string)
- Memory regions
- Device addresses and IRQ routing
- Chosen node (bootargs, stdout-path)

Passed to firmware via `a1` register. Reference: QEMU `hw/riscv/virt.c`
machine init, RustSBI FDT handling.

---

## 11. Monitor & Debug

### 11.1 MMP (Machina Monitor Protocol)

Reference: QEMU QMP (`qapi/`).

JSON-RPC over chardev (Unix socket or TCP). Machine-readable interface
for external tools.

```json
// Request
{"execute": "query-status"}

// Response
{"return": {"status": "running", "singlestep": false}}

// Event
{"event": "STOP", "timestamp": {"seconds": 1234, "microseconds": 0}}
```

**Core commands**: `query-status`, `stop`, `cont`, `system_reset`,
`system_powerdown`, `quit`, `query-cpus`, `memsave`, `human-monitor-command`.

### 11.2 HMP (Human Monitor Protocol)

Text command-line interface. Every HMP command is implemented by calling
the corresponding MMP command internally.

```
(machina) info status
VM status: running
(machina) info registers
 pc  0x0000000080200000
 ra  0x0000000080200128
 ...
(machina) stop
(machina) cont
(machina) x/16xw 0x80000000
```

### 11.3 GDB Stub

Reference: QEMU `gdbstub/`, GDB Remote Serial Protocol.

- TCP listener (default port 1234)
- Supports: `g`/`G` (read/write regs), `m`/`M` (read/write mem),
  `s` (step), `c` (continue), `z`/`Z` (breakpoints)
- Software breakpoints via TB invalidation
- Per-CPU register access
- CLI: `machina -s` (shorthand for `-gdb tcp::1234`)

---

## 12. Testing Framework

### 12.1 Test Layers

| Layer | Scope | Method | Location |
|-------|-------|--------|----------|
| Unit | Single component | In-process Rust API | `tests/cases/unit/` |
| Integration | Subsystem | In-process Rust API | `tests/cases/integration/` |
| Difftest | Correctness | Compare vs QEMU + Spike | `tests/cases/difftest/` |
| System | End-to-end | qtest (out-of-process) | `tests/cases/system/` |

### 12.2 qtest Framework

Reference: QEMU `tests/qtest/libqtest.h`.

**Protocol** (text, over Unix socket — QEMU qtest-compatible):

```
readb ADDR          → OK VALUE
readw ADDR          → OK VALUE
readl ADDR          → OK VALUE
readq ADDR          → OK VALUE
writeb ADDR VALUE   → OK
writew ADDR VALUE   → OK
writel ADDR VALUE   → OK
writeq ADDR VALUE   → OK
read ADDR SIZE      → OK DATA (hex)
write ADDR SIZE DATA → OK
clock_step NS       → OK VALUE
clock_set NS        → OK VALUE
irq_intercept_out QOM_PATH → OK
set_irq_in QOM_PATH NAME NUM LEVEL → OK
```

**Async messages**: `IRQ raise NUM`, `IRQ lower NUM`.

**Rust client API** (matches QEMU programming experience):

```rust
use machina_qtest::prelude::*;

#[test]
fn test_uart_tx() {
    let m = MachinaTest::start("-machine ref -m 128M");
    // Write to UART THR
    m.writel(0x1000_0000, 0x41);
    // Check LSR TX empty
    assert_ne!(m.readl(0x1000_0005) & 0x20, 0);
    // Advance clock, check IRQ
    m.clock_step(1_000_000);
    assert!(m.get_irq(UART0_IRQ));
}
```

### 12.3 Difftest

Compare machina execution against reference implementations:
- **QEMU**: Run same binary on both, compare register state after each
  instruction (or per-TB)
- **Spike**: RISC-V ISA simulator, instruction-accurate reference

### 12.4 Acceptance Criteria

**Phase 1**: Boot rCore-Tutorial v3 (Ch1-8) to user shell.
**Phase 2**: Boot rCore-Tutorial v3 (Ch9) with virtio device drivers.
**Phase 3**: Boot full rCore.
**Debug support**: gdbstub, MMP/HMP monitor, register/memory dump
available from Phase 1.

---

## 13. CLI Interface

QEMU-style command line:

```bash
# Full-system emulation
machina \
    -machine ref \
    -m 128M \
    -smp 1 \
    -bios third-party/rustsbi/target/riscv64/release/rustsbi.bin \
    -kernel path/to/rcore.bin \
    -nographic \
    -serial stdio

# With GDB
machina -machine ref -m 128M -kernel rcore.bin -s -S

# With monitor
machina -machine ref -m 128M -kernel rcore.bin \
    -monitor stdio \
    -serial tcp::4555,server=on

# With MMP
machina -machine ref -m 128M -kernel rcore.bin \
    -mmp tcp::5555,server=on

# Aliases
machina-riscv64 -machine ref -m 128M -kernel rcore.bin -nographic
machina-loongarch64 -machine ref -m 128M -kernel kernel.bin -nographic
```

---

## 14. Heterogeneous Multi-Arch Support

The system supports mixing different guest architectures in a single
emulation instance (e.g., RISC-V main cores + LoongArch co-processor).

**Design**:
- `GuestCpu` trait is arch-agnostic; each arch implements it
- `Machine` trait can compose multiple CPU types
- Shared `AddressSpace` with per-CPU TLB
- Inter-processor communication via shared memory regions or IPI

**Phase 1**: Single-arch per instance (RISC-V only).
**Phase 2**: LoongArch as separate instance.
**Phase 3**: True heterogeneous (mixed archs in one instance).

---

## 15. Migration from tcg-rs

### 15.1 Crate Rename

All crates renamed from `tcg-*` to `machina-*`:

| Old | New |
|-----|-----|
| `tcg-core` | `machina-core` (IR moved to accel) |
| `tcg-backend` | merged into `machina-accel` |
| `tcg-frontend` | `machina-guest-riscv` |
| `tcg-exec` | merged into `machina-accel` (exec/) |
| `tcg-linux-user` | `machina-linux-user` |
| `tcg-tests` | `machina-tests` |
| `decode` | `machina-decode` |
| `disas` | `machina-disas` |

### 15.2 No Backward Compatibility

Direct replacement. No migration shims, no re-exports. The `tcg-*`
names are retired.

---

## 16. Reference Sources

| Component | Primary Reference |
|-----------|-------------------|
| Privilege / CSR / MMU | RISC-V Privileged Spec v1.12 |
| PLIC | SiFive PLIC Spec + QEMU `hw/intc/sifive_plic.c` |
| ACLINT | RISC-V ACLINT Spec + QEMU `hw/intc/riscv_aclint.c` |
| UART | 16550A Datasheet + QEMU `hw/char/serial.c` |
| MemoryRegion | QEMU `system/memory.c` + rust-vmm `vm-memory` |
| Device model | QEMU `rust/qom/` + `rust/hw/` |
| Chardev | QEMU `chardev/` |
| Boot flow | QEMU `hw/riscv/virt.c` + `hw/riscv/boot.c` |
| SBI | RustSBI source + SBI Spec v2.0 |
| DTB/FDT | QEMU `hw/riscv/virt.c` + devicetree spec |
| Clock model | QEMU `util/qemu-timer.c` + `hw/core/clock.c` |
| TLB | QEMU `accel/tcg/cputlb.c` |
| TB management | QEMU `accel/tcg/tb-maint.c` |
| QMP → MMP | QEMU `qapi/` + `monitor/` |
| HMP | QEMU `monitor/hmp*.c` |
| GDB stub | QEMU `gdbstub/` |
| qtest | QEMU `tests/qtest/libqtest.h` |
| LoongArch | LoongArch Reference Manual v1.0 + QEMU `target/loongarch/` |
