#![allow(dead_code)]

use core::{cell::SyncUnsafeCell, mem::MaybeUninit};

use kspin::SpinNoIrq;
use lazyinit::LazyInit;
use memory_addr::PhysAddr;
use x2apic::ioapic::IoApic;
use x2apic::lapic::{LocalApic, LocalApicBuilder, xapic_base};
use x86_64::instructions::port::Port;

use self::vectors::*;
use crate::mem::phys_to_virt;

pub(super) mod vectors {
    pub const APIC_TIMER_VECTOR: u8 = 0xf0;
    pub const APIC_SPURIOUS_VECTOR: u8 = 0xf1;
    pub const APIC_ERROR_VECTOR: u8 = 0xf2;
    // 键盘中断vector
    // pub const APIC_KEYBOARD_VEVTOR: u8 = 0x21;
}

/// The maximum number of IRQs.
pub const MAX_IRQ_COUNT: usize = 256;

/// The timer IRQ number.
pub const TIMER_IRQ_NUM: usize = APIC_TIMER_VECTOR as usize;

// 键盘中断vector
pub const KEYBOARD_IRQ_NUM: usize = 0x21;

const IO_APIC_BASE: PhysAddr = pa!(0xFEC0_0000);

/// 属于CPU核心
///
/// - 每个核心只有有一个
/// - 主要功能是处理CPU接受到的中断, 可能是处理器间中断(IPI), 也可能来自IoApic
static LOCAL_APIC: SyncUnsafeCell<MaybeUninit<LocalApic>> =
    SyncUnsafeCell::new(MaybeUninit::uninit());

static mut IS_X2APIC: bool = false;

/// 全局静态IO_APIC实例, 不属于CPU核心
///
/// - 可以有多个, 这里只设置了1个
/// - 主要负责收集和路由CPU外部中断, 决定目标处理CPU(如果有多个核心), 发送给目标CPU的LocalApic
static IO_APIC: LazyInit<SpinNoIrq<IoApic>> = LazyInit::new();

/// Enables or disables the given IRQ.
///
/// - from register_handler_common(irq_num, handler)
/// - 启用对应编号irq_num的中断
#[cfg(feature = "irq")]
pub fn set_enable(vector: usize, enabled: bool) {
    // should not affect LAPIC interrupts
    // TIMER iqr_num = 0xf0 = 15*16 = 240
    // 似乎其他自己注册的中断编号应该比240小
    if vector < APIC_TIMER_VECTOR as _ {
        unsafe {
            // 使用IO_APIC实例启用或关闭对应irq
            if enabled {
                axlog::ax_println!("enable {vector}");
                // 到这里退出了
                IO_APIC.lock().enable_irq(vector as u8);
            } else {
                axlog::ax_println!("disable {vector}");
                IO_APIC.lock().disable_irq(vector as u8);
            }
        }
    }
}

/// Registers an IRQ handler for the given IRQ.
///
/// It also enables the IRQ if the registration succeeds. It returns `false` if
/// the registration failed.
///
/// - from axruntime::init_interrupt()
/// - x86_64这里不需要额外的处理直接调用register_handler_common(irq_num, handler), riscv就需要
/// - 不启用irq就不能注册中断处理函数
#[cfg(feature = "irq")]
pub fn register_handler(vector: usize, handler: crate::irq::IrqHandler) -> bool {
    axlog::ax_println!("--------------x86_64 irq register handler here---------------------");

    crate::irq::register_handler_common(vector, handler)
}

/// Dispatches the IRQ. 中断处理转接, 根据中断编号vector查表调用中断处理函数
///
/// - This function is called by the common interrupt handler. It looks
/// up in the IRQ handler table and calls the corresponding handler. If
/// necessary, it also acknowledges the interrupt controller after handling.
#[cfg(feature = "irq")]
pub fn dispatch_irq(vector: usize) {
    crate::irq::dispatch_irq_common(vector);
    unsafe { local_apic().end_of_interrupt() };
}

pub(super) fn local_apic<'a>() -> &'a mut LocalApic {
    // It's safe as `LOCAL_APIC` is initialized in `init_primary`.
    unsafe { LOCAL_APIC.get().as_mut().unwrap().assume_init_mut() }
}

pub(super) fn raw_apic_id(id_u8: u8) -> u32 {
    if unsafe { IS_X2APIC } {
        id_u8 as u32
    } else {
        (id_u8 as u32) << 24
    }
}

fn cpu_has_x2apic() -> bool {
    match raw_cpuid::CpuId::new().get_feature_info() {
        Some(finfo) => finfo.has_x2apic(),
        None => false,
    }
}

/// x2APIC设备初始化
///
/// - APIC: 高级可编程中断控制器
/// - x2APIC 是 x86 架构中高级可编程中断控制器 (APIC) 的扩展模式，相比传统 APIC 和 xAPIC 提供了显著改进
pub(super) fn init_primary() {
    info!("Initialize Local APIC...");

    unsafe {
        // Disable 8259A interrupt controllers
        Port::<u8>::new(0x21).write(0xff);
        Port::<u8>::new(0xA1).write(0xff);
    }

    // LocalApic初始化准备, 要求这些field定义
    let mut builder = LocalApicBuilder::new();
    builder
        .timer_vector(APIC_TIMER_VECTOR as _)
        .error_vector(APIC_ERROR_VECTOR as _)
        .spurious_vector(APIC_SPURIOUS_VECTOR as _);

    if cpu_has_x2apic() {
        info!("Using x2APIC.");
        unsafe { IS_X2APIC = true };
    } else {
        info!("Using xAPIC.");
        let base_vaddr = phys_to_virt(pa!(unsafe { xapic_base() } as usize));
        builder.set_xapic_base(base_vaddr.as_usize() as u64);
    }

    // 启用LocalApic并转移到全局静态变量中, 不需要irq条件
    let mut lapic = builder.build().unwrap();
    unsafe {
        lapic.enable();
        LOCAL_APIC.get().as_mut().unwrap().write(lapic);
    }

    info!("Initialize IO APIC...");
    let io_apic = unsafe { IoApic::new(phys_to_virt(IO_APIC_BASE).as_usize() as u64) };
    IO_APIC.init_once(SpinNoIrq::new(io_apic));
}

#[cfg(feature = "smp")]
pub(super) fn init_secondary() {
    unsafe { local_apic().enable() };
}
