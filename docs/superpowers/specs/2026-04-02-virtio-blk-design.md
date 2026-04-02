# VirtIO Block Device Design for Machina

## Goal

Add VirtIO MMIO block device support to Machina so that rCore ch6-ch8 (file system chapters) can run. The implementation must be compatible with QEMU's virt machine layout and accept QEMU-compatible `-drive` CLI arguments.

## Architecture

```
+------------------------------------------+
|  hw/virtio/ crate (machina-hw-virtio)    |
|                                          |
|  +----------------+  +----------------+  |
|  | VirtioMmio     |  | VirtioBlk      |  |
|  | (MMIO regs,    |  | (block config, |  |
|  |  queue mgmt,   |  |  req parse,    |  |
|  |  MmioOps impl) |  |  mmap backend) |  |
|  +-------+--------+  +-------+--------+  |
|          |  VirtioDevice      |           |
|          |  trait              |           |
|          +--------------------+           |
+------------------------------------------+
           |                  |
  AddressSpace           PLIC IRQ 1
  0x10001000
```

### Components

**VirtioMmio** — Generic VirtIO MMIO transport. Implements `MmioOps`. Handles standard register reads/writes (MAGIC, VERSION, DEVICE_ID, STATUS, QUEUE_*, INTERRUPT_*, CONFIG space). Delegates device-specific operations to a `VirtioDevice` backend via trait.

**VirtioDevice trait** — Backend interface. Methods: `device_type()`, `features()`, `ack_features()`, `config_read/write()`, `handle_queue()`, `reset()`. Decouples transport from device logic. Extensible to virtio-net, virtio-gpu, etc.

**VirtioBlk** — Implements `VirtioDevice` for block storage. Parses block request descriptor chains (header → data → status). Reads/writes via mmap'd raw file backing. Reports capacity in config space.

**VirtQueue** — Per-queue state: descriptor table address, available ring address, used ring address, queue size, last_avail_idx cursor. Provides methods to walk descriptor chains and update the used ring.

**GuestMem trait** — Abstraction for accessing guest physical memory. Implemented via RAM pointer arithmetic. Used by VirtQueue and VirtioBlk to read descriptors, transfer data, and update rings.

## MMIO Register Map

Address 0x10001000, size 0x1000 (4 KB).

| Offset | Name | R/W | Value |
|--------|------|-----|-------|
| 0x000 | MAGIC_VALUE | R | 0x74726976 |
| 0x004 | VERSION | R | 2 (modern) or 1 (legacy) |
| 0x008 | DEVICE_ID | R | 2 (block) |
| 0x00c | VENDOR_ID | R | 0x554D4551 |
| 0x010 | DEVICE_FEATURES | R | features[sel*32 +: 32] |
| 0x014 | DEVICE_FEATURES_SEL | W | 0 or 1 |
| 0x020 | DRIVER_FEATURES | W | accepted features |
| 0x024 | DRIVER_FEATURES_SEL | W | 0 or 1 |
| 0x030 | QUEUE_SEL | W | queue index |
| 0x034 | QUEUE_NUM_MAX | R | 256 |
| 0x038 | QUEUE_NUM | W | actual queue size |
| 0x044 | QUEUE_READY | RW | queue enabled |
| 0x050 | QUEUE_NOTIFY | W | triggers handle_queue |
| 0x060 | INTERRUPT_STATUS | R | bit 0=vring, bit 1=config |
| 0x064 | INTERRUPT_ACK | W | clears interrupt bits |
| 0x070 | STATUS | RW | device status (0=reset) |
| 0x080 | QUEUE_DESC_LOW | W | desc table addr low |
| 0x084 | QUEUE_DESC_HIGH | W | desc table addr high |
| 0x090 | QUEUE_AVAIL_LOW | W | avail ring addr low |
| 0x094 | QUEUE_AVAIL_HIGH | W | avail ring addr high |
| 0x0a0 | QUEUE_USED_LOW | W | used ring addr low |
| 0x0a4 | QUEUE_USED_HIGH | W | used ring addr high |
| 0x0fc | CONFIG_GENERATION | R | 0 (no live changes) |
| 0x100+ | CONFIG | RW | device-specific config |

Legacy (v1) support: VERSION returns 1, QUEUE_PFN at 0x040, QUEUE_ALIGN at 0x03c, GUEST_PAGE_SIZE at 0x028. These are only needed if virtio-drivers uses v1 flow.

## VirtQueue Layout (in guest memory)

```
Descriptor Table (queue_size * 16 bytes):
  struct { u64 addr, u32 len, u16 flags, u16 next }

Available Ring (6 + queue_size * 2 bytes):
  struct { u16 flags, u16 idx, u16 ring[N] }

Used Ring (6 + queue_size * 8 bytes):
  struct { u16 flags, u16 idx, { u32 id, u32 len } ring[N] }
```

Descriptor flags: NEXT=1, WRITE=2, INDIRECT=4.

## Block Request Format

Each block request is a descriptor chain of 3 segments:

1. **Header** (device-readable, 16 bytes): `{ u32 type, u32 reserved, u64 sector }`. type: 0=IN(read), 1=OUT(write), 4=FLUSH.
2. **Data** (read: device-writable / write: device-readable): sector_count * 512 bytes.
3. **Status** (device-writable, 1 byte): 0=OK, 1=IOERR, 2=UNSUP.

## Storage Backend

Raw file mmap. The file specified by `-drive file=<path>` is opened and mmap'd into host memory. Guest read/write operations translate to memcpy against the mmap'd region. File size determines device capacity (in 512-byte sectors).

Config space layout (at MMIO offset 0x100):
- offset 0: u64 capacity (number of 512-byte sectors)
- offset 8: u32 size_max (0 = no limit)
- offset 12: u32 seg_max (0 = no limit)
- offset 16: geometry (zeros)
- offset 20: u32 blk_size (512)

## Request Processing

Synchronous: when the guest writes QUEUE_NOTIFY, the MMIO write handler immediately:

1. Reads available ring idx from guest memory
2. For each new entry (last_avail_idx..avail_idx):
   a. Read descriptor chain: header → data → status
   b. Parse header: type, sector
   c. Execute I/O: memcpy between mmap region and data buffer
   d. Write status byte (0=OK)
   e. Append to used ring
3. Update used ring idx in guest memory
4. Set interrupt_status |= 1
5. Assert IRQ line (irq.set(true))

## IRQ Routing

```
VirtioMmio ---> PLIC source 1 ---> PLIC context (S-mode)
                                        |
                                   CPU mip.SEIP
```

PLIC source IRQ 1 (VIRTIO_IRQ in QEMU virt.h). The guest acknowledges by writing INTERRUPT_ACK, which clears interrupt_status and deasserts the IRQ line.

## CLI Integration

Parse QEMU-compatible arguments:

```
-drive file=<path>,if=none,format=raw,id=<id>
-device virtio-blk-device,drive=<id>
```

Simplified: extract `file=<path>` from `-drive`. The `-device` argument selects device type. If no `-drive` is specified, no VirtIO device is created.

Add to CliArgs:
```rust
struct CliArgs {
    // ... existing fields ...
    drive: Option<PathBuf>,  // -drive file= path
}
```

## FDT Integration

Add a VirtIO MMIO node to the FDT:

```
virtio_mmio@10001000 {
    compatible = "virtio,mmio";
    reg = <0x0 0x10001000 0x0 0x1000>;
    interrupts = <0x1>;
    interrupt-parent = <&plic>;
};
```

rCore ch6 hardcodes 0x10001000 and does not read FDT, but the node ensures compatibility with other guest OSes.

## Crate Structure

```
hw/virtio/
  Cargo.toml          # machina-hw-virtio
  src/
    lib.rs            # pub mod mmio, block, queue
    mmio.rs           # VirtioMmio: MmioOps impl
    block.rs          # VirtioBlk: VirtioDevice impl
    queue.rs          # VirtQueue, descriptor chain walker
```

Dependencies: machina-hw-core (IrqLine), machina-memory (for GuestMem).

## Testing Strategy

1. Unit tests for VirtQueue descriptor chain parsing (mock guest memory)
2. Unit tests for VirtioBlk config space reads
3. Integration test: create VirtioMmio + VirtioBlk, simulate device initialization sequence (write STATUS, negotiate features, setup queue)
4. Full-system test: run ch6 with fs.img, verify shell prompt appears

## Scope Boundaries

**In scope**: VirtIO MMIO transport (v1+v2), block device, single queue, raw file backend, `-drive` CLI, FDT node, PLIC IRQ wiring.

**Out of scope**: virtio-net, virtio-gpu, multi-queue, indirect descriptors, event index, virtio-pci transport, IOMMU.
