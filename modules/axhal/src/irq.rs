//! Interrupt management.

use handler_table::HandlerTable;

use crate::platform::irq::{MAX_IRQ_COUNT, dispatch_irq};
use crate::trap::{IRQ, register_trap_handler};

// from axruntime::init_interrupt()
pub use crate::platform::irq::{register_handler, set_enable};

// pub use crate::platform::console::uart_irq_handler;

/// The type if an IRQ handler.
///
/// - 别名: pub type Handler = fn();
/// - 目前只支持这种类型的中断处理函数
pub type IrqHandler = handler_table::Handler;

// 中断向量表, 在x86_64里是中断描述符表似乎
static IRQ_HANDLER_TABLE: HandlerTable<MAX_IRQ_COUNT> = HandlerTable::new();

/// Platform-independent IRQ dispatching.
#[allow(dead_code)]
pub(crate) fn dispatch_irq_common(irq_num: usize) {
    trace!("IRQ {}", irq_num);
    if !IRQ_HANDLER_TABLE.handle(irq_num) {
        warn!("Unhandled IRQ {}", irq_num);
    }
}

/// Platform-independent IRQ handler registration.
///
/// It also enables the IRQ if the registration succeeds. It returns `false` if
/// the registration failed.
///
/// - from x86_64::irq::register_handler(vector, handler)
#[allow(dead_code)]
pub(crate) fn register_handler_common(irq_num: usize, handler: IrqHandler) -> bool {
    // 在架构无关的IRQ_HANDLER_TABLE处注册处理函数后, 之后遇到中断就可以查表调用函数处理了
    if irq_num < MAX_IRQ_COUNT && IRQ_HANDLER_TABLE.register_handler(irq_num, handler) {
        // 看看都注册了哪些handler
        axlog::ax_println!("irq number registerd: {irq_num}");
        
        axlog::ax_println!("register_handler_common: {irq_num}");
        // 启用对应编号irq_num的中断
        set_enable(irq_num, true);
        // TODO: 没有到达这里在上面失效了
        return true;
    }
    warn!("register handler for IRQ {} failed", irq_num);
    false
}

/// 架构无关
///
/// - 条件编译调用对应架构的dispatch_irq(irq_nunm)
#[register_trap_handler(IRQ)]
fn handler_irq(irq_num: usize) -> bool {
    let guard = kernel_guard::NoPreempt::new();
    dispatch_irq(irq_num);
    drop(guard); // rescheduling may occur when preemption is re-enabled.
    true
}
