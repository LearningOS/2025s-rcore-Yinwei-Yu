//! File and filesystem-related syscalls
use crate::fs::{inode::ROOT_INODE, OSInode, StatMode,Stdin,Stdout};
use crate::fs::{open_file, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};
use core::any::Any;
use core::any;
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        println!("open file success!,fd={}", fd);
        fd as isize
    } else {
        println!("open file fali!");
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();

    // check legality
    if fd >= inner.fd_table.len() || inner.fd_table[fd].is_none() {
        println!("Invalid fd: {}", fd);
        return -1;
    }

    let file_node = inner.fd_table[fd].as_ref().unwrap();

    // 调用 File trait 中的 as_any 方法，获取内部对象的 &dyn Any
    let any: &dyn Any = file_node.as_any();

    // 添加调试信息
    println!("Attempting downcast for fd: {}", fd);
    println!("File type name: {:?}", any::type_name_of_val(any)); // 打印实际类型名

    let stat = if let Some(os_node) = any.downcast_ref::<OSInode>() {
        println!("Successfully downcasted to OSInode");
        // 如果是OSInode类型，获取inode信息
        let ino = os_node.get_inode_id();
        let (block_id, block_offset) = os_node.get_inode_pos();
        let nlink = ROOT_INODE.get_link_num(block_id, block_offset); 
        Stat {
            dev: 0,
            ino,
            mode: StatMode::FILE, 
            nlink,
            pad: [0; 7],
        }
    } else if any.is::<Stdin>() {
        println!("Detected Stdin");
        Stat {
            dev: 0,
            ino: 0,
            mode: StatMode::FILE,
            nlink: 1,
            pad: [0; 7], 
        }
    } else if any.is::<Stdout>() {
        println!("Detected Stdout");
        Stat {
            dev: 0,
            ino: 1,
            mode: StatMode::FILE,
            nlink: 1,
            pad: [0; 7], // 同上
        }
    } else {
        // 如果不是已知类型
        println!("Unknown file type for fd: {}", fd);
        return -1; 
    };

    // copy data from kernel space to user space
    let token = inner.get_user_token();
    let st_buffer = translated_byte_buffer(token, st as *const u8, core::mem::size_of::<Stat>());
    if st_buffer.is_empty() && core::mem::size_of::<Stat>() > 0 {
        println!("Failed to translate user buffer for stat");
        return -1; // 无法访问用户内存
    }
    let stat_ptr = &stat as *const _ as *const u8;
    let mut current_offset = 0;
    for buf_slice in st_buffer.into_iter() {
        let copy_len = buf_slice.len();
        unsafe {
            buf_slice.copy_from_slice(core::slice::from_raw_parts(
                stat_ptr.add(current_offset),
                copy_len,
            ));
        }
        current_offset += copy_len;
    }
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    let token = current_user_token();
    let old = translated_str(token, old_name);
    let new = translated_str(token, new_name);
    if old == new {
        return -1;
    }
    ROOT_INODE.link(old.as_str(), new.as_str())
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(name: *const u8) -> isize {
    let token = current_user_token();
    let name = translated_str(token, name);
    if let Some(inode) = ROOT_INODE.find(name.as_str()) {
        if ROOT_INODE.get_link_num(inode.block_id, inode.block_offset) == 1 {
            // clear data if only one link exists
            inode.clear();
        }
        return ROOT_INODE.unlink(name.as_str());
    }
    -1
}
