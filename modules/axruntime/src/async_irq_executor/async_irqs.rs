#![allow(dead_code)]

//! modules/axhal/src/async_irq.rs

extern crate alloc;

use super::Task;
use conquer_once::spin::OnceCell;
use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::{Stream, StreamExt};
use futures_util::task::AtomicWaker;

static UART_RECEIVE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

// #[cfg(feature = "multitask")]
pub fn init_async_irq_system() {
    use alloc::string::ToString;

    // 创建内核线程, 运行异步执行器, 理论上永远不会退出
    axtask::spawn_raw(
        move || {
            use super::executor::Executor;
            let mut executor = Executor::new();
            executor.spawn(Task::new(example_task()));
            executor.spawn(Task::new(async_uart_handler()));
            executor.run();
        },
        "async-executor".to_string(),
        0x4000, // 16KB stack
    );
}

/// 由axhal::...::uart_irq_handler()调用
///
/// - 这里是中断上下文, 所以must not block or allocate
/// - 这里把uart_data加入异步流[`Stream`]
/// - 最后唤醒异步执行器尝试进行处理
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

async fn async_number() -> u32 {
    42
}

async fn example_task() {
    let number = async_number().await;
    axlog::ax_print!("async number: {}", number);
}

/// 这里是async和正常函数的分离点
///
/// - 再往上调用这个函数的是非async但是阻塞的执行器, 这个执行器在空闲时应该yield, 这里是因为没有其他任务所以一直占用CPU
/// - 需要注意的是, 这个async函数实际上永远只会返回pending, 因为我们很容易看到while循环没有break语句, 也就是一个无限循环,
///   但只有uart中断发生时这个函数才会被poll
/// - poll这个future的时候传入了context
/// - 执行器初始化的时候加入此future时, 会poll一次, 因为这不是由中断引起的
/// - 在这首次poll的时候, 会由执行器新建并传入包含执行器信息的waker, 之后这个future就一直在pending
pub async fn async_uart_handler() {
    let mut uart_stream = UartStream::new();

    // println!("poll print_key_presses()");

    // .await这里相当于poll了scan_codes.next()这个future, 也传入了context信息
    while let Some(_char) = uart_stream.next().await {
        axlog::ax_print!("async uart handler triggered");
    }
}

/// 字段 _private 的目的是防止從模塊外部構造結構(可以去除)
#[derive(Default)]
struct UartStream {
    _private: (),
}

impl UartStream {
    pub fn new() -> Self {
        UART_RECEIVE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        UartStream { _private: () }
    }
}

/// Future trait 只是對單個異步值進行抽象，並且期望 poll 方法在返回 Poll::Ready 後不再被調用
/// 然而，我們的串口字符隊列包含多個異步值，所以保持對它的輪詢是可以的
/// `Stream` trait适用于產生多個異步值的類型
/// - Future poll()      -> Poll<Output>
/// - Stream poll_next() -> Poll<Option<Item>>
impl Stream for UartStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let queue = UART_RECEIVE_QUEUE
            .try_get()
            .expect("UART_RECEIVE_QUEUE not initialized");

        if let Some(byte) = queue.pop() {
            return Poll::Ready(Some(byte));
        }

        // 这里注册的`Waker`实际上来自于TaskWaker::new_waker(task_id, task_queue.clone())
        WAKER.register(cx.waker());
        match queue.pop() {
            Some(byte) => {
                WAKER.take();
                Poll::Ready(Some(byte))
            }
            None => Poll::Pending,
        }
    }
}
