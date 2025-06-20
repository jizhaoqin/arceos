#![allow(dead_code)]

use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::{Stream, StreamExt};

use axhal::console::RECEIVE_BUFFER;
use axhal::notify_async_irq_handler::{UART_RECEIVE_QUEUE, WAKER};

/// 异步uart中断处理函数
///
/// - 这里是async和正常函数的分离点
/// - 直接调用这个函数的是异步运行时, 这个执行器在空闲时应该yield, 这里是因为没有其他任务所以一直占用CPU
/// - 需要注意的是, 这个async函数实际上永远只会返回pending, 因为while循环没有break语句, 是一个无限循环,
///   但只有uart中断发生时这个函数才会被poll
/// - poll这个future的时候传入了context
/// - 执行器初始化的时候加入此future时, 会poll一次, 因为这不是由中断引起的
/// - 在这首次poll的时候, 会由执行器新建并传入包含执行器信息的waker, 之后这个future就一直在pending
pub async fn async_uart_handler() {
    let mut uart_stream = UartStream::new();
    while let Some(byte) = uart_stream.next().await {
        let mut buffer = RECEIVE_BUFFER.lock();
        // 这里暂时限制缓冲区长度
        if buffer.len() < 1024 {
            buffer.push_back(byte);
        }
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
