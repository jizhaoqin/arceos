mod apic;
mod boot;
mod uart16550;

pub mod mem;
pub mod misc;
pub mod time;

#[cfg(feature = "smp")]
pub mod mp;

#[cfg(feature = "irq")]
pub mod irq {
    pub use super::apic::*;
}

pub mod console {
    // 将uart16550的东西重导出为console::*
    pub use super::uart16550::*;
}

unsafe extern "C" {
    fn rust_main(cpu_id: usize, dtb: usize) -> !;
    #[cfg(feature = "smp")]
    fn rust_main_secondary(cpu_id: usize) -> !;
}

fn current_cpu_id() -> usize {
    match raw_cpuid::CpuId::new().get_feature_info() {
        Some(finfo) => finfo.initial_local_apic_id() as usize,
        None => 0,
    }
}

unsafe extern "C" fn rust_entry(magic: usize, _mbi: usize) {
    // TODO: handle multiboot info
    if magic == self::boot::MULTIBOOT_BOOTLOADER_MAGIC {
        crate::mem::clear_bss();
        crate::cpu::init_primary(current_cpu_id());
        self::uart16550::init();
        self::time::init_early();
        rust_main(current_cpu_id(), 0);
    }
}

#[allow(unused_variables)]
unsafe extern "C" fn rust_entry_secondary(magic: usize) {
    #[cfg(feature = "smp")]
    if magic == self::boot::MULTIBOOT_BOOTLOADER_MAGIC {
        crate::cpu::init_secondary(current_cpu_id());
        rust_main_secondary(current_cpu_id());
    }
}

/// Initializes the platform devices for the primary CPU.
/// 
/// - from axruntime/lib.rs: 151
/// - 这里初始化一些设备, 这里只有中断控制器和计时器
/// - 中断控制器虽然初始化了但没有启用, 需要irq条件编译启用才能注册自定义中断处理函数
/// - 启用irq时自定义的timer处理函数似乎把time::init_primary()中的处理给覆盖了
pub fn platform_init() {
    self::apic::init_primary();
    self::time::init_primary();
}

/// Initializes the platform devices for secondary CPUs.
#[cfg(feature = "smp")]
pub fn platform_init_secondary() {
    self::apic::init_secondary();
    self::time::init_secondary();
}
