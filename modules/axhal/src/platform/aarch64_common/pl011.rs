//! PL011 UART.

extern crate alloc;

use alloc::collections::VecDeque;
use arm_pl011::Pl011Uart;
use kspin::SpinNoIrq;
use memory_addr::PhysAddr;

use crate::mem::phys_to_virt;

const UART_BASE: PhysAddr = pa!(axconfig::devices::UART_PADDR);

static UART: SpinNoIrq<Pl011Uart> =
    SpinNoIrq::new(Pl011Uart::new(phys_to_virt(UART_BASE).as_mut_ptr()));

// 输入缓冲区
pub static RECEIVE_BUFFER: SpinNoIrq<VecDeque<u8>> = SpinNoIrq::new(VecDeque::new());

/// Writes a byte to the console.
pub fn putchar(c: u8) {
    let mut uart = UART.lock();
    match c {
        b'\n' => {
            uart.putchar(b'\r');
            uart.putchar(b'\n');
        }
        c => uart.putchar(c),
    }
}

/// Reads a byte from the console, or returns [`None`] if no input is available.
///
/// - 成功基于中断实现获取串口输入
fn getchar() -> Option<u8> {
    // UART.lock().getchar()
    RECEIVE_BUFFER.lock().pop_front()
}

/// Write a slice of bytes to the console.
pub fn write_bytes(bytes: &[u8]) {
    for c in bytes {
        putchar(*c);
    }
}

/// Reads bytes from the console into the given mutable slice.
/// Returns the number of bytes read.
///
/// - 调用getchar()
pub fn read_bytes(bytes: &mut [u8]) -> usize {
    let mut read_len = 0;
    while read_len < bytes.len() {
        if let Some(c) = getchar() {
            bytes[read_len] = c;
        } else {
            break;
        }
        read_len += 1;
    }
    read_len
}

/// Initialize the UART
pub fn init_early() {
    UART.lock().init();
}

/// 注册uart中断处理函数
///
/// - register_handler(vector, handler)会自动启用中断
pub fn init() {
    #[cfg(feature = "irq")]
    crate::irq::register_handler(super::gic::UART_IRQ_NUM, uart_irq_handler);
}

/// UART IRQ Handler
///
/// - 此函数注册在中断向量表中
/// - 作用是把从串口设备读到的byte传递给notify_async_uart_irq_handler(byte)
/// - 然后立即返回
pub fn uart_irq_handler() {
    let is_receive_interrupt = UART.lock().is_receive_interrupt();
    UART.lock().ack_interrupts();
    if is_receive_interrupt {
        // info!("uart irq received");
        while let Some(byte) = UART.lock().getchar() {
            crate::notify_async_irq_handler::notify_async_uart_irq_handler(byte);
        }
    }
}
