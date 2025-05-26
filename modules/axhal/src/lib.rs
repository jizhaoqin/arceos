//! [ArceOS] hardware abstraction layer, provides unified APIs for
//! platform-specific operations.
//!
//! It does the bootstrapping and initialization process for the specified
//! platform, and provides useful operations on the hardware.
//!
//! Currently supported platforms (specify by cargo features):
//!
//! - `x86-pc`: Standard PC with x86_64 ISA.
//! - `riscv64-qemu-virt`: QEMU virt machine with RISC-V ISA.
//! - `aarch64-qemu-virt`: QEMU virt machine with AArch64 ISA.
//! - `aarch64-raspi`: Raspberry Pi with AArch64 ISA.
//! - `dummy`: If none of the above platform is selected, the dummy platform
//!   will be used. In this platform, most of the operations are no-op or
//!   `unimplemented!()`. This platform is mainly used for [cargo test].
//!
//! # Cargo Features
//!
//! - `smp`: Enable SMP (symmetric multiprocessing) support.
//! - `fp_simd`: Enable floating-point and SIMD support.
//! - `paging`: Enable page table manipulation.
//! - `irq`: Enable interrupt handling support.
//!
//! [ArceOS]: https://github.com/arceos-org/arceos
//! [cargo test]: https://doc.rust-lang.org/cargo/guide/tests.html

#![no_std]
#![feature(doc_auto_cfg)]

#[allow(unused_imports)]
#[macro_use]
extern crate log;

#[allow(unused_imports)]
#[macro_use]
extern crate memory_addr;

pub mod cpu;
pub mod mem;
pub mod time;

#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "irq")]
pub mod irq;

#[cfg(feature = "paging")]
pub mod paging;

/// Console input and output.
pub mod console {
    pub use axplat::console::{read_bytes, write_bytes};
}

/// CPU power management.
pub mod power {
    #[cfg(feature = "smp")]
    pub use axplat::power::cpu_boot;
    pub use axplat::power::system_off;
}

/// Trap handling.
pub mod trap {
    pub use axcpu::trap::{IRQ, PAGE_FAULT};
    pub use axcpu::trap::{PageFaultFlags, register_trap_handler};
}

/// CPU register states for context switching.
///
/// There are three types of context:
///
/// - [`TaskContext`][axcpu::TaskContext]: The context of a task.
/// - [`TrapFrame`][axcpu::TrapFrame]: The context of an interrupt or an exception.
/// - [`UspaceContext`][axcpu::uspace::UspaceContext]: The context for user/kernel mode switching.
pub mod context {
    #[cfg(feature = "uspace")]
    pub use axcpu::uspace::UspaceContext;
    pub use axcpu::{TaskContext, TrapFrame};
}

pub use axcpu::asm;
pub use axplat::init::{platform_init, platform_init_secondary};

/// Initializes HAL data structures for the primary core.
pub fn init(cpu_id: usize, _dtb: usize) {
    self::cpu::init_primary(cpu_id);
    self::mem::init();
}

/// Initializes HAL data structures for secondary cores.
pub fn init_secondary(cpu_id: usize) {
    self::cpu::init_secondary(cpu_id);
}
