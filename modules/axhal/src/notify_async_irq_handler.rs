extern crate alloc;

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use futures_util::task::AtomicWaker;

pub static UART_RECEIVE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
pub static WAKER: AtomicWaker = AtomicWaker::new();

/// 添加数据进入队列并通知异步处理
/// 
/// - 由中断上下文uart_irq_handler()调用, 所以不能阻塞需要尽快返回, 不能处理复杂逻辑
/// - 这里把uart_data加入异步流[`Stream`]
/// - 最后唤醒异步执行器尝试进行处理
/// - 需要保留在axhal以在axhal注册处理函数, 处理函数调用此函数, 避免交叉依赖
pub fn notify_async_uart_irq_handler(byte: u8) {
    if let Ok(queue) = UART_RECEIVE_QUEUE.try_get() {
        if queue.push(byte).is_err() {
            axlog::ax_println!("WARNING: UART_RECEIVE_QUEUE full");
        } else {
            // 中断处理最后执行唤醒操作
            // 具体逻辑是从virtual table调用通过impl Wake for TaskWaker传入的函数
            // 效果是将async_uart_handler()的id重新加入所属执行器的task_queue
            // 之后执行器将poll sync_uart_handler(), 执行后续逻辑
            WAKER.wake();
        }
    } else {
        axlog::ax_println!("WARNING: UART_RECEIVE_QUEUE uninitialized");
    }
}
