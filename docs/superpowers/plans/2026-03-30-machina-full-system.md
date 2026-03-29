# Machina Full-System Emulation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evolve tcg-rs into machina — a full-system emulator that boots
rCore-Tutorial v3 on a RISC-V riscv64-ref machine.

**Architecture:** Restructure tcg-rs into machina-* crates, merge JIT
components into a unified `accel/` crate, add MemoryRegion tree,
privilege levels, MMU, device models (PLIC/ACLINT/UART), chardev
backends, RustSBI integration, and a QEMU virt-compatible ref machine.
The plan is split into 7 phases executed sequentially.

**Tech Stack:** Rust (2021 edition), x86-64 host, RISC-V guest (RV64GC),
RustSBI (git submodule), no external device crates.

**Spec:** `docs/superpowers/specs/2026-03-29-machina-full-system-design.md`

**Acceptance Criteria (Phase 1 final):**
- `machina -M riscv64-ref -m 128M -bios rustsbi.bin -kernel rcore.bin
  -nographic` boots rCore-Tutorial v3 (Ch1-8) to user shell
- gdbstub allows `target remote :1234` from GDB
- mtest can writel/readl device registers and verify IRQ behavior

---

## Phase 1: Foundation (Crate Restructure + Core Traits + Memory)

### Task 1.1: Workspace Rename and Skeleton

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Rename: `core/` stays, update `Cargo.toml` → `machina-core`
- Rename: `decode/Cargo.toml` → `machina-decode`
- Rename: `disas/Cargo.toml` → `machina-disas`
- Create: `src/main.rs` (placeholder binary)
- Create: `util/Cargo.toml`, `util/src/lib.rs`
- Create: `memory/Cargo.toml`, `memory/src/lib.rs`
- Create: `monitor/Cargo.toml`, `monitor/src/lib.rs`
- Create: `system/Cargo.toml`, `system/src/lib.rs`
- Create: `hw/core/Cargo.toml`, `hw/core/src/lib.rs`
- Create: `hw/intc/Cargo.toml`, `hw/intc/src/lib.rs`
- Create: `hw/char/Cargo.toml`, `hw/char/src/lib.rs`
- Create: `hw/riscv/Cargo.toml`, `hw/riscv/src/lib.rs`
- Create: `tests/mtest/Cargo.toml`, `tests/mtest/src/lib.rs`
- Delete: `linux-user/` (entire crate)

- [ ] **Step 1:** Update workspace root `Cargo.toml`:
  - Change `[workspace] members` to list all new crate paths
  - Add `[package]` section: `name = "machina"`, binary entry
  - Add `[[bin]] name = "machina", path = "src/main.rs"`
  - Remove `linux-user` from members

- [ ] **Step 2:** Rename each existing crate's `Cargo.toml` `[package]
  name` field:
  - `core/` → `machina-core`
  - `decode/` → `machina-decode`
  - `disas/` → `machina-disas`

- [ ] **Step 3:** Create skeleton crates with empty `lib.rs` for:
  `util`, `memory`, `monitor`, `system`, `hw/core`, `hw/intc`,
  `hw/char`, `hw/riscv`, `tests/mtest`. Each `Cargo.toml` uses
  `machina-` prefix (e.g., `machina-memory`).

- [ ] **Step 4:** Create `src/main.rs` with minimal placeholder:

```rust
fn main() {
    eprintln!("machina: not yet implemented");
    std::process::exit(1);
}
```

- [ ] **Step 5:** Delete `linux-user/` directory entirely.

- [ ] **Step 6:** Run `cargo build` — all crates compile (empty libs).
  Run `cargo test` — existing tests adapted or temporarily disabled
  where crate names changed.

- [ ] **Step 7:** Update all internal `Cargo.toml` dependency references
  from `tcg-*` to `machina-*`. Fix all `use tcg_*` imports across the
  codebase to `use machina_*`. Run `cargo build && cargo test`.

- [ ] **Step 8:** Commit: `project: rename workspace from tcg-rs to machina`

---

### Task 1.2: Core Traits and Address Types

**Files:**
- Modify: `core/src/lib.rs`
- Create: `core/src/machine.rs`
- Create: `core/src/address.rs`
- Modify: `core/src/cpu.rs` (extend GuestCpu trait)
- Test: `core/src/address.rs` (unit tests inline)

- [ ] **Step 1:** Create `core/src/address.rs` with newtype wrappers:

```rust
/// Guest Physical Address
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GPA(pub u64);

/// Guest Virtual Address
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GVA(pub u64);

/// Host Virtual Address
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct HVA(pub u64);

impl GPA {
    pub const fn new(addr: u64) -> Self { Self(addr) }
    pub const fn offset(self, off: u64) -> Self {
        Self(self.0.wrapping_add(off))
    }
}
// Same for GVA, HVA. Add Display, From<u64>, Into<u64>.
```

- [ ] **Step 2:** Write inline tests for address arithmetic, ordering,
  display formatting. Run `cargo test -p machina-core`.

- [ ] **Step 3:** Create `core/src/machine.rs`:

```rust
use crate::address::GPA;
use std::path::PathBuf;

pub struct MachineOpts {
    pub ram_size: u64,
    pub cpu_count: u32,
    pub kernel: Option<PathBuf>,
    pub bios: Option<PathBuf>,
    pub append: Option<String>,
}

pub trait Machine: Send + Sync {
    fn name(&self) -> &str;
    fn init(&mut self, opts: &MachineOpts) -> Result<(), Box<dyn std::error::Error>>;
    fn reset(&mut self);
    fn pause(&mut self);
    fn resume(&mut self);
    fn shutdown(&mut self);
    fn boot(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn cpu_count(&self) -> usize;
    fn ram_size(&self) -> u64;
}
```

- [ ] **Step 4:** Extend `GuestCpu` trait in `core/src/cpu.rs` with
  full-system methods: `pending_interrupt`, `is_halted`, `set_halted`,
  `privilege_level`, `handle_interrupt`, `handle_exception`,
  `tlb_flush`, `tlb_flush_page`, and GDB methods
  `gdb_read_registers`, `gdb_write_registers`, `gdb_read_register`,
  `gdb_write_register`. Add default no-op implementations where
  sensible so existing code does not break.

- [ ] **Step 5:** Export new modules from `core/src/lib.rs`.
  Run `cargo build`. Commit: `core: add Machine trait and address newtypes`

---

### Task 1.3: Memory Subsystem — MemoryRegion Tree

**Files:**
- Create: `memory/src/region.rs`
- Create: `memory/src/address_space.rs`
- Create: `memory/src/flat_view.rs`
- Create: `memory/src/ram.rs`
- Modify: `memory/src/lib.rs`
- Test: `memory/tests/region_tests.rs`

- [ ] **Step 1:** Write failing test in `memory/tests/region_tests.rs`:

```rust
use machina_memory::*;

#[test]
fn test_flat_view_single_ram() {
    let mut root = MemoryRegion::container("root");
    let ram = MemoryRegion::ram("ram", 0x800_0000); // 128 MiB
    root.add_subregion(ram, GPA::new(0x8000_0000));
    let fv = FlatView::from_region(&root);
    assert_eq!(fv.lookup(GPA::new(0x8000_0000)).is_some(), true);
    assert_eq!(fv.lookup(GPA::new(0x7FFF_FFFF)).is_none(), true);
}

#[test]
fn test_flat_view_overlap_priority() {
    let mut root = MemoryRegion::container("root");
    let ram = MemoryRegion::ram("ram", 0x1000_0000);
    let io = MemoryRegion::io("uart", 0x100, /* ops */ todo!());
    root.add_subregion(ram, GPA::new(0x0));
    root.add_subregion_with_priority(io, GPA::new(0x1000_0000), 1);
    let fv = FlatView::from_region(&root);
    // IO region has higher priority, should win at 0x1000_0000
    let hit = fv.lookup(GPA::new(0x1000_0000)).unwrap();
    assert!(hit.is_io());
}
```

- [ ] **Step 2:** Implement `memory/src/region.rs`:
  - `MemoryRegion` struct with `name`, `size`, `region_type`,
    `priority`, `offset` (in parent), `subregions`, `enabled`
  - `RegionType` enum: `Ram { ptr, size }`, `Rom { ptr, size }`,
    `Io { ops: MmioOps }`, `Alias { target, offset }`, `Container`
  - `MmioOps` trait: `fn read(&self, offset: u64, size: u32) -> u64`,
    `fn write(&mut self, offset: u64, size: u32, val: u64)`
  - Methods: `container()`, `ram()`, `io()`, `alias()`,
    `add_subregion()`, `add_subregion_with_priority()`

- [ ] **Step 3:** Implement `memory/src/ram.rs`:
  - `RamBlock` struct: mmap-backed host memory allocation
  - `new(size: u64) -> Self`: allocate via `libc::mmap`
  - `as_ptr() -> *mut u8`, `size() -> u64`
  - `Drop`: `libc::munmap`

- [ ] **Step 4:** Implement `memory/src/flat_view.rs`:
  - `FlatView` struct with `ranges: Vec<FlatRange>` sorted by GPA
  - `FlatRange { addr: GPA, size: u64, region_ref, offset_in_region }`
  - `from_region(root: &MemoryRegion) -> Self`: recursively flatten
    tree, handle priorities (higher priority wins on overlap)
  - `lookup(addr: GPA) -> Option<&FlatRange>`: binary search

- [ ] **Step 5:** Implement `memory/src/address_space.rs`:
  - `AddressSpace` struct wrapping root `MemoryRegion` + cached
    `RwLock<FlatView>`
  - `read(addr: GPA, size: u32) -> u64`: lookup FlatView → dispatch
    to RAM read or MMIO ops.read
  - `write(addr: GPA, size: u32, val: u64)`: same for write
  - `update_flat_view()`: rebuild FlatView from tree

- [ ] **Step 6:** Run tests: `cargo test -p machina-memory`. Fix until
  pass.

- [ ] **Step 7:** Commit: `memory: implement MemoryRegion tree and FlatView`

---

## Phase 2: JIT Engine Restructuring

### Task 2.1: Merge IR + Backend + Exec into accel/

**Files:**
- Create: `accel/Cargo.toml`
- Move: `core/src/ir/` → `accel/src/ir/`
- Move: `backend/src/` → `accel/src/` (optimize, liveness, constraint,
  regalloc, codegen, host/)
- Move: `exec/src/` → `accel/src/exec/`
- Move: `exec/src/timer.rs` → `accel/src/timer.rs` (or create new)
- Delete: `backend/`, `exec/` crate directories
- Modify: all crates that depended on `machina-core` for IR,
  `machina-backend`, or `machina-exec`

- [ ] **Step 1:** Create `accel/Cargo.toml` as `machina-accel`, depending
  on `machina-core`, `machina-memory`, `machina-util`.

- [ ] **Step 2:** Move IR modules from `core/src/` into `accel/src/ir/`.
  Update `core/src/lib.rs` to no longer export IR. Update `accel/src/lib.rs`
  to export `pub mod ir`.

- [ ] **Step 3:** Move backend modules (optimize, liveness, constraint,
  regalloc, codegen, host/) into `accel/src/`. Update internal imports.

- [ ] **Step 4:** Move exec modules (cpu_exec, translate_all, tb,
  tb_maint, cputlb) into `accel/src/exec/`.

- [ ] **Step 5:** Delete old `backend/` and `exec/` crate directories.
  Remove from workspace members.

- [ ] **Step 6:** Update all downstream crates (`guest/riscv`,
  `tests/cases`, `tools/*`) to depend on `machina-accel` instead of
  the old crates. Fix all import paths.

- [ ] **Step 7:** Run `cargo build && cargo test`. Fix compilation errors.
  All existing tests must pass.

- [ ] **Step 8:** Commit: `accel: unify IR, backend, and exec into single JIT crate`

---

### Task 2.2: Rename Frontend to Guest

**Files:**
- Rename: `frontend/` → `guest/riscv/`
- Modify: `guest/riscv/Cargo.toml` → `machina-guest-riscv`
- Update: all imports referencing `machina-frontend`

- [ ] **Step 1:** Move `frontend/` directory to `guest/riscv/`. Update
  `Cargo.toml` name to `machina-guest-riscv`.

- [ ] **Step 2:** Update dependency references in `accel/`, `tests/`,
  `tools/`, workspace root. Fix `use machina_frontend` →
  `use machina_guest_riscv`.

- [ ] **Step 3:** Run `cargo build && cargo test`. All tests pass.

- [ ] **Step 4:** Commit: `project: rename frontend to guest/riscv`

---

### Task 2.3: Virtual Clock and Timer

**Files:**
- Create: `accel/src/timer.rs`
- Test: `accel/tests/timer_tests.rs`

- [ ] **Step 1:** Write failing test:

```rust
#[test]
fn test_virtual_clock_step() {
    let clock = VirtualClock::new(ClockType::Virtual);
    assert_eq!(clock.get_ns(), 0);
    clock.step(1_000_000); // 1ms
    assert_eq!(clock.get_ns(), 1_000_000);
}

#[test]
fn test_timer_expiry() {
    let clock = VirtualClock::new(ClockType::Virtual);
    let fired = Arc::new(AtomicBool::new(false));
    let fired_clone = fired.clone();
    clock.add_timer(500_000, move || {
        fired_clone.store(true, Ordering::SeqCst);
    });
    clock.step(499_999);
    assert!(!fired.load(Ordering::SeqCst));
    clock.step(1);
    assert!(fired.load(Ordering::SeqCst));
}
```

- [ ] **Step 2:** Implement `accel/src/timer.rs`:
  - `ClockType` enum: `Realtime`, `Virtual`, `Host`
  - `VirtualClock`: `ns: AtomicI64`, `enabled: AtomicBool`,
    `timers: Mutex<BinaryHeap<TimerEntry>>`
  - `TimerEntry`: `expire_time: i64`, `callback: Box<dyn FnOnce()>`
  - `step(ns)`: advance clock, fire expired timers
  - `get_ns()`, `set_ns()`, `add_timer()`, `remove_timer()`

- [ ] **Step 3:** Run tests, fix until pass.

- [ ] **Step 4:** Commit: `accel: add virtual clock and timer system`

---

## Phase 3: RISC-V System Mode

### Task 3.1: Privilege Levels and Full CSR Set

**Files:**
- Modify: `guest/riscv/src/cpu.rs` — add privilege level, full CSR
- Create: `guest/riscv/src/csr.rs` — CSR read/write dispatch
- Test: `guest/riscv/tests/csr_tests.rs`

- [ ] **Step 1:** Write failing CSR tests:

```rust
#[test]
fn test_mstatus_read_write() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::Machine);
    cpu.csr_write(CSR_MSTATUS, 0x0000_0000_0000_0088); // MPP=S, MIE=1
    let val = cpu.csr_read(CSR_MSTATUS);
    assert_eq!(val & 0x1888, 0x0088); // MPP and MIE bits
}

#[test]
fn test_csr_privilege_check() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::Supervisor);
    // S-mode cannot read M-mode CSRs
    assert!(cpu.try_csr_read(CSR_MSTATUS).is_err());
    // S-mode can read S-mode CSRs
    assert!(cpu.try_csr_read(CSR_SSTATUS).is_ok());
}

#[test]
fn test_medeleg_delegation() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::Machine);
    // Delegate illegal instruction to S-mode
    cpu.csr_write(CSR_MEDELEG, 1 << 2);
    assert_eq!(cpu.csr_read(CSR_MEDELEG) & (1 << 2), 1 << 2);
}
```

- [ ] **Step 2:** Extend `RiscvCpu` in `guest/riscv/src/cpu.rs`:
  - Add `priv_level: PrivLevel` field (enum: `User=0, Supervisor=1,
    Machine=3`)
  - Add `csr: CsrFile` field containing all M/S/U CSR registers
  - Add `interrupt_request: AtomicU32`, `halted: AtomicBool`

- [ ] **Step 3:** Implement `guest/riscv/src/csr.rs`:
  - `CsrFile` struct: all M-mode CSRs (`mstatus`, `misa`, `medeleg`,
    `mideleg`, `mie`, `mtvec`, `mscratch`, `mepc`, `mcause`, `mtval`,
    `mip`), all S-mode CSRs (`sstatus`, `sie`, `stvec`, `sscratch`,
    `sepc`, `scause`, `stval`, `sip`, `satp`), U-mode (`fflags`,
    `frm`, `fcsr`)
  - `csr_read(addr: u16) -> Result<u64, Exception>`: grouped match by
    address range (0x000-0x0FF U-mode, 0x100-0x1FF S-mode,
    0x300-0x3FF M-mode), privilege check
  - `csr_write(addr: u16, val: u64) -> Result<(), Exception>`: same
    with WARL/WLRL field masking
  - `sstatus` reads/writes alias into `mstatus` with S-mode mask
  - `sip`/`sie` alias into `mip`/`mie` with delegation mask
  - `VendorCsr` trait + `Option<Box<dyn VendorCsr>>` for extensions

- [ ] **Step 4:** Run tests. Commit: `guest/riscv: add privilege levels and full CSR set`

---

### Task 3.2: Exception and Interrupt Model

**Files:**
- Create: `guest/riscv/src/exception.rs`
- Test: `guest/riscv/tests/exception_tests.rs`

- [ ] **Step 1:** Write failing tests:

```rust
#[test]
fn test_exception_to_m_mode() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::Supervisor);
    cpu.pc = 0x8020_1000;
    cpu.csr_write(CSR_MTVEC, 0x8000_0000); // Direct mode
    cpu.raise_exception(Exception::IllegalInstruction, 0);
    assert_eq!(cpu.pc, 0x8000_0000);
    assert_eq!(cpu.priv_level, PrivLevel::Machine);
    assert_eq!(cpu.csr_read(CSR_MEPC), 0x8020_1000);
    assert_eq!(cpu.csr_read(CSR_MCAUSE), 2); // IllegalInstruction
}

#[test]
fn test_exception_delegated_to_s_mode() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::User);
    cpu.pc = 0x1000;
    cpu.csr.medeleg = 1 << 8; // Delegate ecall-from-U to S-mode
    cpu.csr_write(CSR_STVEC, 0x8020_0000);
    cpu.raise_exception(Exception::EcallFromU, 0);
    assert_eq!(cpu.pc, 0x8020_0000);
    assert_eq!(cpu.priv_level, PrivLevel::Supervisor);
    assert_eq!(cpu.csr_read(CSR_SEPC), 0x1000);
}

#[test]
fn test_mret() {
    let mut cpu = RiscvCpu::new();
    cpu.set_priv(PrivLevel::Machine);
    cpu.csr.mepc = 0x8020_1004;
    // Set MPP=S in mstatus
    cpu.csr.mstatus |= 1 << 11; // MPP = S
    cpu.execute_mret();
    assert_eq!(cpu.pc, 0x8020_1004);
    assert_eq!(cpu.priv_level, PrivLevel::Supervisor);
}
```

- [ ] **Step 2:** Implement `guest/riscv/src/exception.rs`:
  - `Exception` enum: all RISC-V exception causes (InstructionMisaligned,
    InstructionAccessFault, IllegalInstruction, Breakpoint,
    LoadMisaligned, LoadAccessFault, StoreMisaligned,
    StoreAccessFault, EcallFromU, EcallFromS, EcallFromM,
    InstructionPageFault, LoadPageFault, StorePageFault)
  - `Interrupt` enum: SSI, MSI, STI, MTI, SEI, MEI
  - `raise_exception(&mut self, excp, tval)`:
    1. Determine target mode (check medeleg for delegation)
    2. Save `pc` → `xepc`, cause → `xcause`, tval → `xtval`
    3. Update `xstatus` (save prev priv in xPP, clear xIE)
    4. Set `pc` → `xtvec` (direct or vectored)
    5. Switch privilege level
  - `handle_interrupt(&mut self) -> bool`: check `mip & mie`, respect
    delegation, highest priority first (MEI>MSI>MTI>SEI>SSI>STI)
  - `execute_mret(&mut self)`, `execute_sret(&mut self)`:
    restore privilege from xPP, restore xIE from xPIE, set pc=xepc

- [ ] **Step 3:** Run tests. Commit: `guest/riscv: add exception and interrupt model`

---

### Task 3.3: MMU — Sv39 Page Table Walk and TLB

**Files:**
- Create: `guest/riscv/src/mmu.rs`
- Test: `guest/riscv/tests/mmu_tests.rs`

- [ ] **Step 1:** Write failing tests:

```rust
#[test]
fn test_sv39_translate_identity_map() {
    let mut mmu = Mmu::new();
    // Build identity-mapped page table in RAM
    let ram = RamBlock::new(0x100_0000); // 16 MiB
    let pt = build_identity_page_table(ram.as_ptr(), 0x8000_0000, 0x80);
    mmu.set_satp(make_satp(Sv39, pt));
    let result = mmu.translate(
        GVA::new(0x8000_1000), AccessType::Read, PrivLevel::Supervisor,
        &ram,
    );
    assert_eq!(result.unwrap(), GPA::new(0x8000_1000));
}

#[test]
fn test_sv39_page_fault() {
    let mut mmu = Mmu::new();
    let ram = RamBlock::new(0x100_0000);
    // Page table with no mapping for 0xDEAD_0000
    let pt = build_identity_page_table(ram.as_ptr(), 0x8000_0000, 0x1);
    mmu.set_satp(make_satp(Sv39, pt));
    let result = mmu.translate(
        GVA::new(0xDEAD_0000), AccessType::Read, PrivLevel::Supervisor,
        &ram,
    );
    assert!(result.is_err()); // LoadPageFault
}

#[test]
fn test_tlb_hit() {
    let mut mmu = Mmu::new();
    // ... setup page table ...
    // First access: TLB miss, page walk
    let _ = mmu.translate(GVA::new(0x8000_0000), AccessType::Read,
        PrivLevel::Supervisor, &ram);
    // Second access: TLB hit, no page walk
    let stats_before = mmu.stats();
    let _ = mmu.translate(GVA::new(0x8000_0000), AccessType::Read,
        PrivLevel::Supervisor, &ram);
    assert_eq!(mmu.stats().page_walks, stats_before.page_walks);
}
```

- [ ] **Step 2:** Implement `guest/riscv/src/mmu.rs`:
  - `Mmu` struct: `satp: u64`, `tlb: [TlbEntry; 256]`, `stats`
  - `TlbEntry`: `valid`, `vpn`, `ppn`, `perm`, `asid`
  - `translate(gva, access_type, priv, mem) -> Result<GPA, Exception>`:
    1. TLB lookup (hash vpn → index, compare tag)
    2. On miss: `walk_page_table()` for Sv39 (3-level)
    3. Permission checks (R/W/X, U-bit, MXR, SUM flags from mstatus)
    4. Refill TLB on success
    5. Return GPA or page fault exception
  - `walk_page_table()`: 3-level walk (VPN[2]→VPN[1]→VPN[0]),
    handle superpage (megapage/gigapage), check V/R/W/X/U/A/D bits
  - `flush()`: clear all TLB entries (sfence.vma)
  - `flush_page(vpn)`: invalidate single entry

- [ ] **Step 3:** Run tests. Commit: `guest/riscv: add Sv39 MMU with TLB`

---

### Task 3.4: PMP (Physical Memory Protection)

**Files:**
- Create: `guest/riscv/src/pmp.rs`
- Test: `guest/riscv/tests/pmp_tests.rs`

- [ ] **Step 1:** Write tests for PMP NAPOT/TOR/NA4 matching and
  R/W/X permission checks.

- [ ] **Step 2:** Implement `Pmp` struct: 16 entries, each with
  `pmpcfg` (R/W/X/A/L bits) and `pmpaddr`. Implement `check_access(addr,
  size, access_type, priv) -> Result<(), Exception>`.

- [ ] **Step 3:** Integrate PMP check into MMU translate path (after
  GVA→GPA, check PMP on GPA). Run tests.

- [ ] **Step 4:** Commit: `guest/riscv: add PMP support`

---

### Task 3.5: Privileged Instruction Translation

**Files:**
- Modify: `guest/riscv/src/translate/` — add SRET, MRET, WFI,
  SFENCE.VMA translation
- Test: integration tests that verify privilege transitions

- [ ] **Step 1:** Add translation for `SRET`, `MRET`:
  - Generate IR that exits TB with `TB_EXIT_NOCHAIN` and a new exit
    reason indicating privilege-changing instruction
  - In execution loop, call `cpu.execute_sret()` / `cpu.execute_mret()`

- [ ] **Step 2:** Add translation for `WFI`:
  - Generate IR that sets `cpu.halted = true` and exits TB
  - Execution loop checks `halted` flag, skips execution until
    interrupt arrives

- [ ] **Step 3:** Add translation for `SFENCE.VMA`:
  - Generate IR that calls `cpu.tlb_flush()` or
    `cpu.tlb_flush_page(rs1)` and exits TB (must not chain — address
    translation may have changed)

- [ ] **Step 4:** Modify CSR instruction translation to check privilege
  level at translate time where possible, otherwise generate runtime
  check + exception.

- [ ] **Step 5:** Modify ECALL translation: generate different exception
  codes based on current privilege level (`EcallFromU`, `EcallFromS`,
  `EcallFromM`).

- [ ] **Step 6:** Add floating-point `mstatus.FS` check: at translate
  time, generate check for `FS == 0` → illegal instruction exception.
  On FP execution, generate code to mark `FS = Dirty`.

- [ ] **Step 7:** Run full test suite. Commit:
  `guest/riscv: add privileged instruction translation`

---

## Phase 4: Device Infrastructure

### Task 4.1: Device Object Model (qdev)

**Files:**
- Create: `hw/core/src/qdev.rs`
- Create: `hw/core/src/bus.rs`
- Test: `hw/core/tests/qdev_tests.rs`

- [ ] **Step 1:** Write failing test:

```rust
#[test]
fn test_device_realize_reset() {
    let mut dev = TestDevice::new("test0");
    dev.realize().unwrap();
    assert_eq!(dev.realized(), true);
    dev.reset();
    assert_eq!(dev.read(0x0, 4), 0); // Reset clears registers
}
```

- [ ] **Step 2:** Implement `hw/core/src/qdev.rs`:
  - `Device` trait (inspired by QEMU Rust QOM, adapted for pure Rust):
    `fn realize(&mut self) -> Result<()>`,
    `fn reset(&mut self)`,
    `fn realized(&self) -> bool`,
    `fn name(&self) -> &str`
  - `DeviceState` base struct: `name`, `realized`, `parent_bus`

- [ ] **Step 3:** Implement `hw/core/src/bus.rs`:
  - `BusDevice` trait: `fn read(&self, offset: u64, size: u32) -> u64`,
    `fn write(&mut self, offset: u64, size: u32, val: u64)`
  - `SysBus` struct: manages list of `MemoryRegion` mappings for
    devices, connects to `AddressSpace`

- [ ] **Step 4:** Run tests. Commit: `hw/core: add device object model and bus`

---

### Task 4.2: IRQ Routing

**Files:**
- Create: `hw/core/src/irq.rs`
- Test: `hw/core/tests/irq_tests.rs`

- [ ] **Step 1:** Write failing test:

```rust
#[test]
fn test_irq_set_clear() {
    let sink = Arc::new(TestIrqSink::new());
    let line = IrqLine::new(sink.clone(), 0);
    line.set(true);
    assert_eq!(sink.level(0), true);
    line.set(false);
    assert_eq!(sink.level(0), false);
}
```

- [ ] **Step 2:** Implement `hw/core/src/irq.rs`:
  - `IrqSink` trait: `fn set_irq(&self, irq: u32, level: bool)`
  - `IrqLine` struct: holds `Arc<dyn IrqSink>` + irq number
  - `OrIrq`: combines multiple input lines with OR logic
  - `SplitIrq`: fans out one input to multiple outputs

- [ ] **Step 3:** Run tests. Commit: `hw/core: add IRQ routing`

---

### Task 4.3: Chardev Backend Framework

**Files:**
- Create: `hw/core/src/chardev/mod.rs`
- Create: `hw/core/src/chardev/null.rs`
- Create: `hw/core/src/chardev/stdio.rs`
- Create: `hw/core/src/chardev/socket.rs`
- Test: `hw/core/tests/chardev_tests.rs`

- [ ] **Step 1:** Write failing test:

```rust
#[test]
fn test_null_chardev() {
    let mut null = NullChardev::new();
    assert_eq!(null.write(b"hello").unwrap(), 5);
    assert_eq!(null.can_read(), 0);
}

#[test]
fn test_chardev_frontend_backend() {
    let backend = Box::new(NullChardev::new());
    let mut fe = CharFrontend::new();
    fe.attach(backend);
    assert_eq!(fe.write(b"data").unwrap(), 4);
}
```

- [ ] **Step 2:** Implement `hw/core/src/chardev/mod.rs`:
  - `Chardev` trait: `write`, `can_read`, `read`, `set_handlers`
  - `CharFrontend` struct: holds `Option<Box<dyn Chardev>>`,
    callback slots (`on_can_read`, `on_read`, `on_event`)
  - `CharEvent` enum: `Opened`, `Closed`, `Break`
  - `CharHandlers` struct for callback registration

- [ ] **Step 3:** Implement `null.rs` (discard output, no input),
  `stdio.rs` (read stdin, write stdout with raw terminal mode),
  `socket.rs` (TCP listener + client, basic nonblocking I/O).

- [ ] **Step 4:** Run tests. Commit: `hw/core: add chardev frontend-backend framework`

---

### Task 4.4: Device Clock

**Files:**
- Create: `hw/core/src/clock.rs`
- Test: `hw/core/tests/clock_tests.rs`

- [ ] **Step 1:** Implement device clock: `DeviceClock` struct with
  `frequency: u64`, `period_ns: u64`. Methods: `set_frequency()`,
  `get_period_ns()`. Children clocks can subscribe to parent frequency
  changes.

- [ ] **Step 2:** Test frequency propagation. Commit:
  `hw/core: add device clock`

---

### Task 4.5: FDT Generator

**Files:**
- Create: `hw/core/src/fdt.rs`
- Test: `hw/core/tests/fdt_tests.rs`

- [ ] **Step 1:** Write failing test:

```rust
#[test]
fn test_fdt_basic_tree() {
    let mut fdt = FdtBuilder::new();
    fdt.begin_node("").unwrap(); // root
    fdt.property_string("compatible", "machina,ref").unwrap();
    fdt.property_u32("#address-cells", 2).unwrap();
    fdt.property_u32("#size-cells", 2).unwrap();
    fdt.end_node().unwrap();
    let blob = fdt.finish().unwrap();
    assert!(blob.len() > 0);
    // Verify FDT magic
    assert_eq!(&blob[0..4], &[0xd0, 0x0d, 0xfe, 0xed]);
}
```

- [ ] **Step 2:** Implement `FdtBuilder`:
  - Builds FDT blob in memory following devicetree spec
  - Methods: `begin_node()`, `end_node()`, `property_u32()`,
    `property_u64()`, `property_string()`, `property_bytes()`,
    `finish() -> Vec<u8>`
  - Generate proper header, structure block, strings block

- [ ] **Step 3:** Run tests. Commit: `hw/core: add FDT generator`

---

### Task 4.6: Firmware/Kernel Loader

**Files:**
- Create: `hw/core/src/loader.rs`
- Test: `hw/core/tests/loader_tests.rs`

- [ ] **Step 1:** Implement `load_binary(path, mem, addr) -> Result<u64>`
  — load raw binary into memory at specified GPA, return entry point.

- [ ] **Step 2:** Implement `load_elf(path, mem) -> Result<u64>` — parse
  ELF64 headers, load PT_LOAD segments into memory, return entry point.

- [ ] **Step 3:** Tests with small test binaries. Commit:
  `hw/core: add firmware and kernel loader`

---

## Phase 5: RISC-V Devices and Machine

### Task 5.1: PLIC (Platform-Level Interrupt Controller)

**Files:**
- Create: `hw/intc/src/sifive_plic.rs`
- Test: `hw/intc/tests/plic_tests.rs`

- [ ] **Step 1:** Write failing tests:

```rust
#[test]
fn test_plic_priority_and_claim() {
    let mut plic = Plic::new(128, 2); // 128 sources, 2 contexts
    plic.realize().unwrap();
    // Set source 10 priority = 5
    plic.write(0x28, 4, 5); // offset 10*4 = 0x28
    // Enable source 10 for context 0
    plic.write(0x2000 + 0, 4, 1 << 10); // enable bit
    // Set threshold for context 0 = 0
    plic.write(0x20_0000 + 0, 4, 0);
    // Trigger IRQ 10
    plic.set_irq(10, true);
    // Claim should return 10
    let claimed = plic.read(0x20_0004, 4);
    assert_eq!(claimed, 10);
}
```

- [ ] **Step 2:** Implement `Plic` struct:
  - `source_priority: Vec<u32>` (per-source priority)
  - `pending: Vec<bool>` (per-source pending bits)
  - `enable: Vec<Vec<u32>>` (per-context enable bitmask)
  - `threshold: Vec<u32>`, `claim: Vec<u32>` (per-context)
  - MMIO layout matching SiFive PLIC spec:
    - `0x000000-0x000FFF`: priorities
    - `0x001000-0x001FFF`: pending bits
    - `0x002000-0x1FFFFF`: enable bits (per context)
    - `0x200000+`: threshold + claim/complete (per context)
  - `set_irq(source, level)`: update pending, evaluate and signal
    to connected `IrqLine`
  - Claim logic: return highest priority pending enabled source
  - Complete: clear claim, re-evaluate

- [ ] **Step 3:** Connect PLIC output to `IrqLine` that signals CPU
  external interrupt (`MEI`/`SEI`).

- [ ] **Step 4:** Run tests. Commit: `hw/intc: add PLIC`

---

### Task 5.2: ACLINT (CLINT-Compatible Timer + IPI)

**Files:**
- Create: `hw/intc/src/riscv_aclint.rs`
- Test: `hw/intc/tests/aclint_tests.rs`

- [ ] **Step 1:** Write failing tests:

```rust
#[test]
fn test_aclint_mtime_read() {
    let aclint = Aclint::new(1);
    aclint.realize().unwrap();
    // mtime at offset 0xBFF8
    let t1 = aclint.read(0xBFF8, 8);
    std::thread::sleep(std::time::Duration::from_millis(1));
    let t2 = aclint.read(0xBFF8, 8);
    assert!(t2 > t1);
}

#[test]
fn test_aclint_timer_interrupt() {
    let sink = Arc::new(TestIrqSink::new());
    let mut aclint = Aclint::new(1);
    aclint.connect_timer_irq(0, IrqLine::new(sink.clone(), 0));
    aclint.realize().unwrap();
    // Set mtimecmp[0] = current mtime + 1 (fire immediately)
    let now = aclint.read(0xBFF8, 8);
    aclint.write(0x4000, 8, now + 1); // mtimecmp[0]
    aclint.tick(); // Advance time check
    assert!(sink.level(0)); // MTI should fire
}
```

- [ ] **Step 2:** Implement `Aclint`:
  - MTIMER: `mtime` (64-bit, monotonic), `mtimecmp[N]` per hart
  - MSWI: `msip[N]` per hart (write 1 → raise MSI, write 0 → clear)
  - SSWI: `ssip[N]` per hart (same for SSI)
  - MMIO layout (CLINT-compatible):
    - `0x0000-0x3FFF`: MSIP registers (4 bytes per hart)
    - `0x4000-0xBFF7`: mtimecmp registers (8 bytes per hart)
    - `0xBFF8-0xBFFF`: mtime (8 bytes)
  - Timer comparison: when `mtime >= mtimecmp[i]`, raise MTI for
    hart i via connected `IrqLine`
  - Register virtual clock timer callback for periodic mtime advance

- [ ] **Step 3:** Run tests. Commit: `hw/intc: add ACLINT`

---

### Task 5.3: UART (ns16550a)

**Files:**
- Create: `hw/char/src/ns16550a.rs`
- Test: `hw/char/tests/uart_tests.rs`

- [ ] **Step 1:** Write failing tests:

```rust
#[test]
fn test_uart_tx() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let out_clone = output.clone();
    let mut uart = Ns16550a::new();
    uart.attach_chardev(Box::new(TestChardev::new(out_clone)));
    uart.realize().unwrap();
    uart.write(0, 4, 0x41); // Write 'A' to THR
    assert_eq!(*output.lock().unwrap(), vec![0x41]);
}

#[test]
fn test_uart_lsr_tx_empty() {
    let mut uart = Ns16550a::new();
    uart.attach_chardev(Box::new(NullChardev::new()));
    uart.realize().unwrap();
    let lsr = uart.read(5, 1); // LSR
    assert_ne!(lsr & 0x60, 0); // THRE + TEMT set when idle
}

#[test]
fn test_uart_rx_interrupt() {
    let irq_sink = Arc::new(TestIrqSink::new());
    let mut uart = Ns16550a::new();
    uart.connect_irq(IrqLine::new(irq_sink.clone(), 0));
    uart.realize().unwrap();
    // Enable RX interrupt (IER bit 0)
    uart.write(1, 1, 0x01);
    // Simulate chardev delivering a byte
    uart.receive_byte(0x42);
    assert!(irq_sink.level(0)); // IRQ raised
    let rbr = uart.read(0, 1); // Read RBR clears IRQ
    assert_eq!(rbr, 0x42);
}
```

- [ ] **Step 2:** Implement `Ns16550a`:
  - Registers: RBR/THR, IER, IIR/FCR, LCR, MCR, LSR, MSR, SCR,
    DLL/DLM (baud divisor)
  - 16-byte RX FIFO, 16-byte TX FIFO
  - IRQ conditions: RX data available, TX holding empty, RX line
    status, modem status
  - Connect to `CharFrontend` for I/O
  - MMIO: 8 registers at byte offsets 0-7

- [ ] **Step 3:** Run tests. Commit: `hw/char: add ns16550a UART`

---

### Task 5.4: riscv64-ref Machine Definition

**Files:**
- Create: `hw/riscv/src/ref_machine.rs`
- Create: `hw/riscv/src/boot.rs`
- Create: `hw/riscv/src/sbi.rs`
- Test: `hw/riscv/tests/ref_machine_tests.rs`

- [ ] **Step 1:** Implement `hw/riscv/src/ref_machine.rs`:
  - `RefMachine` struct implementing `Machine` trait
  - `init()`:
    1. Create root `AddressSpace`
    2. Create RAM `MemoryRegion` at 0x8000_0000 (size from opts)
    3. Create + realize ACLINT at 0x0200_0000
    4. Create + realize PLIC at 0x0C00_0000
    5. Create + realize UART0 at 0x1000_0000
    6. Wire IRQs: UART→PLIC source 10, PLIC→CPU MEI/SEI
    7. Wire ACLINT timer→CPU MTI, ACLINT MSWI→CPU MSI
    8. Create chardev for UART (stdio or socket per config)
  - Memory map matches QEMU virt (§10.1 of spec)

- [ ] **Step 2:** Implement `hw/riscv/src/boot.rs`:
  - `boot_ref_machine(machine, opts)`:
    1. Load BIOS (RustSBI) to 0x8000_0000
    2. Load kernel to 0x8020_0000
    3. Generate FDT describing machine, place at top of RAM
    4. Set CPU registers: `a0 = hart_id`, `a1 = fdt_addr`
    5. Set CPU PC = 0x8000_0000, priv = Machine
  - FDT generation: CPU nodes, memory node, PLIC/ACLINT/UART nodes
    with correct `reg` and `interrupts` properties

- [ ] **Step 3:** Implement `hw/riscv/src/sbi.rs`:
  - SBI ecall dispatch (for built-in minimal SBI fallback only; primary
    path is RustSBI firmware running in M-mode)
  - Handle SBI extension IDs: Base, Timer, IPI, RFENCE, HSM, SRST
  - This is used only when no BIOS is provided (`-bios none`)

- [ ] **Step 4:** Write test that creates a `RefMachine`, calls `init()`,
  verifies memory map layout (RAM accessible, UART MMIO responds).

- [ ] **Step 5:** Commit: `hw/riscv: add riscv64-ref machine definition`

---

### Task 5.5: RustSBI Submodule Integration

**Files:**
- Create: `.gitmodules` entry for `third-party/rustsbi/`
- Create: `build.rs` or Makefile for cross-compiling RustSBI

- [ ] **Step 1:** Add RustSBI as git submodule:
  `git submodule add https://github.com/rustsbi/rustsbi third-party/rustsbi`

- [ ] **Step 2:** Create build script that compiles RustSBI for
  `riscv64gc-unknown-none-elf` target, producing `rustsbi.bin`.
  Document the cross-compilation requirements (riscv64 target installed).

- [ ] **Step 3:** Verify `rustsbi.bin` can be loaded by the `boot.rs`
  loader. Commit: `project: add RustSBI as git submodule`

---

## Phase 6: Debug and Monitor

### Task 6.1: mtest Framework

**Files:**
- Modify: `tests/mtest/src/lib.rs`
- Create: `tests/mtest/src/protocol.rs`
- Create: `tests/mtest/src/client.rs`
- Create: `tests/mtest/src/machine.rs`
- Test: `tests/cases/system/mtest_basic.rs`

- [ ] **Step 1:** Implement mtest text protocol parser/serializer in
  `tests/mtest/src/protocol.rs`:
  - Commands: `readb`, `readw`, `readl`, `readq`, `writeb`, `writew`,
    `writel`, `writeq`, `read`, `write`, `clock_step`, `clock_set`,
    `irq_intercept_out`, `set_irq_in`
  - Responses: `OK`, `OK <value>`, `FAIL`
  - Async: `IRQ raise <num>`, `IRQ lower <num>`

- [ ] **Step 2:** Implement mtest server in machina binary:
  - When launched with `-mtest <socket_path>`, listen on Unix socket
  - Parse incoming commands, dispatch to memory/device/clock subsystems
  - Send responses

- [ ] **Step 3:** Implement client API in `tests/mtest/src/client.rs`:

```rust
pub struct MachinaTest { /* unix socket connection */ }

impl MachinaTest {
    pub fn start(args: &str) -> Self { /* spawn machina, connect */ }
    pub fn readl(&self, addr: u64) -> u32 { /* send readl, parse OK */ }
    pub fn writel(&self, addr: u64, val: u32) { /* send writel */ }
    pub fn clock_step(&self, ns: u64) -> u64 { /* send clock_step */ }
    pub fn get_irq(&self, num: u32) -> bool { /* check IRQ state */ }
}

impl Drop for MachinaTest {
    fn drop(&mut self) { /* kill child process */ }
}
```

- [ ] **Step 4:** Write basic system test:

```rust
#[test]
fn test_mtest_uart_read() {
    let m = MachinaTest::start("-M riscv64-ref -m 128M");
    // UART LSR should show TX empty on startup
    let lsr = m.readl(0x1000_0005);
    assert_ne!(lsr & 0x60, 0);
}
```

- [ ] **Step 5:** Run tests. Commit: `tests: add mtest framework`

---

### Task 6.2: GDB Stub

**Files:**
- Create: `monitor/src/gdbstub/mod.rs`
- Create: `monitor/src/gdbstub/protocol.rs`
- Create: `monitor/src/gdbstub/commands.rs`

- [ ] **Step 1:** Implement GDB RSP packet parsing in
  `monitor/src/gdbstub/protocol.rs`:
  - Packet format: `$<data>#<checksum>`
  - `parse_packet(buf) -> Option<GdbPacket>`
  - `encode_packet(data) -> Vec<u8>`
  - ACK/NACK handling (`+`/`-`)

- [ ] **Step 2:** Implement command handlers in `commands.rs`:
  - `g` / `G`: read/write all registers (via `GuestCpu` trait)
  - `m` / `M`: read/write memory (via `AddressSpace`)
  - `s`: single-step (set step flag, execute 1 insn, break)
  - `c`: continue (resume execution)
  - `z` / `Z`: SW breakpoints (TB invalidation at breakpoint addr,
    insert trap insn)
  - `?`: stop reason
  - `qSupported`: feature negotiation

- [ ] **Step 3:** Implement TCP listener: `-gdb tcp::<port>` or `-s`
  (shorthand for `-gdb tcp::1234`). `-S` = start paused (wait for GDB
  `c` command).

- [ ] **Step 4:** Integration test: start machina with `-s -S`, connect
  with GDB, read PC register, step one instruction, verify PC changed.

- [ ] **Step 5:** Commit: `monitor: add GDB remote stub`

---

### Task 6.3: MMP (Machina Monitor Protocol)

**Files:**
- Create: `monitor/src/mmp/mod.rs`
- Create: `monitor/src/mmp/protocol.rs`
- Create: `monitor/src/mmp/commands.rs`
- Create: `monitor/src/mmp/server.rs`

- [ ] **Step 1:** Implement JSON-RPC protocol in `protocol.rs`:
  - `MmpRequest`: `{"execute": "<cmd>", "arguments": {...}}`
  - `MmpResponse`: `{"return": {...}}` or `{"error": {...}}`
  - `MmpEvent`: `{"event": "<name>", "timestamp": {...}}`
  - Serde-based serialization/deserialization

- [ ] **Step 2:** Implement core commands in `commands.rs`:
  - `query-status` → `{"status": "running"|"paused", ...}`
  - `stop` → pause all vCPUs
  - `cont` → resume all vCPUs
  - `system_reset` → call `machine.reset()`
  - `system_powerdown` → call `machine.shutdown()`
  - `quit` → exit process
  - `query-cpus` → list vCPU states
  - `human-monitor-command` → execute HMP command, return output

- [ ] **Step 3:** Implement server in `server.rs`:
  - Listen on chardev (Unix socket or TCP via `-mmp` option)
  - Parse JSON lines, dispatch to command handlers, send responses
  - Emit events (STOP, RESUME, SHUTDOWN, etc.)

- [ ] **Step 4:** Commit: `monitor: add MMP protocol`

---

### Task 6.4: HMP (Human Monitor Protocol)

**Files:**
- Create: `monitor/src/hmp/mod.rs`
- Create: `monitor/src/hmp/parser.rs`
- Create: `monitor/src/hmp/commands.rs`

- [ ] **Step 1:** Implement HMP command parser in `parser.rs`:
  - Line-based text protocol
  - Commands: `info status`, `info registers`, `info cpus`,
    `stop`, `cont`, `quit`, `system_reset`,
    `x/<count><fmt><size> <addr>` (memory examine),
    `print <expr>`, `help`

- [ ] **Step 2:** Implement each HMP command by calling the corresponding
  MMP command internally and formatting the result as human-readable
  text.

- [ ] **Step 3:** Wire HMP to `-monitor` chardev option. Display
  `(machina)` prompt when interactive.

- [ ] **Step 4:** Commit: `monitor: add HMP protocol (wraps MMP)`

---

## Phase 7: Integration and Boot

### Task 7.1: Binary Entry Point (src/main.rs)

**Files:**
- Modify: `src/main.rs`
- Add dependency: `clap` for argument parsing

- [ ] **Step 1:** Implement QEMU-style argument parsing:
  - `-M` / `-machine` `<name>`: select machine (required)
  - `-m` `<size>`: RAM size (default 128M)
  - `-smp` `<n>`: CPU count (default 1)
  - `-bios` `<path>`: firmware binary
  - `-kernel` `<path>`: kernel binary
  - `-append` `<string>`: kernel command line
  - `-nographic`: disable graphical output, serial→stdio
  - `-serial` `<chardev>`: serial port backend
  - `-chardev` `<spec>`: define chardev backend
  - `-monitor` `<chardev>`: monitor backend
  - `-mmp` `<chardev>`: MMP server
  - `-gdb` `<dev>`: GDB server
  - `-s`: shorthand for `-gdb tcp::1234`
  - `-S`: start paused
  - `-mtest` `<socket>`: mtest server
  - `-M ?`: list available machines

- [ ] **Step 2:** Machine registry: map machine names to constructors.
  Register `riscv64-ref` → `RefMachine::new()`.

- [ ] **Step 3:** Main flow:
  1. Parse arguments
  2. Create machine from `-M` name
  3. Call `machine.init(opts)`
  4. Set up chardevs, connect to devices
  5. Start monitor/gdb/mtest servers if requested
  6. Call `machine.boot()`
  7. Enter execution loop (call `cpu_exec_loop_mt` or single-thread
     depending on `-smp` and icount settings)

- [ ] **Step 4:** Implement alias detection: if binary is invoked as
  `machina-riscv64`, prepend `-M riscv64-ref` to args.

- [ ] **Step 5:** Run `cargo build`. Verify `machina -M ?` lists
  `riscv64-ref`. Commit: `project: implement CLI entry point`

---

### Task 7.2: Execution Loop Integration

**Files:**
- Modify: `accel/src/exec/cpu_exec.rs`
- Modify: `system/src/cpus.rs`

- [ ] **Step 1:** Modify execution loop for full-system mode:
  - At TB exit points, check `cpu.pending_interrupt()` and call
    `cpu.handle_interrupt()` if pending
  - Check `cpu.is_halted()` — if halted, sleep until interrupt
    (condvar wakeup from device IRQ delivery)
  - Check virtual clock timers (Phase 1: inline poll)
  - Check mtest/gdb breakpoints if active

- [ ] **Step 2:** Implement `system/src/cpus.rs`:
  - `CpuManager` struct: manages vCPU threads
  - `start(cpus: Vec<Box<dyn GuestCpu>>, shared: SharedState)`:
    spawn one thread per vCPU, each runs `cpu_exec_loop`
  - `pause_all()`, `resume_all()`, `stop_all()` via atomic flags +
    condvar
  - Single-thread mode: run all vCPUs in one thread, round-robin

- [ ] **Step 3:** Wire `CpuManager` into `src/main.rs` execution flow.

- [ ] **Step 4:** Commit: `system: integrate execution loop for full-system mode`

---

### Task 7.3: Self-Modifying Code Detection

**Files:**
- Modify: `accel/src/exec/tb_maint.rs`
- Modify: `memory/src/ram.rs`

- [ ] **Step 1:** Add per-page code bitmap to RAM: track which 4K pages
  have associated TBs.

- [ ] **Step 2:** On TB translation: mark the page(s) covered by the TB
  in the bitmap.

- [ ] **Step 3:** On memory write to a code-marked page: flush all TBs
  for that page (call `tb_invalidate_page()`).

- [ ] **Step 4:** Test with self-modifying code guest program. Commit:
  `accel: add self-modifying code detection`

---

### Task 7.4: End-to-End Boot Test

**Files:**
- Create: `tests/cases/system/boot_rcore.rs`

- [ ] **Step 1:** Create test that boots rCore-Tutorial v3 (Ch1-3):
  - Start `machina -M riscv64-ref -m 128M -bios rustsbi.bin
    -kernel rcore-ch3.bin -nographic -serial file:output.log`
  - Wait for process (with timeout)
  - Check `output.log` contains expected rCore boot messages

- [ ] **Step 2:** Iteratively debug boot failures using gdbstub:
  - Common issues: CSR access traps, MMU faults, timer not firing,
    UART not outputting
  - Fix each issue in the relevant component
  - Document fixes

- [ ] **Step 3:** Extend to Ch4-8 (memory management, process scheduling,
  file system basics):
  - Each chapter may reveal new missing features
  - Add CSR/extension support as needed
  - Fix page table walk edge cases

- [ ] **Step 4:** Final verification: rCore-Tutorial v3 Ch1-8 boots to
  user shell, can run basic user programs.

- [ ] **Step 5:** Commit: `tests: add rCore-Tutorial v3 boot test`

---

## Dependency Graph

```
Phase 1 (Foundation)
  Task 1.1 → Task 1.2 → Task 1.3

Phase 2 (JIT Restructuring)
  Task 2.1 → Task 2.2 → Task 2.3
  (depends on Phase 1)

Phase 3 (RISC-V System Mode)
  Task 3.1 → Task 3.2 → Task 3.3 → Task 3.4 → Task 3.5
  (depends on Phase 2)

Phase 4 (Device Infrastructure)
  Task 4.1 → Task 4.2 (parallel with 4.3, 4.4, 4.5, 4.6)
  (depends on Phase 1 Task 1.3)

Phase 5 (Devices & Machine)
  Task 5.1, 5.2, 5.3 (parallel, depend on Phase 4)
  Task 5.4 (depends on 5.1, 5.2, 5.3)
  Task 5.5 (independent, can start early)

Phase 6 (Debug & Monitor)
  Task 6.1, 6.2, 6.3, 6.4 (parallel)
  (depends on Phase 4 + Phase 5)

Phase 7 (Integration)
  Task 7.1 (depends on Phase 5)
  Task 7.2 (depends on Phase 3 + Phase 5)
  Task 7.3 (depends on Phase 2)
  Task 7.4 (depends on ALL above)
```

**Critical path:** 1.1 → 1.2 → 1.3 → 2.1 → 2.2 → 3.1 → 3.2 → 3.3
→ 3.5 → 5.4 → 7.1 → 7.2 → 7.4

**Parallelizable:** Phase 4 can start after Phase 1; Phase 6 can
overlap with Phase 5; Tasks 5.1/5.2/5.3 are independent.
