use crate::io::{self, BufReader, prelude::*};
use crate::sync::{Mutex, MutexGuard};

#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};

struct StdinRaw;
struct StdoutRaw;

impl Read for StdinRaw {
    /// Non-blocking read, returns number of bytes read.
    ///
    /// - from Stdin.read(buf)
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_len = 0;

        // shell传入的buf.len == 1
        // 每次读数个字节, 直到把buf填满为止
        while read_len < buf.len() {
            let len = arceos_api::stdio::ax_console_read_bytes(buf[read_len..].as_mut())?;
            // 如果第一次没有读到信息, 直接返回
            if len == 0 {
                break;
            }
            read_len += len;
        }
        Ok(read_len)
    }
}

impl Write for StdoutRaw {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        arceos_api::stdio::ax_console_write_bytes(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A handle to the standard input stream of a process.
pub struct Stdin {
    inner: &'static Mutex<BufReader<StdinRaw>>,
}

/// A locked reference to the [`Stdin`] handle.
pub struct StdinLock<'a> {
    inner: MutexGuard<'a, BufReader<StdinRaw>>,
}

impl Stdin {
    /// Locks this handle to the standard input stream, returning a readable
    /// guard.
    ///
    /// The lock is released when the returned lock goes out of scope. The
    /// returned guard also implements the [`Read`] and [`BufRead`] traits for
    /// accessing the underlying data.
    pub fn lock(&self) -> StdinLock<'static> {
        // Locks this handle with 'static lifetime. This depends on the
        // implementation detail that the underlying `Mutex` is static.
        StdinLock {
            inner: self.inner.lock(),
        }
    }

    /// Locks this handle and reads a line of input, appending it to the specified buffer.
    #[cfg(feature = "alloc")]
    pub fn read_line(&self, buf: &mut String) -> io::Result<usize> {
        self.inner.lock().read_line(buf)
    }
}

impl Read for Stdin {
    /// Block until at least one byte is read.
    ///
    /// - from shell/main.rs
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // 第一次尝试读取
        // 这里shell调用传入的buf长度为1, 只能读取一个字节
        // 这里调用StdinRaw.read方法
        let read_len = self.inner.lock().read(buf)?;
        // 如果长度为空或第一次就读到了东西则返回
        if buf.is_empty() || read_len > 0 {
            return Ok(read_len);
        }

        // 如果首次没读到东西则:
        // try again until we got something
        loop {
            let read_len = self.inner.lock().read(buf)?;
            if read_len > 0 {
                return Ok(read_len);
            }
            // 这里每次循环没有读到输入就放弃线程, 也就是阻塞当前线程直到读到为止
            // 所以理论上当前线程如果没有输入就不会做任何事
            crate::thread::yield_now();
        }
    }
}

impl Read for StdinLock<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl BufRead for StdinLock<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, n: usize) {
        self.inner.consume(n)
    }

    #[cfg(feature = "alloc")]
    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.inner.read_until(byte, buf)
    }

    #[cfg(feature = "alloc")]
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        self.inner.read_line(buf)
    }
}

/// A handle to the global standard output stream of the current process.
pub struct Stdout {
    inner: &'static Mutex<StdoutRaw>,
}

/// A locked reference to the [`Stdout`] handle.
pub struct StdoutLock<'a> {
    inner: MutexGuard<'a, StdoutRaw>,
}

impl Stdout {
    /// Locks this handle to the standard output stream, returning a writable
    /// guard.
    ///
    /// The lock is released when the returned lock goes out of scope. The
    /// returned guard also implements the `Write` trait for writing data.
    pub fn lock(&self) -> StdoutLock<'static> {
        StdoutLock {
            inner: self.inner.lock(),
        }
    }
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().flush()
    }
}

impl Write for StdoutLock<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Constructs a new handle to the standard input of the current process.
///
/// - 对比rust标准库实现
/// ```no_run
/// pub fn stdin() -> Stdin {
///     static INSTANCE: OnceLock<Mutex<BufReader<StdinRaw>>> = OnceLock::new();
///     Stdin {
///         inner: INSTANCE.get_or_init(|| {
///             Mutex::new(BufReader::with_capacity(stdio::STDIN_BUF_SIZE, stdin_raw()))
///         }),
///     }
/// ```
pub fn stdin() -> Stdin {
    static INSTANCE: Mutex<BufReader<StdinRaw>> = Mutex::new(BufReader::new(StdinRaw));
    Stdin { inner: &INSTANCE }
}

/// Constructs a new handle to the standard output of the current process.
pub fn stdout() -> Stdout {
    static INSTANCE: Mutex<StdoutRaw> = Mutex::new(StdoutRaw);
    Stdout { inner: &INSTANCE }
}

#[doc(hidden)]
pub fn __print_impl(args: core::fmt::Arguments) {
    if cfg!(feature = "smp") {
        // synchronize using the lock in axlog, to avoid interleaving
        // with kernel logs
        arceos_api::stdio::ax_console_write_fmt(args).unwrap();
    } else {
        stdout().lock().write_fmt(args).unwrap();
    }
}
