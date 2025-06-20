#![allow(dead_code)]

//! modules/axhal/src/async_irq.rs

extern crate alloc;

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use futures_util::task::AtomicWaker;

pub static UART_RECEIVE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
pub static WAKER: AtomicWaker = AtomicWaker::new();

/// 由axhal::...::uart_irq_handler()调用
///
/// - 这里是中断上下文, 所以must not block or allocate
/// - 这里把uart_data加入异步流[`Stream`]
/// - 最后唤醒异步执行器尝试进行处理
/// - 需要保留在axhal以在axhal注册处理函数, 处理函数调用此函数, 避免交叉依赖
pub fn add_uart_data(byte: u8) {
    if let Ok(queue) = UART_RECEIVE_QUEUE.try_get() {
        if queue.push(byte).is_err() {
            axlog::ax_println!("WARNING: UART_RECEIVE_QUEUE full; dropping keyboard input");
        } else {
            // 中断处理的最后执行唤醒操作
            // 具体逻辑是从virtual table调用了我们通过impl Wake for TaskWaker传入的函数
            // 效果是将print_key_presses()的id重新加入所属执行器的task_queue
            // 之后执行器将poll print_key_presses(), 进入函数体打印字符
            WAKER.wake();
        }
    } else {
        axlog::ax_println!("WARNING: UART_RECEIVE_QUEUE uninitialized");
    }
}
