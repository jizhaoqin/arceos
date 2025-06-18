#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[macro_use]
#[cfg(feature = "axstd")]
extern crate axstd as std;

#[cfg(not(feature = "axstd"))]
fn path_to_str(path: &impl AsRef<std::ffi::OsStr>) -> &str {
    path.as_ref().to_str().unwrap()
}

#[cfg(feature = "axstd")]
fn path_to_str(path: &str) -> &str {
    path
}

mod cmd;

#[cfg(feature = "use-ramfs")]
mod ramfs;

use std::io::prelude::*;

const LF: u8 = b'\n';
const CR: u8 = b'\r';
const DL: u8 = b'\x7f';
const BS: u8 = b'\x08';
const SPACE: u8 = b' ';

const MAX_CMD_LEN: usize = 256;

fn print_prompt() {
    print!(
        "arceos:{}$ ",
        path_to_str(&std::env::current_dir().unwrap())
    );
    std::io::stdout().flush().unwrap();
}

#[cfg_attr(feature = "axstd", unsafe(no_mangle))]
fn main() {
    // 调用ulib::axstd::io::stdin()
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // 此buffer储存从stdin得到的输入信息
    let mut buf = [0; MAX_CMD_LEN];
    // 记录光标所在位置, 正常输出是右移, stdin获得特殊字符进行特殊移动
    let mut cursor = 0;
    cmd::run_cmd("help".as_bytes());
    print_prompt();

    loop {
        // 这里的实现是一个线程阻塞io, 可以考虑修改为非阻塞io, 基于中断
        // TODO: 但目前先不考虑修改, 而是看到底层究竟是什么硬件或中断来获得用户输入
        // 一次只读取一个byte, 也就是一个u8 ascii码
        if stdin.read(&mut buf[cursor..cursor + 1]).ok() != Some(1) {
            continue;
        }

        if buf[cursor] == b'\x1b' {
            buf[cursor] = b'^';
        }
        match buf[cursor] {
            CR | LF => {
                println!();
                if cursor > 0 {
                    cmd::run_cmd(&buf[..cursor]);
                    cursor = 0;
                }
                print_prompt();
            }
            BS | DL => {
                if cursor > 0 {
                    stdout.write_all(&[BS, SPACE, BS]).unwrap();
                    cursor -= 1;
                }
            }
            0..=31 => {}
            c => {
                if cursor < MAX_CMD_LEN - 1 {
                    stdout.write_all(&[c]).unwrap();
                    cursor += 1;
                }
            }
        }
    }
}
