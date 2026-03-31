// machina: QEMU-style full-system emulator entry point.

use std::env;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use machina_accel::exec::ExecEnv;
use machina_accel::x86_64::emitter::MmioConfig;
use machina_accel::X86_64CodeGen;
use machina_core::machine::{Machine, MachineOpts};
use machina_hw_riscv::ref_machine::RefMachine;
use machina_system::cpus::{
    machina_mem_read, machina_mem_write, FullSystemCpu,
};
use machina_system::CpuManager;

fn usage() {
    eprintln!("Usage: machina [options]");
    eprintln!("Options:");
    eprintln!(
        "  -M machine    Machine type \
         (default: riscv64-ref)"
    );
    eprintln!("  -m size       RAM size in MiB (default: 128)");
    eprintln!("  -bios path    BIOS/firmware binary");
    eprintln!("  -kernel path  Kernel binary");
    eprintln!("  -nographic    Disable graphical output");
    eprintln!("  -h, --help    Show this help");
}

struct CliArgs {
    machine: String,
    ram_mib: u64,
    bios: Option<PathBuf>,
    kernel: Option<PathBuf>,
    #[allow(dead_code)]
    nographic: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            machine: "riscv64-ref".to_string(),
            ram_mib: 128,
            bios: None,
            kernel: None,
            nographic: false,
        }
    }
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cli = CliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-M" | "-machine" => {
                i += 1;
                cli.machine =
                    args.get(i).ok_or("-M requires argument")?.clone();
            }
            "-m" => {
                i += 1;
                let s = args.get(i).ok_or("-m requires argument")?;
                cli.ram_mib = s
                    .trim_end_matches('M')
                    .parse::<u64>()
                    .map_err(|e| format!("-m: {}", e))?;
            }
            "-bios" => {
                i += 1;
                cli.bios = Some(
                    args.get(i)
                        .ok_or("-bios requires argument")?
                        .clone()
                        .into(),
                );
            }
            "-kernel" => {
                i += 1;
                cli.kernel = Some(
                    args.get(i)
                        .ok_or("-kernel requires argument")?
                        .clone()
                        .into(),
                );
            }
            "-nographic" => {
                cli.nographic = true;
            }
            "-h" | "--help" => {
                usage();
                process::exit(0);
            }
            other => {
                return Err(format!("Unknown option: {}", other));
            }
        }
        i += 1;
    }
    Ok(cli)
}

fn install_crash_handler() {
    unsafe {
        libc::signal(libc::SIGSEGV, crash_handler as *const () as usize);
    }
}

extern "C" fn crash_handler(_sig: libc::c_int) {
    let pc = machina_system::cpus::LAST_TB_PC.load(Ordering::Relaxed);
    eprintln!(
        "\nmachina: SIGSEGV in JIT code, \
         last TB pc={:#x}",
        pc,
    );
    std::process::exit(139);
}

fn main() {
    install_crash_handler();
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("machina: {}", e);
            usage();
            process::exit(1);
        }
    };

    if cli.machine == "?" {
        eprintln!("Available machines:");
        eprintln!("  riscv64-ref    RISC-V reference machine");
        process::exit(0);
    }
    if cli.machine != "riscv64-ref" {
        eprintln!("machina: unknown machine: {}", cli.machine);
        process::exit(1);
    }

    let mut machine = RefMachine::new();

    let ram_size = cli.ram_mib * 1024 * 1024;
    let opts = MachineOpts {
        ram_size,
        cpu_count: 1,
        kernel: cli.kernel.clone(),
        bios: cli.bios.clone(),
        append: None,
        nographic: cli.nographic,
    };

    if let Err(e) = machine.init(&opts) {
        eprintln!("machina: init failed: {}", e);
        process::exit(1);
    }

    if let Err(e) = machine.boot() {
        eprintln!("machina: boot failed: {}", e);
        process::exit(1);
    }

    eprintln!(
        "machina: {} booted, {} MiB RAM",
        machine.name(),
        cli.ram_mib
    );

    // JIT backend with MMIO helpers.
    let mut backend = X86_64CodeGen::new();
    backend.mmio = Some(MmioConfig {
        ram_base: 0x8000_0000,
        ram_end: 0x8000_0000 + ram_size,
        load_helper: machina_mem_read as *const () as u64,
        store_helper: machina_mem_write as *const () as u64,
    });
    let env = ExecEnv::new(backend);
    let shared = env.shared.clone();

    // Take CPU0 from machine for execution.
    let shared_mip = machine.shared_mip();
    let cpu0 = machine.take_cpu(0).expect("cpu0 must exist after boot");

    let ram_ptr = machine.ram_ptr();
    let wfi_waker = machine.wfi_waker();
    let as_ptr = machine.address_space()
        as *const machina_memory::address_space::AddressSpace;

    let mut cpu_mgr = CpuManager::new();
    cpu_mgr.set_wfi_waker(wfi_waker.clone());

    let stop_flag = cpu_mgr.running_flag();
    let fs_cpu = unsafe {
        FullSystemCpu::new(
            cpu0,
            ram_ptr,
            ram_size,
            shared_mip,
            wfi_waker.clone(),
            as_ptr,
            Arc::clone(&stop_flag),
        )
    };
    cpu_mgr.add_cpu(fs_cpu);

    // Wire SiFive Test to execution control.
    use machina_hw_riscv::sifive_test::ShutdownReason;
    let shutdown_reason: Arc<std::sync::Mutex<Option<ShutdownReason>>> =
        Arc::new(std::sync::Mutex::new(None));
    {
        let reason_slot = Arc::clone(&shutdown_reason);
        let flag = Arc::clone(&stop_flag);
        let wk = wfi_waker;
        machine
            .sifive_test()
            .set_shutdown_handler(Box::new(move |reason| {
                *reason_slot.lock().unwrap() = Some(reason);
                flag.store(false, Ordering::SeqCst);
                wk.stop();
            }));
    }

    eprintln!("machina: entering execution loop");

    let _exit = unsafe { cpu_mgr.run(&shared) };

    let code = match *shutdown_reason.lock().unwrap() {
        Some(ShutdownReason::Pass) => {
            eprintln!("machina: shutdown (pass)");
            0
        }
        Some(ShutdownReason::Reset) => {
            eprintln!("machina: reset requested");
            3
        }
        Some(ShutdownReason::Fail(c)) => {
            eprintln!("machina: fail (code {:#x})", c);
            1
        }
        None => {
            eprintln!("machina: execution exited");
            0
        }
    };
    process::exit(code);
}
