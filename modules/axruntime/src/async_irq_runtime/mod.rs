pub mod async_uart_irq;
pub mod executor;

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::ToString;
use core::fmt::Debug;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};
use core::{future::Future, pin::Pin};

pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Default)]
struct TaskId(u64);

impl TaskId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// 中断处理异步运行时
/// 
/// - 创建线程, 运行异步执行器, 理论上永远不会退出
pub fn init_async_irq_runtime() {
    axtask::spawn_raw(
        move || {
            let mut executor = executor::Executor::new();
            executor.spawn(Task::new(async_uart_irq::async_uart_handler()));
            executor.run();
        },
        "async-irq-runtime".to_string(),
        0x4000, // 16KB stack
    );
}

async fn async_number() -> u32 {
    10000
}

/// 示例异步程序
async fn example_task() {
    let number = async_number().await;
    axlog::ax_print!("async number: {}", number);
}
