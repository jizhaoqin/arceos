//! Uart 16550.

// 需要这个启用alloc, 能默认调用的只有core
extern crate alloc;

use alloc::collections::VecDeque;
use kspin::SpinNoIrq;
use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};

const UART_CLOCK_FACTOR: usize = 16;
const OSC_FREQ: usize = 1_843_200;

// COM1 => IRQ4
const UART_IRQ_NUM: usize = 0x24;

// UART16550 标准端口地址 (COM1)
// const UART_BASE: PhysAddr = pa!(0x3F8);
// const UART_IRQ_NUM: usize = 4; // COM1 IRQ

/// from getchar()
/// COM1.lock()时同时禁用内核抢占和中断
/// 创建静态实例, 通过此实例执行字符读取和输出功能
/// 类似rust_os的WRITER(只输出), rust_os的输入获取由中断实现(异步和非异步都是如此)
/// - 0x3f8 is the standard I/O port address for the first serial port, also known as COM1, on a PC.
///   This port is used for communication with serial devices like modems and printers.
///   Specifically, the port range 0x3f8 through 0x3ff is associated with COM1.
/// - The standard addresses for the first four COM ports are:
///   COM1: 0x3F8
///   COM2: 0x2F8
///   COM3: 0x3E8
///   COM4: 0x2E8  
static COM1: SpinNoIrq<Uart16550> = SpinNoIrq::new(Uart16550::new(0x3f8));

// 输入缓冲区
static RECEIVE_BUFFER: SpinNoIrq<VecDeque<u8>> = SpinNoIrq::new(VecDeque::new());

bitflags::bitflags! {
    /// Line status flags
    struct LineStsFlags: u8 {
        const INPUT_FULL = 1;
        // 1 to 4 unknown
        const OUTPUT_EMPTY = 1 << 5;
        // 6 and 7 unknown
    }
}

/// 创建一个串口
///
/// - port: w/r
struct Uart16550 {
    data: Port<u8>,
    int_en: PortWriteOnly<u8>,
    fifo_ctrl: PortWriteOnly<u8>,
    line_ctrl: PortWriteOnly<u8>,
    modem_ctrl: PortWriteOnly<u8>,
    line_sts: PortReadOnly<u8>,
}

impl Uart16550 {
    const fn new(port: u16) -> Self {
        // 这里COM1 port的起始地址为0x3f8
        // (0x3f8..=0x3ff)即(0x3f8..=0x3f8+7)属于COM1串口serial port
        // 每个端口对应 16550 UART 兼容芯片的不同寄存器功能
        Self {
            // 接收缓冲寄存器 (RBR, 读), 发送保持寄存器 (THR, 写)
            data: Port::new(port),
            // 中断使能寄存器 (IER)
            int_en: PortWriteOnly::new(port + 1),
            // 中断标识寄存器 (IIR, 读), FIFO 控制寄存器 (FCR, 写)
            fifo_ctrl: PortWriteOnly::new(port + 2),
            // 线路控制寄存器LCR (设置数据格式)
            line_ctrl: PortWriteOnly::new(port + 3),
            // 调制解调器控制寄存器MCR
            modem_ctrl: PortWriteOnly::new(port + 4),
            // 线路状态寄存器LSR
            line_sts: PortReadOnly::new(port + 5),
        }
    }

    /// 串口设置
    ///
    /// - baud_rate波特率用来协调串口通信
    /// - 这里的硬件串口是由qemu模拟出来的
    fn init(&mut self, baud_rate: usize) {
        unsafe {
            // Disable interrupts, 禁用中断
            self.int_en.write(0x00);

            // 启用中断
            // self.int_en.write(0x01);

            // Enable DLAB
            self.line_ctrl.write(0x80);

            // Set maximum speed according the input baud rate by configuring DLL and DLM
            let divisor = OSC_FREQ / (baud_rate * UART_CLOCK_FACTOR);
            self.data.write((divisor & 0xff) as u8);
            self.int_en.write((divisor >> 8) as u8);

            // Disable DLAB and set data word length to 8 bits
            // 设定数据长度为1个字节
            self.line_ctrl.write(0x03);

            // Enable FIFO, clear TX/RX queues and
            // set interrupt watermark at 14 bytes
            self.fifo_ctrl.write(0xC7);

            // Mark data terminal ready, signal request to send
            // and enable auxilliary output #2 (used as interrupt line for CPU)
            self.modem_ctrl.write(0x0B);
        }
    }

    fn line_sts(&mut self) -> LineStsFlags {
        unsafe { LineStsFlags::from_bits_truncate(self.line_sts.read()) }
    }

    /// 向串口发送一个字节
    ///
    /// - 发送的效果是有qemu接收, 处理并发送到terminal打印
    /// - 这里用串口代替vga缓冲区
    fn putchar(&mut self, c: u8) {
        while !self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY) {}
        unsafe { self.data.write(c) };
    }

    /// 从串口读取一个字节
    ///
    /// - from get_char()
    /// - 效果是通过串口从qemu模拟的硬件读取一个字节
    /// - 而qemu则接受terminal输入模拟硬件行为
    /// - 这里并没有注册串口中断, 而是采用轮询的形式不断通过串口访问硬件
    /// TODO: 改为使用中断的形式, 在那之前尝试注册一个键盘中断试一试
    /// - 键盘中断不再尝试, 兼容性不佳
    /// - 注意中断编号IRQ4或0x21与port端口编号是两回事
    /// - 这里用串口代替键盘, 这样就不需要键盘中断了, 因为在arceos看来根本没有键盘硬件, 只有qemu模拟的串口硬件
    /// - 用户的键盘由qemu映射称串口硬件了
    fn getchar(&mut self) -> Option<u8> {
        // 如果数据就绪(线路状态寄存器LSR的0位为1表示就绪, 这个寄存器由qemu模拟的外部设备更改)
        if self.line_sts().contains(LineStsFlags::INPUT_FULL) {
            // 则从接收缓存寄存器data读取一个字节
            unsafe { Some(self.data.read()) }
        } else {
            None
        }
    }
}

/// Writes a byte to the console.
fn putchar(c: u8) {
    let mut uart = COM1.lock();
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
/// - from read_bytes(bytes)
/// - 这里调用 [`COM1`] 实例读取单个字符
fn getchar() -> Option<u8> {
    COM1.lock().getchar()
    // RECEIVE_BUFFER.lock().pop_front()
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
/// - from arceos_api::stdio::ax_console_read_bytes(buf)
/// - 注意这里参数改名字了
pub fn read_bytes(bytes: &mut [u8]) -> usize {
    let mut read_len = 0;

    // 每次读1个字节, 直到把buf填满为止
    // shell传入的buf长度为1
    while read_len < bytes.len() {
        // 调用getchar()
        if let Some(c) = getchar() {
            bytes[read_len] = c;
        } else {
            break;
        }
        // 每次读1个字节
        read_len += 1;
    }
    read_len
}

/// 设置波特率为115200
pub(super) fn init() {
    COM1.lock().init(115200);
    #[cfg(feature = "irq")]
    {
        // 在platform初始化的时候注册uart中断控制器
        // crate::irq::register_handler(UART_IRQ_NUM, uart_irq_handler);
        // crate::irq::set_enable(UART_IRQ_NUM, true);
    }
}

/// UART interrupt handler for x86_64
///
/// - 作用是把从串口设备读到的字符放到缓冲区里
pub fn uart_irq_handler() {
    let mut buffer = RECEIVE_BUFFER.lock();

    axlog::ax_println!("handler trigered");
    while let Some(c) = COM1.lock().getchar() {
        if buffer.len() < 1024 {
            buffer.push_back(c);
        }
    }
}
